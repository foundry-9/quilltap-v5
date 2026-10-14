/**
 * Oracle case: memory salience / protection / age — the Phase-1 weighting
 * corpus re-aimed at v4's scoring module (P4.D289, pin `83334b9a5`).
 *
 * v4 `e0ac7493a` DELETED `lib/memory/memory-weighting.ts` (the floored
 * `calculateEffectiveWeight` has no successor — orderings moved to
 * `salience`). This case keeps the same 17-id corpus and drives the REAL
 * successors: `salience` and `protection` / `calculateProtectionScore` from
 * `lib/memory/scoring/*` and `formatMemoryAge` from
 * `lib/memory/time/age-label`, imported by FILE path (the `scoring/index.ts`
 * barrel pulls `rank.ts` → `recall/*`).
 *
 * IMPORTANT — this imports the actual app code, it does not reimplement it.
 * Run it from inside a v4 checkout (or the round's pinned worktree) so `@/`
 * aliases resolve:
 *
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-weighting.ts" > /tmp/oracle-weighting.ndjson
 *
 * The corpus is fixed in code (no randomness, no Date.now()): every case pins
 * an explicit `now` and explicit memory fields, so the oracle is reproducible
 * and the Rust side hardcodes the identical corpus
 * (`quilltap-harness/src/lib.rs` `corpus()`).
 */

import { salience } from '@/lib/memory/scoring/salience';
import {
  calculateProtectionScore,
  protection,
  DEFAULT_PROTECTION_CONFIG,
} from '@/lib/memory/scoring/protection';
import { formatMemoryAge } from '@/lib/memory/time/age-label';

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
  // ── Episodic spine (v4 8bf3cb5f): the kind bonus + the event-clock age ──
  { id: 'episodic-old-low', m: mem({
      importance: 0.2, createdAt: '2025-09-01T00:00:00.000Z',
      kind: 'episodic' }) }, // the exact delete-first profile the bonus protects
  { id: 'episodic-clamp', m: mem({
      importance: 1.0, createdAt: '2026-06-27T00:00:00.000Z',
      reinforcementCount: 64, relatedMemoryIds: ['a','b','c','d'],
      lastAccessedAt: '2026-06-26T00:00:00.000Z',
      kind: 'episodic' }) }, // bonus present but min(1,·) clamps
  { id: 'semantic-explicit', m: mem({
      importance: 0.5, createdAt: '2026-06-01T00:00:00.000Z',
      kind: 'semantic' }) }, // explicit semantic → bonus 0
  { id: 'event-clock-age', m: mem({
      importance: 0.5, createdAt: '2026-06-25T00:00:00.000Z',
      occurredAt: '2026-05-20T00:00:00.000Z', kind: 'episodic' }) },
      // age reads the EVENT clock; decay stays on the write clock
  { id: 'event-clock-age-reinforced', m: mem({
      importance: 0.5, createdAt: '2026-01-10T00:00:00.000Z',
      lastReinforcedAt: '2026-06-25T00:00:00.000Z',
      occurredAt: '2025-11-02T00:00:00.000Z', kind: 'episodic' }) },
      // reinforce bumps decay reference; the LABEL still follows occurredAt
  { id: 'event-clock-unparsable', m: mem({
      importance: 0.5, createdAt: '2026-06-25T00:00:00.000Z',
      occurredAt: 'not-a-date' }) }, // NaN → falls back to the write clock
  { id: 'event-clock-empty', m: mem({
      importance: 0.5, createdAt: '2026-06-25T00:00:00.000Z',
      occurredAt: '' }) }, // falsy → write clock
];

for (const { id, m } of CORPUS) {
  const sal = salience(m, { nowMs: NOW.getTime() });
  const p = calculateProtectionScore(m, DEFAULT_PROTECTION_CONFIG, NOW);
  const w = protection(m, { nowMs: NOW.getTime() });
  const row = {
    id,
    salience: {
      salience: sal.salience,
      base: sal.base,
      decay: sal.decay,
      daysSince: sal.daysSince,
    },
    protection: {
      score: p.score,
      contentComponent: p.contentComponent,
      reinforcementBonus: p.reinforcementBonus,
      graphDegreeBonus: p.graphDegreeBonus,
      recentAccessBonus: p.recentAccessBonus,
      episodicBonus: p.episodicBonus,
      daysSinceRefTime: p.daysSinceRefTime,
    },
    // The NEW wrapper (no corpus row carries a source, so every row takes
    // the AUTO arm: protected iff score >= PROTECTION_THRESHOLD).
    protectionWrapper: { protected: w.protected, protectedBySource: w.protectedBySource, score: w.score },
    // Episodic spine: the human-facing age label (event clock — occurredAt
    // when present/parsable, else the write/reinforce clock).
    age: formatMemoryAge(m, NOW),
  };
  process.stdout.write(JSON.stringify(row) + '\n');
}
