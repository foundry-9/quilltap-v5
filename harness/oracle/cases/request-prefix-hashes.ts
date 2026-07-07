/**
 * Oracle case (W4.7e): the LLM-request cache-prefix hashes.
 *
 * Drives the REAL pure function from v4's lib/llm/cache-prefix-hashes.ts:
 *   computeRequestPrefixHashes. Side-effect-free; no injection needed. The
 *   stableStringify SORTS object keys, the history-tail mapping renders absent
 *   optional fields as the literal `undefined`, and hashes are SHA-256 truncated
 *   to the first 16 hex characters.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/request-prefix-hashes.ts \
 *     > /tmp/oracle-request-prefix-hashes.ndjson
 */

import { computeRequestPrefixHashes } from '@/lib/llm/cache-prefix-hashes';
import type { LLMMessage } from '@/lib/llm/base';

type Row = {
  id: string;
  messages: LLMMessage[];
  tools: unknown[] | undefined;
  out: unknown;
};

const sys = (content: unknown): LLMMessage =>
  ({ role: 'system', content } as unknown as LLMMessage);
const user = (content: unknown, extra?: Partial<LLMMessage>): LLMMessage =>
  ({ role: 'user', content, ...extra } as unknown as LLMMessage);
const assistant = (content: unknown, extra?: Partial<LLMMessage>): LLMMessage =>
  ({ role: 'assistant', content, ...extra } as unknown as LLMMessage);

const cases: Array<[string, LLMMessage[], unknown[] | undefined]> = [
  // System block tiers
  ['no-messages', [], undefined],
  ['one-system', [sys('identity stack')], undefined],
  ['two-system', [sys('block one'), sys('block two')], undefined],
  ['three-system', [sys('b1'), sys('b2'), sys('b3')], undefined],
  ['four-system-caps-at-three', [sys('b1'), sys('b2'), sys('b3'), sys('b4')], undefined],
  // A non-string system content is not counted as a system block
  ['system-nonstring-content-skipped', [sys([{ type: 'text', text: 'x' }]), sys('real')], undefined],

  // Tools array (only hashed when non-empty)
  ['empty-tools-no-hash', [user('hi'), user('there')], []],
  ['tools-nonempty', [user('hi'), user('there')], [{ name: 'read', z: 1, a: 2 }]],
  ['tools-sorted-keys', [user('a'), user('b')], [{ b: 1, a: 2, nested: { y: 1, x: 2 } }]],

  // History tail (non-system minus last), only when > 1 non-system
  ['single-non-system-no-tail', [sys('s'), user('only')], undefined],
  ['two-non-system-tail-of-one', [user('u1'), assistant('a1')], undefined],
  [
    'three-non-system-tail-of-two',
    [sys('s'), user('u1'), assistant('a1'), user('u2')],
    undefined,
  ],
  // name / toolCallId / toolCalls present vs absent (undefined literal)
  [
    'tail-with-name-and-toolcalls',
    [
      user('hello', { name: 'Alice' }),
      assistant('sure', {
        toolCalls: [{ id: 'c1', type: 'function', function: { name: 'foo', arguments: '{}' } }],
      }),
      user('again'),
    ],
    undefined,
  ],
  [
    'tail-tool-result-message',
    [
      user('do it'),
      { role: 'tool', content: 'result', toolCallId: 'c1' } as unknown as LLMMessage,
      user('thanks'),
    ],
    undefined,
  ],
  // Array content in the tail (multimodal blocks) — stableStringify recurses + sorts
  [
    'tail-array-content',
    [
      user([
        { type: 'text', text: 'look' },
        { type: 'image', url: 'u', detail: 'high' },
      ]),
      assistant('ok'),
      user('more'),
    ],
    undefined,
  ],
  // Non-ASCII (BMP) content — key sort + hashing over UTF-8 bytes
  ['non-ascii-content', [sys('café ☕ Ångström'), user('héllo'), user('wörld')], undefined],
  // Everything at once
  [
    'kitchen-sink',
    [
      sys('identity'),
      sys('reminder'),
      sys('compressed history'),
      user('u1', { name: 'Bob' }),
      assistant('a1'),
      user('u2'),
    ],
    [{ name: 'search', parameters: { z: {}, a: [1, 2, 3] } }],
  ],
];

for (const [id, messages, tools] of cases) {
  const out = computeRequestPrefixHashes(messages, tools);
  const row: Row = { id, messages, tools: tools === undefined ? null : tools, out };
  process.stdout.write(JSON.stringify(row) + '\n');
}
