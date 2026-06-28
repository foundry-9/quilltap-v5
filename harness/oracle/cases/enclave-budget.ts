/**
 * Oracle case #7: enclave budget math (the pure subset).
 *
 * Drives the REAL pure budget functions from the v4 server's
 * lib/background-jobs/handlers/autonomous-room-turn.ts:
 *   - checkBudget(chat, now, { dailyTokenBudget, dailyTokensSpent }) — the
 *     pre-turn exhaustion verdict over wall-clock / room-turns / room-tokens /
 *     daily user-token cap.
 *   - computeBudgetProgress(chat, turns, tokens, now, { budget, spent }) — the
 *     fraction-toward-the-binding-cap that drives pacing milestones.
 *
 * Both were module-private in v4 and are now `export`ed (a behavior-preserving
 * change) so the oracle can import the REAL code rather than reimplement it.
 *
 * The stateful orchestration around them (transitionRunState, the milestone
 * bitmask writes, grace-turn granting) is NOT tier-1 material — it's Phase-3
 * state-machine territory. `lastLocalMidnightIso` is deliberately excluded too:
 * it resolves the daily-cap rollover at the instance's *local* midnight
 * (timezone + DST dependent), which is not pure ms-arithmetic.
 *
 * Times are emitted as ISO-8601 UTC strings; the Rust harness bridges them to
 * epoch-ms with the same `iso_to_ms` the fixed clock uses, so the budget
 * arithmetic is compared exactly while the ISO parse stays out of the core.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-budget.ts \
 *     > /tmp/oracle-enclave-budget.ndjson
 */

import {
  checkBudget,
  computeBudgetProgress,
} from '@/lib/background-jobs/handlers/autonomous-room-turn';
import type { ChatMetadataBase } from '@/lib/schemas/types';

// A fixed "now". All run-start instants are expressed relative to it.
const NOW = '2026-06-28T12:00:00.000Z';
const nowMs = Date.parse(NOW);
// Convenience: an ISO string `mins` minutes before NOW.
const ago = (mins: number): string => new Date(nowMs - mins * 60_000).toISOString();

// The chat-row fields checkBudget / computeBudgetProgress actually read. We cast
// a partial through `unknown` to the full type — faithful at runtime, since the
// functions only touch these keys.
interface Caps {
  budgetMaxTurns: number | null;
  budgetMaxTokens: number | null;
  budgetMaxWallClockMs: number | null;
  runStartedAt: string | null;
  runPausedAccumMs: number | null;
  runTurnsConsumed: number | null;
  runTokensConsumed: number | null;
}
const NO_CAPS: Caps = {
  budgetMaxTurns: null,
  budgetMaxTokens: null,
  budgetMaxWallClockMs: null,
  runStartedAt: null,
  runPausedAccumMs: null,
  runTurnsConsumed: null,
  runTokensConsumed: null,
};
const asChat = (c: Caps): ChatMetadataBase => c as unknown as ChatMetadataBase;

type CheckRow = {
  kind: 'check';
  id: string;
  caps: Caps;
  now: string;
  dailyTokenBudget: number | null;
  dailyTokensSpent: number;
  out: { exhausted: boolean; nextState?: string; reason?: string };
};
type ProgressRow = {
  kind: 'progress';
  id: string;
  caps: Caps;
  turnsConsumed: number;
  tokensConsumed: number;
  now: string;
  dailyBudget: number | null;
  dailySpent: number;
  out: { fraction: number; binding: string } | null;
};
type Row = CheckRow | ProgressRow;

const rows: Row[] = [];

