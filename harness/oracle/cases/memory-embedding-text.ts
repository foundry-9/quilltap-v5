/**
 * Tier-1 ORACLE for v4's per-provider memory embedding text at the pin
 * (`83334b9a5`) — `lib/memory/index-store/embedding-text.ts` +
 * `buildMemoryAnchorLine` from `lib/memory/episodic.ts`
 * (`crate::memory_embedding_text`, P4.D289, C1-A item 7, LR-12).
 *
 * Drives v4's REAL modules by FILE path (never the `index-store` barrel,
 * which loads the vector store). Every source row is rendered in every zone
 * of ZONES through all three functions: `buildMemoryAnchorLine(view, tz)`,
 * `embeddingTextFor(source, tz)` and `lexicalEmbeddingTextFor(source)`. The
 * zone is always passed explicitly, so `getMemoryTimeZone()` is never read.
 *
 * Runs under `TZ=UTC`: v4's `Date.parse` reads a zone-less datetime in the
 * PROCESS zone, which v5 models as UTC (the ruled `ZONELESS_PARSE_AS_UTC`,
 * LR-10) — the `zoneless-datetime` source agrees only on this leg.
 *
 * Absent keys stay absent (`undefined`), `null` stays `null` — v4 treats both
 * as no anchor.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-embedding-text.ts" > /tmp/oracle-memory-embedding-text.ndjson
 */

import {
  embeddingTextFor,
  lexicalEmbeddingTextFor,
  LEXICAL_TEXT_SHAPE_VERSION,
  type EmbeddingTextSource,
} from '@/lib/memory/index-store/embedding-text';
import { buildMemoryAnchorLine } from '@/lib/memory/episodic';

const emit = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify(row) + '\n');

const ZONES = ['UTC', 'America/Chicago', 'Asia/Kolkata', 'Pacific/Chatham', 'Asia/Tokyo'];

const BASE = { summary: 'Lighthouse picnic', content: 'We ate sandwiches at the lighthouse.' };

// `entities` deliberately typed loose: the non-string rows exercise v4's
// runtime `typeof e === 'string'` arm.
type Source = Omit<EmbeddingTextSource, 'entities'> & { entities?: unknown };

