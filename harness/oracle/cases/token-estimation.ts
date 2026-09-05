/**
 * Oracle case #11 (Wave 1 / B3): character-based token estimation.
 *
 * Drives the REAL pure functions from v4's lib/tokens/token-counter.ts:
 * estimateTokens, countMessageTokens, countMessagesTokens, truncateToTokenLimit,
 * countToolSchemaTokens. All exported in v4.
 *
 * P4.D157 (v4 d4138b96b, the 4.9 dead-code sweep) DELETED getContextUsagePercent
 * and getContextWarningLevel — the context-usage gauges — from token-counter.ts.
 * Their rows came out of this case with the v5 twins (neither had a production
 * caller on either side); the remaining rows are byte-identical to the pre-split
 * oracle. Do not re-add them: a named import of a deleted export makes this whole
 * case fail to LINK, emitting a ZERO-byte NDJSON.
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
  countToolSchemaTokens,
} from '@/lib/tokens/token-counter';

type Msg = { role: string; content: string };

type Row =
  | { kind: 'estimate'; id: string; text: string; out: number }
  | { kind: 'message'; id: string; role: string; content: string; out: number }
  | { kind: 'conversation'; id: string; messages: Msg[]; out: number }
  | { kind: 'truncate'; id: string; text: string; maxTokens: number; suffix: string; out: string }
  | { kind: 'toolSchema'; id: string; tools: unknown[]; out: number };

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

// countToolSchemaTokens(tools, provider) — v4 `f933ba9c` (bug 70). The tools ride
// as JSON, so the Rust side measures the SAME serialized bytes v4 measured (both
// encoders emit compact JSON with raw UTF-8 and insertion-order keys).
//
// The unserializable→0 arm is NOT shippable here: its input is a circular object,
// which is precisely what JSON.stringify cannot write, so no NDJSON row can carry
// it. v4 pins it in its own unit test; the Rust twin keeps the guard (a
// serde_json::Value tree cannot be circular, so the arm is unreachable there).
const OPENAI_STYLE_TOOL = {
  type: 'function',
  function: {
    name: 'doc_read',
    description: 'Read a document from the vault by path.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Vault-relative path to the file.' },
      },
      required: ['path'],
    },
  },
};
const ANTHROPIC_STYLE_TOOL = {
  name: 'doc_write',
  description: 'Write a document into the vault.',
  input_schema: {
    type: 'object',
    properties: { path: { type: 'string' }, content: { type: 'string' } },
  },
};
const toolCases: Array<[string, unknown[]]> = [
  ['empty', []],
  ['openai-one', [OPENAI_STYLE_TOOL]],
  ['anthropic-one', [ANTHROPIC_STYLE_TOOL]],
  ['both-shapes', [OPENAI_STYLE_TOOL, ANTHROPIC_STYLE_TOOL]],
  // Per-tool overhead is 4 each, so three trivial entries are 12 above the JSON.
  ['three-trivial', [{ name: 'a' }, { name: 'b' }, { name: 'c' }]],
  // Non-ASCII + an escape: the length basis is UTF-16 code units over the
  // SERIALIZED text, and `"` / `\` widen it.
  ['unicode-and-escapes', [{ name: 'ünïcode', description: 'a "quoted" \\ backslash — em dash 😀' }]],
  // Nulls and nested arrays survive the stringify unchanged.
  ['nulls-and-nesting', [{ name: 'n', extra: null, list: [1, 2, [3, { deep: true }]] }]],
];
for (const [id, tools] of toolCases) {
  rows.push({ kind: 'toolSchema', id, tools, out: countToolSchemaTokens(tools) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
