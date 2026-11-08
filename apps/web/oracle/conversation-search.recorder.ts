/**
 * The v4-side recorder behind `src/app/chat/conversation-search.oracle.spec.ts`
 * (P4.D384, v4 `4f5047241` — in-conversation search, "find in this chat").
 *
 * `lib/chat/conversation-search.ts` is a pure, client-safe matcher v4's search
 * bar runs over the messages the open Salon already holds. v5 cannot import v4,
 * so `src/app/chat/conversation-search.ts` is a transcription and THIS corpus is
 * what proves it, byte for byte (`JSON.stringify` of every output).
 *
 * The corpus is v4's own `conversation-search.test.ts` cases plus a wider set
 * written against the real regexes: every markdown pass of `toSearchableText`
 * (fences, images, links, HTML, headings / block quotes / list markers,
 * emphasis), TOOL rows, empty content, `systemSender` rows, `limit` under the
 * total and `limit: 0`, regex metacharacters, a query of one vs two trimmed
 * characters, case folding under the `u` flag (`'İ'`, `'ß'`), astral text and an
 * astral match at the snippet edges (the snippet slices UTF-16 code units, so a
 * surrogate pair CAN be split — the corpus measures what v4 does, it does not
 * fix it), a match at index 0 / at the end / past 140, overlapping occurrences
 * (`'aa'` in `'aaaa'`), and a `speakerOf` that answers null or a name.
 *
 * `speakerOf` is a function and cannot ride a JSON row, so each `find` row
 * names a `speaker` MODE the spec rebuilds identically:
 * - `absent` — no `speakerOf` at all;
 * - `null` — `() => null`;
 * - `byId` — `m => \`speaker-${m.id}\`` (v4's own test callback);
 * - `announcer` — `m => m.systemSender ? \`Staff ${m.systemSender}\` : null`.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from the lane's pinned v4 worktree, which carries its OWN `npm ci`
 * (never a symlink of the live checkout's `node_modules`), with Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`:
 *
 * ```bash
 * PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
 * ls -ld "$PIN" && git -C "$PIN" rev-parse HEAD     # 624d0d25c…
 * F=<V5>/apps/web/src/testing/fixtures/conversation-search.oracle.ndjson
 * rm -f "$F"
 * cp <V5>/apps/web/oracle/conversation-search.recorder.ts "$PIN/"
 * cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx conversation-search.recorder.ts > "$F"
 * rm -f "$PIN/conversation-search.recorder.ts"
 * ```
 *
 * Expect 1 `constants` + 24 `text` + 6 `escape` + 19 `pattern` + 44 `find` =
 * 94 lines; a shorter file means the recorder errored and the redirect already
 * truncated the old one (the empty-file trap). `rm -f` the target first.
 */

import {
  CONVERSATION_SEARCH_MIN_QUERY,
  conversationQueryPattern,
  escapeRegExp,
  findConversationMatches,
  toSearchableText,
  type SearchableMessage,
} from '@/lib/chat/conversation-search'

type SpeakerMode = 'absent' | 'null' | 'byId' | 'announcer'

const seen = new Set<string>()
function emit(row: Record<string, unknown> & { id: string }): void {
  if (seen.has(row.id)) throw new Error(`duplicate vector label: ${row.id}`)
  seen.add(row.id)
  console.log(JSON.stringify(row))
}

function speakerFn(mode: SpeakerMode): ((m: SearchableMessage) => string | null) | undefined {
  switch (mode) {
    case 'absent':
      return undefined
    case 'null':
      return () => null
    case 'byId':
      return m => `speaker-${m.id}`
    case 'announcer':
      return m => (m.systemSender ? `Staff ${m.systemSender}` : null)
  }
}

/** v4's own test helper (`conversation-search.test.ts`). */
function msg(id: string, content: string, extra: Partial<SearchableMessage> = {}): SearchableMessage {
  return { id, role: 'ASSISTANT', content, createdAt: `2026-10-29T00:00:0${id}Z`, ...extra }
}

emit({ id: 'constants', kind: 'constants', out: { min: CONVERSATION_SEARCH_MIN_QUERY } })

