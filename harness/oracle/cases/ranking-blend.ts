/**
 * Oracle case #2: ranking blend, provider relevance floor, relative-age label.
 *
 * Drives the REAL `computeRankingBlend` and `defaultMinCosineForProvider`
 * (lib/memory/scoring/relevance.ts) and `formatMemoryAge`
 * (lib/memory/time/age-label.ts) over fixed corpora, emitting NDJSON. Proves
 * the harness pattern scales to (a) a second numeric function and (b) a
 * STRING-valued function (different equivalence shape).
 *
 * P4.D289 re-aimed it at the pin (`83334b9a5`): v4 `e0ac7493a` deleted
 * `lib/memory/memory-weighting.ts` and moved these, values unchanged
 * (`formatRelativeAge` → `formatMemoryAge`). Imported by FILE path — the
 * `scoring/index.ts` barrel pulls `rank.ts` → `recall/*`.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/ranking-blend.ts" > /tmp/oracle-ranking.ndjson
 */

import {
  computeRankingBlend,
  defaultMinCosineForProvider,
} from '@/lib/memory/scoring/relevance';
import { formatMemoryAge } from '@/lib/memory/time/age-label';

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

// --- computeRankingBlend(cosine, salience) ---
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
  // The ONE scale predicate (`isLexicalEmbeddingProvider`, `4ae0c4ca6`):
  // `!!provider && set.has(provider)` — empty and lower-case are neural.
  ['empty', ''],
  ['builtin-lower', 'builtin'],
] as const) {
  rows.push({ kind: 'cosine', id, value: defaultMinCosineForProvider(prov as any) });
}

// --- formatMemoryAge — one input per branch boundary ---
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
  rows.push({ kind: 'age', id, value: formatMemoryAge(mem({ importance: 0.5, createdAt }), NOW) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
