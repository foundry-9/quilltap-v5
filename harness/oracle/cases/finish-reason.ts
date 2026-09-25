/**
 * Oracle case (chat-orchestration wave 1.6): provider finish-reason extraction.
 *
 * Drives the REAL pure function from v4's lib/llm/extract-finish-reason.ts:
 *   extractFinishReason. Side-effect-free; no injection needed.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/finish-reason.ts \
 *     > /path/to/oracle-finish-reason.ndjson
 */

import { extractFinishReason } from '@/lib/llm/extract-finish-reason';

type Row = { id: string; raw: unknown; out: string | null };

const cases: Array<[string, unknown]> = [
  // Non-object / null / primitives
  ['null', null],
  ['undefined', undefined],
  ['string', 'stop'],
  ['number', 42],
  ['boolean', true],
  ['empty-object', {}],
  ['unrecognized', { foo: 'bar' }],

  // OpenAI Chat Completions family: choices[0].finish_reason
  ['openai-stop', { choices: [{ finish_reason: 'stop' }] }],
  ['openai-length', { choices: [{ finish_reason: 'length' }] }],
  ['openai-content-filter', { choices: [{ finish_reason: 'content_filter' }] }],
  ['openai-tool-calls', { choices: [{ finish_reason: 'tool_calls' }] }],
  ['openai-empty-choices', { choices: [] }],
  ['openai-choices-no-fr', { choices: [{ index: 0 }] }],
  ['openai-fr-non-string', { choices: [{ finish_reason: 3 }] }],
  ['openai-fr-null', { choices: [{ finish_reason: null }] }],
  ['openai-choices-not-array', { choices: 'stop' }],
  ['zai-stop', { choices: [{ finish_reason: 'stop', message: { content: 'x' } }] }],

  // Anthropic: stop_reason
  ['anthropic-end-turn', { stop_reason: 'end_turn' }],
  ['anthropic-max-tokens', { stop_reason: 'max_tokens' }],
  ['anthropic-stop-sequence', { stop_reason: 'stop_sequence' }],
  ['anthropic-tool-use', { stop_reason: 'tool_use' }],
  ['anthropic-non-string', { stop_reason: 5 }],

  // Google: candidates[0].finishReason
  ['google-stop', { candidates: [{ finishReason: 'STOP' }] }],
  ['google-max-tokens', { candidates: [{ finishReason: 'MAX_TOKENS' }] }],
  ['google-safety', { candidates: [{ finishReason: 'SAFETY' }] }],
  ['google-recitation', { candidates: [{ finishReason: 'RECITATION' }] }],
  ['google-empty-candidates', { candidates: [] }],
  ['google-candidate-no-fr', { candidates: [{ index: 0 }] }],
  ['google-fr-non-string', { candidates: [{ finishReason: 1 }] }],

  // OpenAI Responses API / Grok Responses: status
  ['responses-completed', { status: 'completed' }],
  ['responses-incomplete', { status: 'incomplete' }],
  ['responses-failed', { status: 'failed' }],
  ['status-non-string', { status: 3 }],

  // Precedence: choices wins over stop_reason wins over candidates wins over status
  ['precedence-choices-over-stop', { choices: [{ finish_reason: 'stop' }], stop_reason: 'end_turn' }],
  ['precedence-stop-over-candidates', { stop_reason: 'end_turn', candidates: [{ finishReason: 'STOP' }] }],
  ['precedence-candidates-over-status', { candidates: [{ finishReason: 'STOP' }], status: 'completed' }],
  ['precedence-choices-empty-fallthrough', { choices: [], stop_reason: 'end_turn' }],
  ['precedence-choices-nonstring-fallthrough', { choices: [{ finish_reason: 9 }], status: 'completed' }],
  // P4.D225 (v4 `8bd080267`): OpenRouter's streamed camelCase key, read AFTER
  // the snake one and only inside a non-empty choices array.
  ['openrouter-camel-only', { choices: [{ finishReason: 'stop' }] }],
  ['openrouter-camel-refusal', { choices: [{ finishReason: 'refusal' }] }],
  ['openrouter-snake-beats-camel', { choices: [{ finish_reason: 'tool_calls', finishReason: 'stop' }] }],
  ['openrouter-camel-non-string', { choices: [{ finishReason: 4 }], stop_reason: 'end_turn' }],
  ['openrouter-camel-null-snake', { choices: [{ finish_reason: null, finishReason: 'length' }] }],
  ['camel-empty-choices', { choices: [], finishReason: 'stop' }],
  ['camel-first-not-object', { choices: ['stop'], status: 'completed' }],
  // P4.D225: Google's `promptFeedback.blockReason` — after `stop_reason`,
  // before `candidates`, and only a NON-EMPTY string.
  ['google-block-reason-only', { promptFeedback: { blockReason: 'SAFETY' } }],
  ['google-block-reason-beats-candidates', { promptFeedback: { blockReason: 'PROHIBITED_CONTENT' }, candidates: [{ finishReason: 'STOP' }] }],
  ['google-block-reason-synth-candidate', { promptFeedback: { blockReason: 'OTHER' }, candidates: [{ finishReason: 'OTHER' }] }],
  ['google-block-reason-empty', { promptFeedback: { blockReason: '' }, candidates: [{ finishReason: 'STOP' }] }],
  ['google-block-reason-non-string', { promptFeedback: { blockReason: 2 }, status: 'completed' }],
  ['google-feedback-no-block', { promptFeedback: { safetyRatings: [] }, candidates: [{ finishReason: 'SAFETY' }] }],
  ['google-feedback-null', { promptFeedback: null, candidates: [{ finishReason: 'STOP' }] }],
  ['stop-reason-beats-block-reason', { stop_reason: 'end_turn', promptFeedback: { blockReason: 'SAFETY' } }],
  ['choices-beats-block-reason', { choices: [{ finish_reason: 'stop' }], promptFeedback: { blockReason: 'SAFETY' } }],
  ['block-reason-beats-status', { promptFeedback: { blockReason: 'BLOCKLIST' }, status: 'completed' }],
];

for (const [id, raw] of cases) {
  const row: Row = { id, raw: raw === undefined ? null : raw, out: extractFinishReason(raw) };
  process.stdout.write(JSON.stringify(row) + '\n');
}