// --- toSearchableText ------------------------------------------------------
const TEXTS: Array<[string, string]> = [
  ["v4 test: emphasis, links and headings", '# Title\n\na *fine* **morning** at [the club](https://x.test)'],
  ["v4 test: whitespace and a code fence", '```js\nconst  a = 1\n```'],
  ['underscore emphasis and strike', 'a __bold__ and _soft_ and ~~gone~~ word'],
  ['inline code', 'call `doThing()` now'],
  ['image down to its alt', 'see ![a grey cat](https://x.test/cat.png) there'],
  ['image with an empty alt', 'before ![](x.png) after'],
  ['link inside an image alt is not a thing', '![alt [x]](y)'],
  ['nested link text', '[outer [inner] text](u)'],
  ['HTML tags to spaces', 'a<br>b <span class="x">c</span>d'],
  ['an unclosed angle bracket stays', 'a < b and c > d'],
  ['heading levels and a seven-hash line', '###### six\n####### seven'],
  ['block quotes nested', '> one\n>> two\n   > three'],
  ['four-space indent is not a marker', '    > not a quote'],
  ['list markers', '- dash\n* star\n+ plus\n1. one\n22. twenty-two'],
  ['a marker without a following space', '-dash\n1.one'],
  ['fence with no newline after the info string', '```python'],
  ['two fences around text', 'pre\n```\ncode here\n```\npost'],
  ['tildes and lone stars', '~ single ~ and * lone * star'],
  ['CRLF and tabs collapse', 'one\r\ntwo\t\tthree   four'],
  ['the empty string', ''],
  ['only whitespace', '   \n\t  '],
  ['astral text survives', 'the 𝒻𝒾𝓃𝑒 morning 😀 smiles'],
  ['a bold heading line', '## **Bold** heading'],
  ['link with parens in the URL breaks early', '[text](https://x.test/a_(b))'],
]
for (const [label, input] of TEXTS) {
  emit({ id: `text: ${label}`, kind: 'text', input, out: toSearchableText(input) })
}

// --- escapeRegExp ----------------------------------------------------------
const ESCAPES: string[] = ['(a+b)', '.*+?^${}()|[]\\', 'plain words', '', 'a-b/c', '😀?']
for (const input of ESCAPES) {
  emit({ id: `escape: ${JSON.stringify(input)}`, kind: 'escape', input, out: escapeRegExp(input) })
}

// --- conversationQueryPattern ----------------------------------------------
/** `[query, probe]` — the probe is tested with the pattern (lastIndex reset). */
const PATTERNS: Array<[string, string]> = [
  ['a', 'a'],
  ['  a ', 'a'],
  ['ab', 'xaby'],
  [' ab ', 'xABy'],
  ['', ''],
  ['   ', '   '],
  ['(a+b)', 'x (a+b) y'],
  ['a.b', 'axb'],
  ['[x]', 'a [x] b'],
  ['^$', 'a ^$ b'],
  ['\\d', 'a \\d b'],
  ['İ', 'i̇'],
  ['İs', 'is'],
  ['ß', 'SS'],
  ['ßa', 'ẞA'],
  ['😀', '😀'],
  ['K', 'k'],
  ['\u212Ak', 'kK'],
  ['ſt', 'ST'],
]
for (const [query, probe] of PATTERNS) {
  const pattern = conversationQueryPattern(query)
  let probeHit: boolean | null = null
  if (pattern) {
    pattern.lastIndex = 0
    probeHit = pattern.test(probe)
  }
  emit({
    id: `pattern: ${JSON.stringify(query)} vs ${JSON.stringify(probe)}`,
    kind: 'pattern',
    query,
    probe,
    out: pattern ? { source: pattern.source, flags: pattern.flags, probeHit } : null,
  })
}

// --- findConversationMatches -----------------------------------------------
const V4_MESSAGES: SearchableMessage[] = [
  msg('1', 'The *fine* morning began.'),
  msg('2', '{"tool": "fine"}', { role: 'TOOL' }),
  msg('3', 'A FINE day, a fine day indeed.', { systemSender: 'host' }),
  msg('4', 'Nothing here.'),
]
const LONG = `${'x '.repeat(60)}needle${' y'.repeat(100)}`

interface FindCase {
  label: string
  messages: SearchableMessage[]
  query: string
  limit: number
  speaker: SpeakerMode
}

