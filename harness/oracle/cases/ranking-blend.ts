/**
 * Oracle case #2: ranking blend, provider relevance floor, relative-age label.
 *
 * Drives the REAL `computeRankingBlend`, `defaultMinCosineForProvider`, and
 * `formatRelativeAge` from lib/memory/memory-weighting.ts over fixed corpora,
 * emitting NDJSON. Proves the harness pattern scales to (a) a second numeric
 * function and (b) a STRING-valued function (different equivalence shape).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/ranking-blend.ts \
 *     > /tmp/oracle-ranking.ndjson
 */

import {
  computeRankingBlend,
  defaultMinCosineForProvider,
  formatRelativeAge,
} from '@/lib/memory/memory-weighting';

const NOW = new Date('2026-06-27T12:00:00.000Z');

function mem(fields: Record<string, unknown>): any {
  return fields as unknown;
}

// Each row is tagged by `kind` so the Rust side dispatches to the right fn.
type Row =
  | { kind: 'blend'; id: string; value: number }
  | { kind: 'cosine'; id: string; value: number }
  | { kind: 'age'; id: string; value: string };

const rows: Row[] = [];

// --- computeRankingBlend(cosine, rawWeight) ---
for (const [id, cosine, raw] of [
  ['zero', 0, 0],
  ['cos-only', 1, 0],
  ['raw-only', 0, 1],
  ['mixed', 0.62, 0.4],
  ['high-both', 0.95, 0.88],
] as const) {
  rows.push({ kind: 'blend', id, value: computeRankingBlend(cosine, raw) });
}

// --- defaultMinCosineForProvider(provider) ---
for (const [id, prov] of [
  ['builtin', 'BUILTIN'],
  ['openai', 'OPENAI'],
  ['ollama', 'OLLAMA'],
  ['null', null],
  ['unknown', 'WAT'],
] as const) {
  rows.push({ kind: 'cosine', id, value: defaultMinCosineForProvider(prov as any) });
}

// --- formatRelativeAge — one input per branch boundary ---
// created N days before NOW; pick createdAt so days_old lands in each band.
const day = 86400000;
for (const [id, daysAgo] of [
  ['today', 0.5],
  ['yesterday', 1.5],
  ['days-ago', 4],
  ['last-week', 10],
  ['weeks-ago', 21],
  ['last-month', 45],
  ['months-ago', 200],
  ['one-year', 400],
  ['multi-year', 800],
  ['future-clamped', -5], // negative → clamped to 0 → "today"
] as const) {
  const createdAt = new Date(NOW.getTime() - daysAgo * day).toISOString();
  rows.push({ kind: 'age', id, value: formatRelativeAge(mem({ importance: 0.5, createdAt }), NOW) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
