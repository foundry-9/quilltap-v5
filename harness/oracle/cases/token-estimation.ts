/**
 * Oracle case #11 (Wave 1 / B3): character-based token estimation.
 *
 * Drives the REAL pure functions from v4's lib/tokens/token-counter.ts:
 * estimateTokens, countMessageTokens, countMessagesTokens, truncateToTokenLimit,
 * getContextUsagePercent, getContextWarningLevel. All exported in v4.
 *
 * All rows omit the provider, so the estimator uses the default 3.5
 * chars-per-token (no plugin-registry dependency). The Rust port takes that rate
 * as a parameter and the test passes 3.5. formatTokenCount is NOT covered here —
 * it uses toFixed and is deferred to the formatting unit.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/token-estimation.ts \
 *     > /tmp/oracle-token-estimation.ndjson
 */

import {
  estimateTokens,
  countMessageTokens,
  countMessagesTokens,
  truncateToTokenLimit,
  getContextUsagePercent,
  getContextWarningLevel,
} from '@/lib/tokens/token-counter';

type Msg = { role: string; content: string };

type Row =
  | { kind: 'estimate'; id: string; text: string; out: number }
  | { kind: 'message'; id: string; role: string; content: string; out: number }
  | { kind: 'conversation'; id: string; messages: Msg[]; out: number }
  | { kind: 'truncate'; id: string; text: string; maxTokens: number; suffix: string; out: string }
  | { kind: 'usage'; id: string; usedTokens: number; contextLimit: number; out: number }
  | { kind: 'warning'; id: string; usedTokens: number; contextLimit: number; out: string };

const rows: Row[] = [];

// estimateTokens(text) — default 3.5 cpt, ceil + 5% buffer. Includes a UTF-16
// length case (emoji is a surrogate pair → .length 2).
const estCases: Array<[string, string]> = [
  ['empty', ''],
  ['short', 'hello'],
  ['exact-7', 'abcdefg'], // 7/3.5 = 2 → ceil(2*1.05)=ceil(2.1)=3
  ['sentence', 'The quick brown fox jumps over the lazy dog.'],
  ['unicode-emoji', '😀😀'], // .length = 4 (two surrogate pairs)
  ['accented', 'café résumé'],
];
for (const [id, text] of estCases) {
  rows.push({ kind: 'estimate', id, text, out: estimateTokens(text) });
}

// countMessageTokens({role, content})
const msgCases: Array<[string, string, string]> = [
  ['user-hi', 'user', 'Hello there, how are you?'],
  ['empty-content', 'assistant', ''],
  ['system', 'system', 'You are a helpful assistant.'],
];
for (const [id, role, content] of msgCases) {
  rows.push({ kind: 'message', id, role, content, out: countMessageTokens({ role, content }) });
}

// countMessagesTokens([...])
const convCases: Array<[string, Msg[]]> = [
  ['empty', []],
  ['two', [{ role: 'user', content: 'Hi' }, { role: 'assistant', content: 'Hello! How can I help?' }]],
  ['three', [
    { role: 'system', content: 'Be terse.' },
    { role: 'user', content: 'What is 2+2?' },
    { role: 'assistant', content: '4' },
  ]],
];
for (const [id, messages] of convCases) {
  rows.push({ kind: 'conversation', id, messages, out: countMessagesTokens(messages) });
}

// truncateToTokenLimit(text, maxTokens, undefined, suffix) — ASCII only.
const longText = 'The quick brown fox jumps over the lazy dog and then keeps on running far away into the distance.';
const truncCases: Array<[string, string, number, string]> = [
  ['fits', 'short text', 100, '...'], // under limit → unchanged
  ['empty', '', 100, '...'], // '' → ''
  ['word-boundary', longText, 10, '...'], // truncates at a space
  ['tiny-limit', longText, 1, '...'], // available after suffix ~0 → suffix only
  ['no-suffix', longText, 8, ''], // empty suffix
];
for (const [id, text, maxTokens, suffix] of truncCases) {
  rows.push({ kind: 'truncate', id, text, maxTokens, suffix, out: truncateToTokenLimit(text, maxTokens, undefined, suffix) });
}

// getContextUsagePercent(used, limit) — Math.round, cap 100, limit<=0 → 100.
const usageCases: Array<[string, number, number]> = [
  ['half', 50000, 100000], // 50
  ['round-up', 805, 1000], // 80.5 → 81
  ['over', 150000, 100000], // 150 → cap 100
  ['zero-limit', 100, 0], // → 100
  ['low', 1, 100000], // ~0 → 0
];
for (const [id, used, limit] of usageCases) {
  rows.push({ kind: 'usage', id, usedTokens: used, contextLimit: limit, out: getContextUsagePercent(used, limit) });
}

// getContextWarningLevel(used, limit)
const warnCases: Array<[string, number, number]> = [
  ['ok', 5000, 100000], // 5% → ok
  ['warning-80', 80000, 100000], // 80 → warning
  ['critical-95', 95000, 100000], // 95 → critical
  ['boundary-79', 79000, 100000], // 79 → ok
];
for (const [id, used, limit] of warnCases) {
  rows.push({ kind: 'warning', id, usedTokens: used, contextLimit: limit, out: getContextWarningLevel(used, limit) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
