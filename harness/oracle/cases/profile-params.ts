/**
 * P4.D79 unit 7 — `profileParams()` as widened by v4 `d9c5a1c7`.
 *
 * Drives v4's REAL helper:
 *   profileParams (lib/llm/cheap-llm.ts)
 *
 * The corpus is the cross product of parameter-bag shapes × providers ×
 * maxContext values, because every edge here is a JS coercion the Rust port has
 * to reproduce by hand:
 *
 *   - the base test is `params && typeof params === 'object'` — `null` drops
 *     out despite `typeof null === 'object'`, and ARRAYS pass;
 *   - the provider test is `=== 'OLLAMA'`, case-SENSITIVE;
 *   - `typeof maxContext === 'number' && maxContext > 0` admits `Infinity`
 *     (which then stringifies to `null`) and rejects `NaN`, `0` and negatives;
 *   - `base?.num_ctx == null` is LOOSE equality, so an explicit `null` is
 *     overwritten but `0` and `false` suppress the injection;
 *   - spreading an ARRAY base yields index keys, and overwriting an existing
 *     `num_ctx` keeps the key at its ORIGINAL position — both are key-ORDER
 *     facts, which is why the oracle emits the result as a JSON string too.
 *
 * `<Infinity>` / `<NaN>` / `<undefined>` are sentinels: NDJSON cannot carry
 * those values, so both sides map them back to the real thing.
 *
 * Run from inside the pinned v4 worktree:
 *   cd /tmp/qt-v4-pin-p4d79-aa464abf
 *   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/profile-params.ts \
 *     > /tmp/oracle-profile-params.ndjson
 */

import { profileParams } from '@/lib/llm/cheap-llm';
import type { ConnectionProfile } from '@/lib/schemas/types';

const rows: unknown[] = [];

/** Parameter-bag shapes. `<undefined>` means the key is absent entirely. */
const BAGS: Array<[string, unknown]> = [
  ['absent', '<undefined>'],
  ['null', null],
  ['empty-object', {}],
  ['single-key', { temperature: 0.4 }],
  ['multi-key', { temperature: 0.4, enable_thinking: true, max_tokens: 900 }],
  ['num_ctx-set', { temperature: 0.4, num_ctx: 8192 }],
  ['num_ctx-null', { temperature: 0.4, num_ctx: null, top_p: 0.9 }],
  ['num_ctx-zero', { num_ctx: 0 }],
  ['num_ctx-false', { num_ctx: false }],
  ['num_ctx-string', { num_ctx: '4096' }],
  ['array-empty', []],
  ['array-two', ['a', 'b']],
  ['number', 7],
  ['string', 'nope'],
  ['boolean', true],
];

const PROVIDERS = ['OLLAMA', 'ollama', 'Ollama', 'OPENAI', 'ANTHROPIC', ''];

/** maxContext values, including the two non-JSON numbers and the falsy edges. */
const MAX_CONTEXTS: Array<[string, unknown]> = [
  ['absent', '<undefined>'],
  ['null', null],
  ['positive', 32768],
  ['one', 1],
  ['zero', 0],
  ['negative', -5],
  ['float', 4096.5],
  ['infinity', '<Infinity>'],
  ['nan', '<NaN>'],
  ['string', '8192'],
];

function realValue(v: unknown): unknown {
  if (v === '<undefined>') return undefined;
  if (v === '<Infinity>') return Infinity;
  if (v === '<NaN>') return NaN;
  return v;
}

for (const [bagId, bag] of BAGS) {
  for (const provider of PROVIDERS) {
    for (const [mcId, mc] of MAX_CONTEXTS) {
      const profile: Record<string, unknown> = { provider };
      const realBag = realValue(bag);
      if (realBag !== undefined) profile.parameters = realBag;
      const realMc = realValue(mc);
      if (realMc !== undefined) profile.maxContext = realMc;

      const out = profileParams(profile as unknown as ConnectionProfile);
      rows.push({
        kind: 'params',
        id: `${bagId}/${provider || '<empty>'}/${mcId}`,
        provider,
        bag: bagId,
        maxContext: mcId,
        // `undefined` → the absent sentinel; anything else round-trips as JSON.
        out: out === undefined ? '<undefined>' : out,
        // The literal `JSON.stringify` text, so KEY ORDER is a comparand and
        // not merely a structural equality.
        outJson: out === undefined ? '<undefined>' : JSON.stringify(out),
      });
    }
  }
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