const pad = (n: number) => 'p'.repeat(n)

const FINDS: FindCase[] = [
  // v4's own cases
  { label: 'v4 test: newest first, TOOL skipped', messages: V4_MESSAGES, query: 'fine', limit: 10, speaker: 'absent' },
  { label: 'v4 test: emphasis spanned', messages: V4_MESSAGES, query: 'fine morning', limit: 10, speaker: 'absent' },
  { label: 'v4 test: limit 1 of 2', messages: V4_MESSAGES, query: 'fine', limit: 1, speaker: 'absent' },
  { label: 'v4 test: speaker callback', messages: V4_MESSAGES, query: 'morning', limit: 10, speaker: 'byId' },
  { label: 'v4 test: long excerpt with ellipses', messages: [msg('1', LONG)], query: 'needle', limit: 1, speaker: 'absent' },
  // limits
  { label: 'limit 0 still counts the total', messages: V4_MESSAGES, query: 'fine', limit: 0, speaker: 'absent' },
  { label: 'limit 2 of 2', messages: V4_MESSAGES, query: 'fine', limit: 2, speaker: 'absent' },
  {
    label: 'limit 2 of 5',
    messages: ['1', '2', '3', '4', '5'].map(id => msg(id, `line ${id} mentions the club`)),
    query: 'club',
    limit: 2,
    speaker: 'byId',
  },
  // query length
  { label: 'a one-character query finds nothing', messages: V4_MESSAGES, query: 'a', limit: 10, speaker: 'absent' },
  { label: 'a padded one-character query finds nothing', messages: V4_MESSAGES, query: '  a  ', limit: 10, speaker: 'absent' },
  { label: 'a two-character query', messages: V4_MESSAGES, query: 'in', limit: 10, speaker: 'absent' },
  { label: 'a padded two-character query is trimmed', messages: V4_MESSAGES, query: '  in ', limit: 10, speaker: 'absent' },
  { label: 'the empty query', messages: V4_MESSAGES, query: '', limit: 10, speaker: 'absent' },
  // skipped rows
  {
    label: 'TOOL rows only',
    messages: [msg('1', 'fine tool', { role: 'TOOL' }), msg('2', 'fine again', { role: 'TOOL' })],
    query: 'fine',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'empty content is skipped',
    messages: [msg('1', ''), msg('2', 'a fine reply')],
    query: 'fine',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'markdown-only content strips to nothing',
    messages: [msg('1', '**__**'), msg('2', '***')],
    query: '**',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'USER and SYSTEM roles are searched',
    messages: [msg('1', 'fine user', { role: 'USER' }), msg('2', 'fine system', { role: 'SYSTEM' })],
    query: 'fine',
    limit: 10,
    speaker: 'absent',
  },
  // systemSender
  {
    label: 'systemSender rows are announcements, null sender is not',
    messages: [
      msg('1', 'the lantern is lit', { systemSender: 'lantern' }),
      msg('2', 'the lantern again', { systemSender: null }),
      msg('3', 'a lantern from the host', { systemSender: '' }),
    ],
    query: 'lantern',
    limit: 10,
    speaker: 'announcer',
  },
  { label: 'speakerOf answering null', messages: V4_MESSAGES, query: 'fine', limit: 10, speaker: 'null' },
  { label: 'speakerOf naming announcers only', messages: V4_MESSAGES, query: 'fine', limit: 10, speaker: 'announcer' },
  // markdown in messages
  {
    label: 'a match through a link text',
    messages: [msg('1', 'visit [the Drones Club](https://x.test) tonight')],
    query: 'drones club',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a URL is not searchable',
    messages: [msg('1', 'visit [the club](https://example.test/path) tonight')],
    query: 'example',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'an image alt is searchable',
    messages: [msg('1', '![a grey cat](x.png) sat')],
    query: 'grey cat',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'HTML attributes are not searchable',
    messages: [msg('1', '<span class="secret">shown</span>')],
    query: 'secret',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a fenced block body is searchable, its info string is not',
    messages: [msg('1', '```python\nprint("hello")\n```'), msg('2', 'say python')],
    query: 'python',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a heading, a quote and a list',
    messages: [msg('1', '# The Plan\n> quoted plan\n- plan item\n1. plan two')],
    query: 'plan',
    limit: 10,
    speaker: 'absent',
  },
  // regex metacharacters
  {
    label: 'metacharacters are literal',
    messages: [msg('1', 'x (a+b) y and ab and aab'), msg('2', 'a.b and axb')],
    query: '(a+b)',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a dot is literal',
    messages: [msg('1', 'a.b and axb')],
    query: 'a.b',
    limit: 10,
    speaker: 'absent',
  },
  // case folding under u
  {
    label: "İ against i and İ",
    messages: [msg('1', 'İstanbul and istanbul and ISTANBUL')],
    query: 'İs',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'ß against SS and ẞ',
    messages: [msg('1', 'Straße STRASSE strasse STRAẞE')],
    query: 'ße',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'the Kelvin sign folds to k',
    messages: [msg('1', '300 K is warm, 300 k is not a unit')],
    query: '300 k',
    limit: 10,
    speaker: 'absent',
  },
  // positions
  {
    label: 'a match at index 0',
    messages: [msg('1', `needle ${pad(200)}`)],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a match at the very end',
    messages: [msg('1', `${pad(200)} needle`)],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a match at index 40 (no leading ellipsis)',
    messages: [msg('1', `${pad(40)}needle${pad(200)}`)],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a match at index 41 (leading ellipsis)',
    messages: [msg('1', `${pad(41)}needle${pad(200)}`)],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a match past 140',
    messages: [msg('1', `${pad(150)}needle${pad(10)}`)],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a match longer than the window',
    messages: [msg('1', `${pad(10)}${'ab'.repeat(100)}${pad(10)}`)],
    query: 'ab'.repeat(100),
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'a text exactly 140 long',
    messages: [msg('1', `needle${pad(134)}`)],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  // overlapping
  {
    label: "overlapping 'aa' in 'aaaa'",
    messages: [msg('1', 'aaaa'), msg('2', 'aaa')],
    query: 'aa',
    limit: 10,
    speaker: 'absent',
  },
  // astral
  {
    label: 'an astral query',
    messages: [msg('1', 'smile 😀 and 😀😀 again')],
    query: '😀😀',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'astral characters straddling the snippet start',
    // The snippet starts SNIPPET_LEAD (40) code units before the match. A pair
    // at units 40-41: a match at 82 starts the slice at 42 (after the pair), a
    // match at 81 starts it at 41 (BETWEEN the pair's halves — a lone low
    // surrogate leads); a pair at 41-42 with a match at 81 starts it on the
    // pair's high half (whole).
    messages: [
      msg('1', `${pad(40)}😀${pad(40)}needle${pad(10)}`),
      msg('2', `${pad(40)}😀${pad(39)}needle${pad(10)}`),
      msg('3', `${pad(41)}😀${pad(38)}needle${pad(10)}`),
    ],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'astral characters straddling the snippet end',
    // A match at 0 → the window ends at unit 140; a pair at 139-140 is split.
    messages: [msg('1', `needle${pad(133)}😀${pad(20)}`), msg('2', `needle${pad(134)}😀${pad(20)}`)],
    query: 'needle',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'astral mathematical letters do not fold to ASCII',
    messages: [msg('1', 'the 𝒻𝒾𝓃𝑒 morning')],
    query: 'fine',
    limit: 10,
    speaker: 'absent',
  },
  {
    label: 'createdAt passes through verbatim, even invalid',
    messages: [
      { id: 'a', role: 'ASSISTANT', content: 'fine one', createdAt: 'not a date' },
      { id: 'b', role: 'ASSISTANT', content: 'fine two', createdAt: '' },
    ],
    query: 'fine',
    limit: 10,
    speaker: 'byId',
  },
]

for (const c of FINDS) {
  const speakerOf = speakerFn(c.speaker)
  const out = findConversationMatches(c.messages, c.query, speakerOf ? { limit: c.limit, speakerOf } : { limit: c.limit })
  emit({
    id: `find: ${c.label}`,
    kind: 'find',
    messages: c.messages,
    query: c.query,
    limit: c.limit,
    speaker: c.speaker,
    out,
  })
}
