/**
 * Tier-1 oracle — v4's `sizeMemoryPools` and the pool constants (P4.D265, v4
 * `3f7320138` / `d58548051`; RE-AIMED by P4.D291 onto v4 Phase E's
 * `lib/memory/recall/assemble.ts`, where every one of them now lives), driven
 * through v4's REAL exports, so v5's `quilltap_core::memory_pools` (+ the
 * assemble module's own constants) is diffed rather than trusted.
 *
 *   - `sizeMemoryPools` + the four floors (`DYNAMIC_HEAD_TOKEN_BUDGET`,
 *     `DYNAMIC_HEAD_DEFAULT_SIZE`, `RETRO_HEAD_TOKEN_BUDGET`, `RETRO_HEAD_SIZE`)
 *     + the five pool constants (`DYNAMIC_HEAD_BUDGET_RATIO`,
 *     `DYNAMIC_HEAD_MAX_TOKEN_BUDGET`, `DYNAMIC_HEAD_MAX_SIZE`,
 *     `FROZEN_ARCHIVE_MIN_SIZE`, `FROZEN_ARCHIVE_MAX_SIZE`) +
 *     `PROACTIVE_RECALL_POOL_SIZE`. The two per-entry costs
 *     (`HEAD_TOKENS_PER_ENTRY`, `ARCHIVE_TOKENS_PER_ENTRY`) are module-private
 *     in v4 and are observed only through `sizeMemoryPools`' answers.
 *   - the assembler's own caps: `INTER_CHAR_PER_CHARACTER_LIMIT`,
 *     `INTER_CHAR_DIGEST_LIMIT`, `INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT`,
 *     `RETRO_MINI_RECAP_MAX_ENTRIES`, `MEMORY_RECAP_PHASE_TIMEOUT_MS`.
 *
 * The budget corpus walks every clamp edge (200 / 1200 head tokens at 1,333 /
 * 8,000; the 25 / 60 archive bounds; a fractional budget — v4 floors it; a
 * negative one — v4 clamps it to 0; 1e9) under BOTH `retrospective` values.
 * NaN is excluded: every v4 caller passes a finite budget (the replay route
 * floors a finite, positive `memoryBudget`; the context builders compute one
 * from integer token counts), so `Math.max(0, NaN)`'s NaN never reaches it.
 *
 * Output: one NDJSON line `{case: "memory-pools", constants, cases: [{budget,
 * retro, sizing}]}`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-memory-pools.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/memory-pools.ts > /tmp/oracle-memory-pools.ndjson
 */

import * as injector from '@/lib/memory/recall/assemble';

const BUDGETS = [
  0, 1, 199, 200, 1333, 1334, 1999.5, 2000, 2001, 4000, 7999, 8000, 8001, 100000, -5, 1e9,
];

async function main(): Promise<void> {
  const constants = {
    DYNAMIC_HEAD_TOKEN_BUDGET: injector.DYNAMIC_HEAD_TOKEN_BUDGET,
    DYNAMIC_HEAD_DEFAULT_SIZE: injector.DYNAMIC_HEAD_DEFAULT_SIZE,
    RETRO_HEAD_TOKEN_BUDGET: injector.RETRO_HEAD_TOKEN_BUDGET,
    RETRO_HEAD_SIZE: injector.RETRO_HEAD_SIZE,
    DYNAMIC_HEAD_BUDGET_RATIO: injector.DYNAMIC_HEAD_BUDGET_RATIO,
    DYNAMIC_HEAD_MAX_TOKEN_BUDGET: injector.DYNAMIC_HEAD_MAX_TOKEN_BUDGET,
    DYNAMIC_HEAD_MAX_SIZE: injector.DYNAMIC_HEAD_MAX_SIZE,
    FROZEN_ARCHIVE_MIN_SIZE: injector.FROZEN_ARCHIVE_MIN_SIZE,
    FROZEN_ARCHIVE_MAX_SIZE: injector.FROZEN_ARCHIVE_MAX_SIZE,
    PROACTIVE_RECALL_POOL_SIZE: injector.PROACTIVE_RECALL_POOL_SIZE,
    INTER_CHAR_PER_CHARACTER_LIMIT: injector.INTER_CHAR_PER_CHARACTER_LIMIT,
    INTER_CHAR_DIGEST_LIMIT: injector.INTER_CHAR_DIGEST_LIMIT,
    INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT: injector.INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT,
    RETRO_MINI_RECAP_MAX_ENTRIES: injector.RETRO_MINI_RECAP_MAX_ENTRIES,
    MEMORY_RECAP_PHASE_TIMEOUT_MS: injector.MEMORY_RECAP_PHASE_TIMEOUT_MS,
  };
  const cases: Array<{ budget: number; retro: boolean; sizing: unknown }> = [];
  for (const budget of BUDGETS) {
    for (const retro of [false, true]) {
      cases.push({ budget, retro, sizing: injector.sizeMemoryPools(budget, retro) });
    }
  }
  process.stdout.write(JSON.stringify({ case: 'memory-pools', constants, cases }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-pools oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
