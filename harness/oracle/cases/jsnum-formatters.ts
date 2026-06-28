/**
 * Oracle case #27 (Wave 6 / B18): the JS `toFixed` kernel + display formatters.
 *
 * Drives:
 *   Number.prototype.toFixed — the JS primitive v4's formatters call directly
 *     (testing it head-on pins the V8 half-away-from-zero rounding the Rust
 *      port must reproduce);
 *   formatBytes               (lib/utils/format-bytes.ts)
 *   formatCostForDisplay, formatTokenCount  (lib/utils/format-tokens.ts)
 *   formatTokenCount          (lib/tokens/token-counter.ts — lowercase-k twin)
 *
 * Non-finite and negative-zero inputs can't ride through JSON as numbers, so
 * they are tagged as strings ("NaN" / "Infinity" / "-Infinity" / "-0") and the
 * Rust side maps them back to the f64 constant.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/jsnum-formatters.ts \
 *     > /tmp/oracle-jsnum-formatters.ndjson
 */

import { formatBytes } from '@/lib/utils/format-bytes';
import { formatCostForDisplay, formatTokenCount as formatTokenCountK } from '@/lib/utils/format-tokens';
import { formatTokenCount as formatTokenCountLower } from '@/lib/tokens/token-counter';

const rows: unknown[] = [];

// ---- toFixed kernel -------------------------------------------------------
// `value` is a JSON number, or a tag for what JSON can't carry.
const num = (tag: string): number =>
  tag === 'NaN' ? NaN : tag === 'Infinity' ? Infinity : tag === '-Infinity' ? -Infinity : -0;

const toFixedCases: Array<[number | string, number]> = [
  // exact ties → round half away from zero
  [0.5, 0], [1.5, 0], [2.5, 0], [3.5, 0], [-0.5, 0], [-1.5, 0], [-2.5, 0],
  [0.125, 2], [0.375, 2], [0.625, 2], [0.875, 2], [2.5, 1],
  // float-representation quirks (not real ties)
  [0.15, 1], [0.25, 1], [0.35, 1], [0.45, 1], [0.55, 1], [0.65, 1], [0.85, 1],
  [1.005, 2], [2.675, 2], [0.005, 2], [0.015, 2], [0.025, 2],
  // realistic formatter ranges
  [0.0001, 4], [0.005, 4], [0.009999, 4], [0.01, 3], [0.5, 3], [0.999, 3],
  [1, 2], [123.456, 2], [1.5, 1], [999.95, 1], [1023.5, 1], [488.28125, 1],
  [1 / 3, 4], [0.3333333333333333, 4], [2 / 3, 5],
  // small / zero
  [1e-7, 2], [1e-10, 4], [0, 0], [0, 2],
  // negative, including sign-preserving round-to-zero
  [-0.004, 2], [-2.5, 2], [-123.456, 1], ['-0', 2],
  // integers
  [5, 2], [100, 0], [1000000, 1],
  // non-finite
  ['NaN', 2], ['Infinity', 2], ['-Infinity', 2],
];
for (const [value, digits] of toFixedCases) {
  const v = typeof value === 'string' ? num(value) : value;
  rows.push({ kind: 'tofixed', value, digits, out: v.toFixed(digits) });
}

// ---- formatBytes ----------------------------------------------------------
const byteCases = [
  0, 1, 512, 1023, 1024, 1025, 1536, 2560, 1048575, 1048576, 1073741824,
  1099511627776, 1126999418470400, 999, 500000, -1024, -512, -1536,
];
for (const bytes of byteCases) {
  rows.push({ kind: 'bytes', bytes, out: formatBytes(bytes) });
}

// ---- formatCostForDisplay -------------------------------------------------
const costCases: Array<number | null> = [
  null, 0, 0.00001, 0.0001, 0.005, 0.009999, 0.01, 0.5, 0.999, 0.999999, 1, 2.5, 123.456, 1000,
];
for (const cost of costCases) {
  rows.push({ kind: 'cost', cost, out: formatCostForDisplay(cost) });
}

// ---- formatTokenCount (K and lowercase-k variants) ------------------------
const tokenCases = [0, 5, 999, 1000, 1500, 999999, 1000000, 2500000, 1234567, 1000000000];
for (const tokens of tokenCases) {
  rows.push({ kind: 'tokK', tokens, out: formatTokenCountK(tokens) });
  rows.push({ kind: 'tokLower', tokens, out: formatTokenCountLower(tokens) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
