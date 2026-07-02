/**
 * Oracle case (chat-orchestration wave 1.4): the greedy token-budget tail fit.
 *
 * Drives the REAL pure function from v4's
 * lib/chat/context/message-selector.ts: selectRecentMessages.
 *
 * No provider is passed (undefined) so estimateTokens resolves the plain
 * default chars-per-token (3.5) — matching
 * quilltap_core::token_estimation::DEFAULT_CHARS_PER_TOKEN, which is what the
 * Rust port is driven with in the differential test.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-selector.ts \
 *     > /tmp/oracle-message-selector.ndjson
 */

import { selectRecentMessages, type SelectableMessage } from '@/lib/chat/context/message-selector';

interface Row {
  id: string;
  messages: SelectableMessage[];
  maxTokens: number;
  out: ReturnType<typeof selectRecentMessages>;
}

const rows: Row[] = [];

const msg = (
  role: string,
  content: string,
  extra: Partial<SelectableMessage> = {},
): SelectableMessage => ({ role, content, ...extra });

type Case = [string, SelectableMessage[], number];

const cases: Case[] = [
  // Empty input.
  ['empty', [], 1000],

  // Single message, comfortably under budget.
  ['single-under-budget', [msg('USER', 'hello there')], 1000],

  // Single message that alone exceeds the budget — force-include branch.
  ['single-over-budget-force-include', [msg('USER', 'x'.repeat(500))], 5],

  // Two messages, both fit.
  [
    'two-both-fit',
    [msg('USER', 'hi'), msg('ASSISTANT', 'hello back')],
    1000,
  ],

  // Several messages, budget only allows the most recent ones (walks backwards).
  [
    'walks-backwards-tail-fit',
    [
      msg('USER', 'message one is fairly long padding padding padding'),
      msg('ASSISTANT', 'message two is fairly long padding padding padding'),
      msg('USER', 'message three'),
      msg('ASSISTANT', 'message four'),
    ],
    30,
  ],

  // Exact-boundary fit: budget exactly equals total tokens of all messages
  // (the <= boundary — v4 breaks only on strictly `>`).
  (() => {
    // Two short one-token-ish ASCII messages; we'll compute the exact budget
    // inline via a marker so the corpus is self-documenting (the harness
    // recomputes tokens on the Rust side independently, so we don't need to
    // hardcode a token count here — just give a budget large enough to fit
    // both exactly by trial: content chosen to be short and deterministic).
    const m1 = msg('USER', 'ab');
    const m2 = msg('ASSISTANT', 'cd');
    return ['exact-boundary-two-short', [m1, m2], 14] as Case;
  })(),

  // Budget fits first (older, walked-to-last) message exactly then the next
  // one would overflow by exactly 1 token.
  [
    'overflow-by-one-stops-scan',
    [
      msg('USER', 'a'.repeat(50)),
      msg('ASSISTANT', 'b'),
    ],
    6,
  ],

  // Multi-character format: name + participantId carried through, and name
  // overhead counted (`[name] ` estimated separately).
  [
    'multi-character-name-overhead',
    [
      msg('USER', 'hello', { name: 'Alice', participantId: 'p-1' }),
      msg('ASSISTANT', 'hi there', { name: 'Bob', participantId: 'p-2' }),
    ],
    1000,
  ],

  // Name present but budget only allows the last message once name overhead
  // is included.
  [
    'name-overhead-forces-truncation',
    [
      msg('USER', 'first message here padding padding', { name: 'Charlie' }),
      msg('ASSISTANT', 'second short', { name: 'Dana' }),
    ],
    12,
  ],

  // thoughtSignature preserved through selection.
  [
    'thought-signature-preserved',
    [
      msg('ASSISTANT', 'thinking...', { thoughtSignature: 'sig-abc' }),
    ],
    1000,
  ],

  // id present on input is dropped from output (only role/content/
  // thoughtSignature/name/participantId are carried).
  [
    'id-field-dropped',
    [msg('USER', 'has an id', { id: 'msg-id-123' })],
    1000,
  ],

  // Empty-content message (still counts overhead, still selected if it fits).
  ['empty-content-message', [msg('USER', '')], 1000],

  // Empty-content message alone, tiny budget — still force-included.
  ['empty-content-force-include', [msg('USER', '')], 0],

  // Many messages, budget allows only the very last one.
  [
    'many-messages-only-last-fits',
    [
      msg('USER', 'one padding padding padding padding padding'),
      msg('ASSISTANT', 'two padding padding padding padding padding'),
      msg('USER', 'three padding padding padding padding padding'),
      msg('ASSISTANT', 'four padding padding padding padding padding'),
      msg('USER', 'five'),
    ],
    5,
  ],

  // Zero budget with a nonempty last message — force include still fires,
  // truncated stays true.
  ['zero-budget-nonempty-last', [msg('USER', 'a')], 0],

  // Negative budget (defensive; v4 has no guard against this) — everything
  // overflows immediately, force-include still applies.
  ['negative-budget', [msg('USER', 'hello'), msg('ASSISTANT', 'world')], -5],

  // Unicode content (multi-byte UTF-8, single UTF-16 unit per char here) to
  // exercise the length basis via estimateTokens.
  [
    'unicode-content',
    [msg('USER', 'héllo wörld ñ'), msg('ASSISTANT', 'ok 好的')],
    1000,
  ],

  // Non-BMP astral character (surrogate pair) in content.
  [
    'astral-content',
    [msg('USER', 'emoji test 🎉🎊')],
    1000,
  ],

  // Long single message truncated: budget allows a smaller earlier message
  // but not this later huge one, so only the huge one is force-included
  // because it IS the last message overall (force-include only checks
  // selectedMessages.length === 0, i.e. NOTHING fit, not "this one didn't fit").
  [
    'huge-last-message-force-include-only-item',
    [msg('USER', 'small'), msg('ASSISTANT', 'y'.repeat(2000))],
    3,
  ],

  // Three messages where the middle one fits, but scan still starts at the
  // end and stops at the first overflow (never revisits earlier messages).
  [
    'stops-at-first-overflow-not-cheapest-fit',
    [
      msg('USER', 'tiny'),
      msg('ASSISTANT', 'z'.repeat(200)),
      msg('USER', 'also tiny'),
    ],
    10,
  ],

  // participantId null explicitly.
  [
    'participant-id-null',
    [msg('USER', 'hi', { participantId: null })],
    1000,
  ],

  // thoughtSignature null explicitly.
  [
    'thought-signature-null',
    [msg('ASSISTANT', 'hi', { thoughtSignature: null })],
    1000,
  ],

  // All fields populated together.
  [
    'all-fields-populated',
    [
      msg('USER', 'full message', {
        id: 'id-1',
        thoughtSignature: 'sig-1',
        name: 'Eve',
        participantId: 'p-99',
      }),
    ],
    1000,
  ],

  // Large corpus of messages all fitting comfortably (sanity on ordering
  // preserved after repeated unshift).
  [
    'many-messages-all-fit',
    Array.from({ length: 10 }, (_, i) => msg(i % 2 === 0 ? 'USER' : 'ASSISTANT', `msg-${i}`)),
    10000,
  ],

  // Budget of exactly 4 (the bare message overhead) with empty content and
  // no name — should fit exactly (msgTokens === 4, 0 + 4 <= 4).
  ['bare-overhead-exact-fit', [msg('USER', '')], 4],

  // Budget of 3 (one less than bare overhead) with empty content — should
  // NOT fit via the loop, force-include fires.
  ['bare-overhead-one-under', [msg('USER', '')], 3],

  // Mixed roles including a lowercase / unusual role string (role is just a
  // string in this pure function, not validated).
  [
    'unusual-role-string',
    [msg('system', 'sys msg'), msg('user', 'user msg'), msg('assistant', 'reply')],
    1000,
  ],

  // Whitespace-only content (not trimmed by this function — trimming only
  // matters in core-whisper, not here).
  ['whitespace-only-content', [msg('USER', '   ')], 1000],

  // Two-message exact boundary where the SECOND (older) message pushes total
  // right up to maxTokens with no room to spare.
  [
    'second-message-completes-budget-exactly',
    [msg('USER', 'older'), msg('ASSISTANT', 'newer')],
    18,
  ],
];

for (const [id, messages, maxTokens] of cases) {
  const out = selectRecentMessages(messages, maxTokens, undefined as any);
  rows.push({ id, messages, maxTokens, out });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
