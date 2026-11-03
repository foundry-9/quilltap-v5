/**
 * Oracle case #6: context-compression sizing (the pure subset).
 *
 * Drives the REAL pure functions from the v4 server's
 * lib/chat/context/compression.ts — the budget compression-trigger predicate, the
 * sliding-window message split, and the compressed-history block builder. The
 * async LLM orchestrator (`applyContextCompression`) is deliberately NOT ported
 * here — it's a Phase-3 mocked-LLM target; only the side-effect-free sizing
 * logic is tier-1 material.
 *
 * P4.D347 (v4 `bdd09b75b`): the count-driven `shouldApplyCompression` is GONE at
 * the pin (its rows died at import), so its rows are deleted with its Rust port.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/context-compression.ts \
 *     > /tmp/oracle-context-compression.ndjson
 */

import {
  shouldApplyBudgetCompression,
  splitMessagesForCompression,
  buildCompressedHistoryBlock,
  type CompressibleMessage,
} from '@/lib/chat/context/compression';
import type { ContextCompressionSettings } from '@/lib/schemas/settings.types';

// The predicate reads only `enabled`; build a partial settings object cast to
// the full type — faithful at runtime.
function settings(enabled: boolean, windowSize = 5): ContextCompressionSettings {
  return { enabled, windowSize } as unknown as ContextCompressionSettings;
}

type Msg = { role: 'user' | 'assistant' | 'system'; content: string };

type Row =
  | { kind: 'budget'; id: string; total: number; maxAvailable: number; enabled: boolean; bypass: boolean; out: boolean }
  | { kind: 'split'; id: string; messages: Msg[]; windowSize: number; out: { messagesToCompress: Msg[]; windowMessages: Msg[] } }
  | { kind: 'block'; id: string; input: string | null; out: string | null };

const rows: Row[] = [];

// ---------------------------------------------------------------------------
// shouldApplyBudgetCompression(total, maxAvailable, settings, bypass).
// ---------------------------------------------------------------------------
const budgetCases: Array<[string, number, number, boolean, boolean]> = [
  // [id, total, maxAvailable, enabled, bypass]
  ['disabled', 999, 1, false, false],
  ['bypass', 300, 100, true, true],
  ['under', 100, 200, true, false],
  ['equal', 200, 200, true, false],
  ['over', 201, 200, true, false],
];
for (const [id, total, maxAvailable, enabled, bypass] of budgetCases) {
  rows.push({
    kind: 'budget', id, total, maxAvailable, enabled, bypass,
    out: shouldApplyBudgetCompression(total, maxAvailable, settings(enabled), bypass),
  });
}

// ---------------------------------------------------------------------------
// splitMessagesForCompression(messages, windowSize) — sliding window split.
// ---------------------------------------------------------------------------
const msg = (i: number): Msg => ({ role: i % 2 === 0 ? 'user' : 'assistant', content: `m${i}` });
const seq = (n: number): Msg[] => Array.from({ length: n }, (_, i) => msg(i));
const splitCases: Array<[string, Msg[], number]> = [
  ['empty', seq(0), 5],
  ['under-window', seq(3), 5],
  ['at-window', seq(5), 5],
  ['over-window', seq(7), 5],
  ['window-1', seq(4), 1],
];
for (const [id, messages, windowSize] of splitCases) {
  rows.push({
    kind: 'split', id, messages, windowSize,
    out: splitMessagesForCompression(messages as CompressibleMessage[], windowSize),
  });
}

// ---------------------------------------------------------------------------
// buildCompressedHistoryBlock(compressedHistory) — empty/undefined → null.
// ---------------------------------------------------------------------------
const blockCases: Array<[string, string | undefined]> = [
  ['null', undefined],
  ['empty', ''], // falsy → null, same as undefined
  ['text', 'Alice met Bob at the fair.'],
];
for (const [id, input] of blockCases) {
  rows.push({
    kind: 'block', id, input: input ?? null,
    out: buildCompressedHistoryBlock(input),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
