/**
 * Oracle case: `resolveSamplingParams` — a connection profile's `parameters`
 * blob → the three per-request sampling knobs (P4.D83, v4 `d89babc4`).
 *
 * Drives the REAL `resolveSamplingParams` from the v4 server's
 * `lib/llm/sampling-params.ts` over a fixed corpus and prints one NDJSON line
 * per case. The Rust differential (`sampling_params_equivalence`) feeds the
 * IDENTICAL corpus through `quilltap_core::sampling_params` and compares
 * exactly (tier-1).
 *
 * IMPORTANT — this imports the actual app code, it does not reimplement it.
 *
 * The corpus mines v4's own 76-line unit suite
 * (`lib/llm/__tests__/sampling-params.test.ts`) and then goes past it, because
 * a Rust port can diverge where a TypeScript test never looks: JS `Number()`
 * string coercion (whitespace, hex, exponent, the empty string), `Infinity`,
 * the fall-through when the canonical key is present but unusable, and the
 * non-object blobs an unchecked cast can deliver.
 *
 * Run (Node 24, from the v4 checkout so `@/` resolves):
 *   cd ~/source/quilltap-server
 *   npx tsx <worktree>/harness/oracle/cases/sampling-params.ts \
 *     > /tmp/oracle-sampling-params.ndjson
 */

import {
  resolveSamplingParams,
  // === P4.D362 ===
  SAMPLING_PARAMETER_KEYS,
  withoutSamplingParameters,
  // === end P4.D362 ===
} from '@/lib/llm/sampling-params';

/**
 * The corpus. `params` is what the call site hands the resolver; `undefined`
 * exercises the no-blob arm. Names are stable — the Rust side matches on them.
 */
const CORPUS: Array<{ name: string; params: unknown }> = [
  // --- v4's own suite, case for case -------------------------------------
  { name: 'editor-snake-case', params: { temperature: 1, max_tokens: 16384, top_p: 0.95 } },
  { name: 'imported-camel-case', params: { temperature: 0.7, maxTokens: 4096, topP: 0.9 } },
  {
    name: 'both-spellings-snake-wins',
    params: { max_tokens: 16384, maxTokens: 4096, top_p: 0.95, topP: 0.5 },
  },
  { name: 'numeric-strings', params: { temperature: '0.8', max_tokens: '8192', top_p: '1' } },
  { name: 'only-temperature', params: { temperature: 0.7 } },
  { name: 'empty-bag', params: {} },
  { name: 'undefined-bag', params: undefined },
  { name: 'unparseable', params: { temperature: 'warm', max_tokens: null, top_p: {} } },
  { name: 'deliberate-zero', params: { temperature: 0, top_p: 0 } },
  {
    name: 'ignores-non-sampling-keys',
    params: {
      temperature: 1,
      max_tokens: 16384,
      top_p: 0.95,
      enable_thinking: true,
      request_timeout_seconds: 900,
      num_ctx: 65536,
    },
  },

  // --- past the suite: the fall-through -----------------------------------
  // `readNumber` CONTINUES past a non-finite value, so an unusable canonical
  // key still lets the camelCase spelling answer.
  { name: 'fallthrough-snake-garbage', params: { max_tokens: 'warm', maxTokens: 4096 } },
  { name: 'fallthrough-snake-null', params: { top_p: null, topP: 0.4 } },
  { name: 'fallthrough-snake-object', params: { max_tokens: {}, maxTokens: '2048' } },
  { name: 'fallthrough-both-garbage', params: { max_tokens: 'a', maxTokens: 'b' } },

  // --- past the suite: JS Number() string coercion ------------------------
  // Rust's `str::parse::<f64>` and JS `Number()` disagree on every one of
  // these; the port routes strings through the ported ToNumber.
  { name: 'string-padded', params: { temperature: '  0.5  ', max_tokens: '\t512\n' } },
  { name: 'string-empty-is-zero', params: { temperature: '' } },
  { name: 'string-whitespace-is-zero', params: { top_p: '   ' } },
  { name: 'string-hex', params: { max_tokens: '0x800' } },
  { name: 'string-binary-octal', params: { max_tokens: '0b101', top_p: '0o17' } },
  { name: 'string-exponent', params: { max_tokens: '1e3' } },
  { name: 'string-plus-signed', params: { temperature: '+0.25' } },
  { name: 'string-trailing-garbage', params: { temperature: '0.5abc' } },
  { name: 'string-infinity', params: { max_tokens: 'Infinity' } },
  { name: 'string-lowercase-infinity', params: { max_tokens: 'infinity' } },
  { name: 'string-negative-infinity', params: { temperature: '-Infinity' } },
  { name: 'string-nan-literal', params: { temperature: 'NaN' } },

  // --- past the suite: non-finite and odd number values -------------------
  { name: 'negative-values', params: { temperature: -0.5, max_tokens: -1, top_p: -0 } },
  { name: 'fractional-max-tokens', params: { max_tokens: 1000.5 } },
  { name: 'huge-max-tokens', params: { max_tokens: 1e21 } },

  // --- past the suite: value types that are not numbers or strings --------
  { name: 'boolean-values', params: { temperature: true, max_tokens: false, top_p: true } },
  { name: 'array-value', params: { max_tokens: [4096] } },
  { name: 'nested-object-value', params: { temperature: { value: 0.7 } } },

  // --- past the suite: non-object blobs (an unchecked cast can deliver
  // these; every property read answers undefined) -------------------------
  { name: 'array-bag', params: [] },
  { name: 'string-bag', params: 'nope' },
  { name: 'number-bag', params: 42 },
  { name: 'null-bag', params: null },
];