// ---------------------------------------------------------------------------
// checkBudget — pre-turn exhaustion verdict.
// Caps overlay the NO_CAPS baseline. The wall-clock budget is 1h = 3_600_000ms.
// ---------------------------------------------------------------------------
const checkCases: Array<[string, Partial<Caps>, number | null, number]> = [
  // [id, caps overlay, dailyTokenBudget, dailyTokensSpent]
  ['none-configured', {}, null, 0],

  // Wall-clock. Budget 1h; runStartedAt anchors elapsed = now - start - paused.
  ['wall-exhausted', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(120) }, null, 0],
  ['wall-under', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(30) }, null, 0],
  ['wall-exact-boundary', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(60) }, null, 0], // elapsed == max → exhausted (>=)
  ['wall-pause-saves', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(120), runPausedAccumMs: 90 * 60_000 }, null, 0], // 120-90=30min < 60
  ['wall-set-but-no-start', { budgetMaxWallClockMs: 3_600_000, runStartedAt: null }, null, 0], // start falsy → skipped

  // Turns. Note: checkBudget has NO `> 0` guard (unlike computeBudgetProgress).
  ['turns-exhausted', { budgetMaxTurns: 5, runTurnsConsumed: 5 }, null, 0],
  ['turns-under', { budgetMaxTurns: 5, runTurnsConsumed: 4 }, null, 0],
  ['turns-null-consumed', { budgetMaxTurns: 5, runTurnsConsumed: null }, null, 0], // null → 0 < 5
  ['turns-zero-cap', { budgetMaxTurns: 0, runTurnsConsumed: 0 }, null, 0], // 0 >= 0 → exhausted (no >0 guard)

  // Room tokens.
  ['tokens-exhausted', { budgetMaxTokens: 5000, runTokensConsumed: 5000 }, null, 0],
  ['tokens-under', { budgetMaxTokens: 5000, runTokensConsumed: 4999 }, null, 0],

  // Daily user-token cap → transitions to 'paused', not 'budgetExhausted'.
  ['daily-exhausted', {}, 10_000, 10_000],
  ['daily-under', {}, 10_000, 9_999],

  // Ordering: wall-clock is checked first, so it wins when turns are also over.
  ['order-wall-beats-turns', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(120), budgetMaxTurns: 1, runTurnsConsumed: 9 }, null, 0],
  // Room caps are checked before the daily cap → a room-token exhaustion wins.
  ['order-room-beats-daily', { budgetMaxTokens: 100, runTokensConsumed: 100 }, 10, 999],
];
for (const [id, overlay, dailyTokenBudget, dailyTokensSpent] of checkCases) {
  const caps: Caps = { ...NO_CAPS, ...overlay };
  const result = checkBudget(asChat(caps), nowMs, { dailyTokenBudget, dailyTokensSpent });
  const out = result.exhausted
    ? { exhausted: true, nextState: result.nextState, reason: result.reason }
    : { exhausted: false };
  rows.push({ kind: 'check', id, caps, now: NOW, dailyTokenBudget, dailyTokensSpent, out });
}

// ---------------------------------------------------------------------------
// computeBudgetProgress — fraction toward the binding (closest-to-exhaustion)
// cap. Considers turns, then tokens, then wall-clock, then daily; ties go to
// the first considered. Skips any non-finite or negative fraction, and any cap
// that is null or <= 0 (the `> 0` guard checkBudget lacks).
// ---------------------------------------------------------------------------
const progressCases: Array<[string, Partial<Caps>, number, number, number | null, number]> = [
  // [id, caps overlay, turnsConsumed, tokensConsumed, dailyBudget, dailySpent]
  ['none-configured', {}, 3, 4500, null, 0],

  // Single binding cap each.
  ['turns-only', { budgetMaxTurns: 10 }, 3, 0, null, 0], // 0.3
  ['tokens-only', { budgetMaxTokens: 5000 }, 0, 4500, null, 0], // 0.9
  ['time-only', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(30) }, 0, 0, null, 0], // 0.5
  ['daily-only', { budgetMaxTurns: null }, 0, 0, 10_000, 7_000], // 0.7

  // Time with pause subtracted: elapsed = 120-90 = 30min over 1h → 0.5.
  ['time-pause', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(120), runPausedAccumMs: 90 * 60_000 }, 0, 0, null, 0],

  // Multiple caps: the highest fraction is binding.
  ['tokens-beats-turns', { budgetMaxTurns: 10, budgetMaxTokens: 5000 }, 3, 4500, null, 0], // 0.3 vs 0.9 → tokens
  ['turns-beats-tokens', { budgetMaxTurns: 10, budgetMaxTokens: 5000 }, 9, 1000, null, 0], // 0.9 vs 0.2 → turns

  // Tie: turns considered before tokens → turns wins at equal fraction.
  ['tie-turns-over-tokens', { budgetMaxTurns: 10, budgetMaxTokens: 100 }, 5, 50, null, 0], // both 0.5 → turns
  // Tie: per-run (turns) considered before daily → turns wins.
  ['tie-turns-over-daily', { budgetMaxTurns: 10 }, 5, 0, 100, 50], // both 0.5 → turns

  // `> 0` guard: a zero cap is skipped (would divide by zero / be meaningless).
  ['zero-turns-cap-skipped', { budgetMaxTurns: 0, budgetMaxTokens: 5000 }, 9, 500, null, 0], // turns skipped → tokens 0.1
  ['all-zero-caps', { budgetMaxTurns: 0, budgetMaxTokens: 0 }, 9, 9, null, 0], // both skipped → null

  // Negative fraction skipped: now precedes runStartedAt → elapsed < 0.
  ['negative-time-skipped', { budgetMaxWallClockMs: 3_600_000, runStartedAt: ago(-30), budgetMaxTokens: 5000 }, 0, 500, null, 0], // time skipped → tokens 0.1

  // Over budget (fraction > 1) is allowed and reported.
  ['over-budget', { budgetMaxTurns: 10 }, 13, 0, null, 0], // 1.3
];
for (const [id, overlay, turnsConsumed, tokensConsumed, dailyBudget, dailySpent] of progressCases) {
  const caps: Caps = { ...NO_CAPS, ...overlay };
  const out = computeBudgetProgress(asChat(caps), turnsConsumed, tokensConsumed, nowMs, {
    budget: dailyBudget,
    spent: dailySpent,
  });
  rows.push({ kind: 'progress', id, caps, turnsConsumed, tokensConsumed, now: NOW, dailyBudget, dailySpent, out });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
