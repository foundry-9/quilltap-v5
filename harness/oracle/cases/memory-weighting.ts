/**
 * Oracle case: memory weighting / protection score.
 *
 * Drives the REAL functions from the v4 server's
 * lib/memory/memory-weighting.ts — `calculateEffectiveWeight` and
 * `calculateProtectionScore` — over a fixed, deterministic input corpus, and
 * prints the results as newline-delimited JSON on stdout. The Rust
 * differential test feeds the SAME corpus through the Rust port and asserts
 * byte-equal numeric results (tier-1 exact equivalence).
 *
 * IMPORTANT — this imports the actual app code, it does not reimplement it.
 * Run it from inside the server checkout so `@/` path aliases resolve:
 *
 *   cd ~/source/quilltap-server
 *   QT_ORACLE=~/source/quilltap-v5/harness/oracle \
 *     npx tsx "$QT_ORACLE/cases/memory-weighting.ts" > /tmp/oracle-weighting.ndjson
 *
 * (The server's tsconfig provides the `@/*` → repo-root mapping. memory-
 * weighting.ts imports `@/lib/logger`, but neither target function calls it,
 * so the logger only needs to *load*, not be configured. If the env-validation
 * chain in @/lib/env complains, set the minimal vars it requires — the harness
 * never relies on logger output.)
 *
 * The corpus is fixed in code (no randomness, no Date.now()): every case pins
 * an explicit `now` and explicit memory fields, so the oracle is reproducible
 * and the Rust side can hardcode the identical corpus.
 */

import {
  calculateEffectiveWeight,
  calculateProtectionScore,
  DEFAULT_WEIGHTING_CONFIG,
  DEFAULT_PROTECTION_CONFIG,
} from '@/lib/memory/memory-weighting';

// Fixed reference clock for all cases (UTC). Mirror this exact value in Rust.
const NOW = new Date('2026-06-27T12:00:00.000Z');

// A minimal Memory-shaped record. We only set the fields the two functions
// read; everything else is irrelevant to the math. Cast through unknown so we
// don't have to satisfy the full Memory interface in the harness.
function mem(fields: Record<string, unknown>): any {
  return fields as unknown;
}

// Deterministic corpus: spans the branch space of both functions —
// young/old, reinforced/not, floor-hit vs decay, graph degree, recent access,
// content-cap saturation, reinforcement log2 saturation.
const CORPUS = [
  { id: 'fresh-high', m: mem({
      importance: 0.9, createdAt: '2026-06-27T00:00:00.000Z' }) },
  { id: 'old-high-floor', m: mem({
      importance: 0.9, createdAt: '2025-06-27T00:00:00.000Z' }) }, // ~365d → floor
  { id: 'reinforced-recent', m: mem({
      importance: 0.5, reinforcedImportance: 0.8,
      createdAt: '2026-01-01T00:00:00.000Z',
      lastReinforcedAt: '2026-06-20T00:00:00.000Z' }) },
  { id: 'retrieval-doesnt-reset', m: mem({
      importance: 0.6, createdAt: '2026-03-01T00:00:00.000Z',
      lastAccessedAt: '2026-06-26T00:00:00.000Z' }) }, // access ≠ reinforce
  { id: 'graph-heavy', m: mem({
      importance: 0.4, createdAt: '2026-05-01T00:00:00.000Z',
      relatedMemoryIds: ['a','b','c','d','e','f','g','h'] }) }, // graph bonus caps
  { id: 'reinforce-saturate', m: mem({
      importance: 0.5, createdAt: '2026-06-10T00:00:00.000Z',
      reinforcementCount: 64 }) }, // log2 saturation
  { id: 'recent-access-bonus', m: mem({
      importance: 0.3, createdAt: '2026-04-01T00:00:00.000Z',
      lastAccessedAt: '2026-06-01T00:00:00.000Z' }) }, // within 90d
  { id: 'stale-access', m: mem({
      importance: 0.3, createdAt: '2026-01-01T00:00:00.000Z',
      lastAccessedAt: '2026-01-15T00:00:00.000Z' }) }, // outside 90d → no bonus
  { id: 'zero-importance', m: mem({
      importance: 0.0, createdAt: '2026-06-01T00:00:00.000Z' }) },
  { id: 'content-cap', m: mem({
      importance: 1.0, createdAt: '2026-06-27T06:00:00.000Z' }) }, // content cap 0.40
];

for (const { id, m } of CORPUS) {
  const w = calculateEffectiveWeight(m, DEFAULT_WEIGHTING_CONFIG, NOW);
  const p = calculateProtectionScore(m, DEFAULT_PROTECTION_CONFIG, NOW);
  const row = {
    id,
    weight: {
      effectiveWeight: w.effectiveWeight,
      rawWeight: w.rawWeight,
      minWeight: w.minWeight,
      timeDecayFactor: w.timeDecayFactor,
      daysOld: w.daysOld,
      baseImportance: w.baseImportance,
    },
    protection: {
      score: p.score,
      contentComponent: p.contentComponent,
      reinforcementBonus: p.reinforcementBonus,
      graphDegreeBonus: p.graphDegreeBonus,
      recentAccessBonus: p.recentAccessBonus,
      daysSinceRefTime: p.daysSinceRefTime,
    },
  };
  process.stdout.write(JSON.stringify(row) + '\n');
}