const SOURCES: Array<[string, Source]> = [
  // __tests__/unit/lib/memory/memory-anchor-zone.test.ts — local midnight of
  // 2026-07-14 in Tokyo stored as UTC (the day before).
  ['anchor-zone-tokyo', {
    summary: 'Lighthouse picnic',
    content: 'We ate sandwiches at the lighthouse.',
    occurredAt: '2026-07-13T15:00:00.000Z',
    narrativeTime: null,
    entities: [],
  }],
  // __tests__/unit/lib/embedding/builtin-lexical-text.test.ts
  ['builtin-lexical-memory', {
    summary: 'Lorian fears deep water',
    content: 'Lorian nearly drowned as a boy in the millpond.',
    occurredAt: '2026-07-14T12:00:00.000Z',
    narrativeTime: 'second evening',
    entities: ['Lighthouse Point', 'Bram'],
  }],
  ['builtin-lexical-other', {
    summary: 'Lorian fears deep water',
    content: 'Lorian nearly drowned as a boy in the millpond.',
    occurredAt: '2026-08-02T12:00:00.000Z',
    narrativeTime: 'second evening',
    entities: ['Harbor Inn'],
  }],
  ['builtin-lexical-garden', { summary: 'Garden', content: 'We sat outside.\n(place: the garden)' }],
  // Content ending in an anchor-like line AND real anchors: both builders.
  ['garden-anchored', {
    summary: 'Garden',
    content: 'We sat outside.\n(place: the garden)',
    occurredAt: '2026-07-14T12:00:00.000Z',
    entities: ['Harbor Inn'],
  }],
  // East of UTC: local midnight in Kolkata (+5:30) — the pre-port UTC slice
  // read 2026-07-13.
  ['kolkata-local-midnight', { ...BASE, occurredAt: '2026-07-13T18:30:00.000Z' }],
  ['kolkata-just-before', { ...BASE, occurredAt: '2026-07-13T18:29:59.999Z' }],
  // West of UTC: early-UTC instants that are the previous local day.
  ['west-early-utc', { ...BASE, occurredAt: '2026-07-14T02:00:00.000Z' }],
  ['west-chicago-midnight', { ...BASE, occurredAt: '2026-07-14T05:00:00.000Z' }],
  // A date-only occurredAt is UTC midnight to Date.parse: a day early west.
  ['date-only', { ...BASE, occurredAt: '2026-07-14' }],
  ['padded-instant', { ...BASE, occurredAt: '  2026-07-14T12:00:00.000Z \n' }],
  // DST edges: Chicago fall-back day start, spring-forward, Chatham's
  // +12:45 → +13:45 (2026-09-27 02:45 local), Kolkata no DST.
  ['dst-chicago-fall-before', { ...BASE, occurredAt: '2026-11-01T04:59:59.999Z' }],
  ['dst-chicago-fall-at', { ...BASE, occurredAt: '2026-11-01T05:00:00.000Z' }],
  ['dst-chicago-spring', { ...BASE, occurredAt: '2026-03-08T08:00:00.000Z' }],
  ['chatham-midnight-before', { ...BASE, occurredAt: '2026-09-26T11:14:59.999Z' }],
  ['dst-chatham-gap-before', { ...BASE, occurredAt: '2026-09-26T13:59:59.999Z' }],
  ['dst-chatham-gap-after', { ...BASE, occurredAt: '2026-09-26T14:00:00.000Z' }],
  ['dst-chatham-midnight-before', { ...BASE, occurredAt: '2026-09-27T10:14:59.999Z' }],
  ['dst-chatham-midnight-at', { ...BASE, occurredAt: '2026-09-27T10:15:00.000Z' }],
  ['chatham-midnight-at', { ...BASE, occurredAt: '2026-09-26T11:15:00.000Z' }],
  ['dst-chatham-end', { ...BASE, occurredAt: '2026-04-04T10:15:00.000Z' }],
  // Unparseable: slice(0, 10) that still matches the date regex …
  ['unparseable-slice-matches', { ...BASE, occurredAt: '2026-07-14xyz' }],
  ['unparseable-hour-25', { ...BASE, occurredAt: '2026-07-14T25:00:00Z' }],
  ['unparseable-month-13', { ...BASE, occurredAt: '2026-13-01' }],
  // … and ones that do not.
  ['unparseable-word', { ...BASE, occurredAt: 'yesterday' }],
  // A non-ASCII digit V8 refuses: the slice fails v4's ASCII \d (no `u` flag).
  ['unparseable-fullwidth-digit', { ...BASE, occurredAt: '2026-07-1４' }],
  ['unparseable-arabic-indic-digit', { ...BASE, occurredAt: '2026-07-1\u0661' }],
  ['unparseable-surrogate-cut', { ...BASE, occurredAt: '2026-07-1😀' }],
  // Rolled day (in-bounds day past the month end) and year edges the zoned
  // stamp renders unpadded / five-digit.
  ['rolled-feb-30', { ...BASE, occurredAt: '2026-02-30' }],
  ['year-0050', { ...BASE, occurredAt: '0050-06-01T00:00:00Z' }],
  ['year-10000', { ...BASE, occurredAt: '+010000-01-01T00:00:00Z' }],
  ['zoneless-datetime', { ...BASE, occurredAt: '2026-07-14T23:30' }],
  // Forms only V8's LEGACY Date.parse fallback reads (finite in v4). The
  // stored occurredAt is always ISO, but the anchor line reads whatever the
  // row carries — these measure the `Date.parse` port's reach.
  ['legacy-month-name', { ...BASE, occurredAt: 'July 14, 2026' }],
  ['legacy-slashes', { ...BASE, occurredAt: '2026/07/14' }],
  ['legacy-space-time', { ...BASE, occurredAt: '2026-07-14 12:00' }],
  ['legacy-unpadded', { ...BASE, occurredAt: '2026-7-14' }],
  ['legacy-to-date-string', { ...BASE, occurredAt: 'Tue Jul 14 2026' }],
  ['legacy-fullwidth', { ...BASE, occurredAt: '\uff12\uff10\uff12\uff16-07-14' }],
  // Empty / whitespace-only / null occurredAt.
  ['occurred-empty', { ...BASE, occurredAt: '' }],
  ['occurred-whitespace', { ...BASE, occurredAt: ' \t\u00a0\u3000\n' }],
  ['occurred-null', { ...BASE, occurredAt: null }],
  // narrativeTime.
  ['narrative-padded', { ...BASE, narrativeTime: '  second evening \n' }],
  ['narrative-blank', { ...BASE, narrativeTime: '\u2003 \u2028' }],
  ['narrative-only-null-others', { ...BASE, occurredAt: null, narrativeTime: 'the long winter', entities: null }],
  // Entities.
  ['entities-non-string', { ...BASE, entities: [42, 'Bram', null, { x: 1 }, true, ' Mara '] }],
  ['entities-five', { ...BASE, entities: ['A', 'B', 'C', 'D', 'E'] }],
  ['entities-blank', { ...BASE, entities: ['', '  ', '\u3000', 'Mara', '\ufeff'] }],
  ['entities-blank-then-five', { ...BASE, entities: [' ', 'A', '', 'B', 7, 'C', 'D', 'E'] }],
  ['entities-all-blank', { ...BASE, entities: ['', ' ', null] }],
  // Everything at once, with Unicode text.
  ['all-parts-unicode', {
    summary: 'Café 🌊 at dusk',
    content: 'Ślęża — “the tide” ran high.\n\n',
    occurredAt: '2026-12-31T23:59:59.999Z',
    narrativeTime: 'Year of the Tide',
    entities: ['Ślęża', 'Harbor Inn', 'Bram', 'Mara', 'Lorian'],
  }],
  ['empty-fields', { summary: '', content: '' }],
];

for (const [id, src] of SOURCES) {
  const source = src as EmbeddingTextSource;
  const view = {
    occurredAt: source.occurredAt ?? null,
    narrativeTime: source.narrativeTime ?? null,
    entities: source.entities ?? null,
  };
  for (const tz of ZONES) {
    emit({
      kind: 'text',
      id,
      tz,
      source: src,
      anchorLine: buildMemoryAnchorLine(view, tz),
      embedding: embeddingTextFor(source, tz),
      lexical: lexicalEmbeddingTextFor(source),
    });
  }
}

emit({ kind: 'constant', LEXICAL_TEXT_SHAPE_VERSION });