// === P4.D362 (v4 `544c7ab1e`) — `withoutSamplingParameters` + the key list ===
// One line per row, tagged `section: 'without'` (the resolver test reads only
// untagged lines). `input` is emitted with JSON.stringify — the object's own
// key order — and the Rust side compares `parameters` by serialised BYTES.
const THIRTY: Record<string, unknown> = {};
for (let i = 0; i < 30; i++) THIRTY[`k${(i * 7) % 30}`] = i;
THIRTY.top_p = 1;
THIRTY.zeta = 'z';
const WITHOUT: Array<{ name: string; params: Record<string, unknown> }> = [
  { name: 'empty', params: {} },
  { name: 'only-max-tokens', params: { max_tokens: 512 } },
  { name: 'all-five', params: { topK: 1, top_k: 2, topP: 3, top_p: 4, temperature: 5, max_tokens: 6 } },
  { name: 'removed-order', params: { top_p: 0.9, temperature: 1 } },
  { name: 'null-valued-key', params: { topP: null, max_tokens: 1 } },
  { name: 'nested-not-removed', params: { anthropic: { temperature: 1 }, x: true } },
  { name: 'thirty-keys-order', params: THIRTY },
];
// === end P4.D362 ===

function main(): void {
  for (const { name, params } of CORPUS) {
    const result = resolveSamplingParams(params as Record<string, unknown> | undefined);
    // JSON.stringify DROPS undefined-valued keys, which is exactly the
    // "absent knob" the Rust side represents as None — so a present key here
    // means the resolver produced a number.
    process.stdout.write(
      JSON.stringify({
        case: name,
        temperature: result.temperature,
        maxTokens: result.maxTokens,
        topP: result.topP,
      }) + '\n',
    );
  }
  // === P4.D362 ===
  process.stdout.write(JSON.stringify({ section: 'keys', keys: SAMPLING_PARAMETER_KEYS }) + '\n');
  for (const { name, params } of WITHOUT) {
    const { parameters, removed } = withoutSamplingParameters(params);
    process.stdout.write(
      JSON.stringify({
        section: 'without',
        case: name,
        input: params,
        parameters,
        removed,
        sameObject: parameters === params,
      }) + '\n',
    );
  }
  // === end P4.D362 ===
}

main();
