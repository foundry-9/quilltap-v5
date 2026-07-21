/**
 * Oracle case: the episodic-spine pure helpers (P4.d12 unit 4).
 *
 * Drives the REAL functions from the v4 server's lib/memory/episodic.ts —
 * `buildMemoryAnchorLine`, `buildMemoryEmbeddingText`, `resolveWhenPhrase`,
 * `eventReferenceTimeMs` — over a fixed, deterministic input corpus, and
 * prints the results as newline-delimited JSON on stdout. The Rust
 * differential test feeds the SAME corpus through the Rust port
 * (`quilltap_core::episodic`) and asserts byte-equal strings / exact numbers
 * (tier-1 exact equivalence).
 *
 * IMPORTANT — this imports the actual app code, it does not reimplement it.
 * Run it from inside the server checkout so `@/` path aliases resolve, and
 * **pin TZ=UTC**: `resolveWhenPhrase`'s absolute-ISO branch calls
 * `Date.parse(raw)`, and a zone-less datetime (`2026-07-14t09:30:00`) parses
 * in the process's LOCAL zone. Production `occurredAt` inputs are always
 * zone-carrying repo-minted ISO strings, so the port implements UTC
 * semantics and the oracle must be generated under the same clock:
 *
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx <worktree>/harness/oracle/cases/episodic.ts > /tmp/oracle-episodic.ndjson
 *
 * The corpus is fixed in code (no randomness, no Date.now()): every case pins
 * explicit inputs, so the oracle is reproducible and the Rust side hardcodes
 * the identical corpus.
 */

import {
  buildMemoryAnchorLine,
  buildMemoryEmbeddingText,
  resolveWhenPhrase,
  eventReferenceTimeMs,
} from '@/lib/memory/episodic';

// ---------------------------------------------------------------------------
// buildMemoryAnchorLine — the (when · story time · place) line.
// ---------------------------------------------------------------------------

const ANCHOR_CASES: { id: string; view: Record<string, unknown> }[] = [
  { id: 'anchor-empty', view: {} },
  { id: 'anchor-all-null', view: { occurredAt: null, narrativeTime: null, entities: null } },
  { id: 'anchor-when-date', view: { occurredAt: '2026-07-14' } },
  { id: 'anchor-when-datetime', view: { occurredAt: '2026-07-14T09:30:00.000Z' } },
  { id: 'anchor-when-padded', view: { occurredAt: '  2026-07-14T09:30:00.000Z  ' } },
  { id: 'anchor-when-invalid', view: { occurredAt: 'not-a-date-x' } },
  { id: 'anchor-when-short', view: { occurredAt: '2026-7-4' } },
  { id: 'anchor-when-empty', view: { occurredAt: '   ' } },
  { id: 'anchor-story', view: { narrativeTime: 'the third night at sea' } },
  { id: 'anchor-story-padded', view: { narrativeTime: '  the third night at sea  ' } },
  { id: 'anchor-story-empty', view: { narrativeTime: '   ' } },
  { id: 'anchor-entities-one', view: { entities: ['Lighthouse Point'] } },
  {
    id: 'anchor-entities-cap',
    view: { entities: ['A', 'B', 'C', 'D', 'E', 'F'] },
  },
  {
    id: 'anchor-entities-trim-filter',
    view: { entities: ['  Paris  ', '', '   ', 'Bob'] },
  },
  {
    id: 'anchor-all',
    view: {
      occurredAt: '2026-07-14T21:00:00.000Z',
      narrativeTime: 'the third night at sea',
      entities: ['Lighthouse Point', 'Marta'],
    },
  },
  {
    id: 'anchor-story-and-place',
    view: { narrativeTime: 'midwinter', entities: ['The Salon'] },
  },
];

// ---------------------------------------------------------------------------
// buildMemoryEmbeddingText — the canonical embedded text.
// ---------------------------------------------------------------------------

const EMBED_CASES: {
  id: string;
  summary: string;
  content: string;
  anchors?: Record<string, unknown> | null;
}[] = [
  { id: 'embed-no-anchors-arg', summary: 'S', content: 'C' },
  { id: 'embed-null-anchors', summary: 'S', content: 'C', anchors: null },
  { id: 'embed-empty-anchors', summary: 'Sum', content: 'Body text', anchors: {} },
  {
    id: 'embed-null-fields',
    summary: 'Sum',
    content: 'Body text',
    anchors: { occurredAt: null, narrativeTime: null, entities: [] },
  },
  {
    id: 'embed-with-anchor',
    summary: 'Cafe meeting',
    content: 'Alice met Bob at the cafe',
    anchors: { occurredAt: '2026-07-14T09:30:00.000Z', entities: ['Paris'] },
  },
  {
    id: 'embed-empty-strings',
    summary: '',
    content: '',
    anchors: { narrativeTime: 'dawn' },
  },
];

// ---------------------------------------------------------------------------
// resolveWhenPhrase — deterministic when-phrase resolution.
// Anchor A: Tuesday 2026-07-14T09:30:00.000Z. Anchor B: Monday 2026-01-05.
// ---------------------------------------------------------------------------

const A = '2026-07-14T09:30:00.000Z';
const B = '2026-01-05T00:00:00.000Z';

const WHEN_CASES: { id: string; phrase: string | null | undefined; anchor: string }[] = [
  { id: 'when-null', phrase: null, anchor: A },
  { id: 'when-undefined', phrase: undefined, anchor: A },
  { id: 'when-empty', phrase: '', anchor: A },
  { id: 'when-blank', phrase: '   ', anchor: A },
  { id: 'when-bad-anchor', phrase: 'yesterday', anchor: 'not-an-anchor' },
  // Absolute ISO.
  { id: 'when-iso-date', phrase: '2026-07-10', anchor: A },
  { id: 'when-iso-datetime-z', phrase: '2026-07-10T18:45:00.000Z', anchor: A },
  { id: 'when-iso-datetime-lower', phrase: '2026-07-10t18:45:00z', anchor: A },
  { id: 'when-iso-datetime-offset', phrase: '2026-07-10T18:45:00+02:00', anchor: A },
  { id: 'when-iso-datetime-offset-nocolon', phrase: '2026-07-10T18:45:00+0200', anchor: A },
  { id: 'when-iso-datetime-nozone', phrase: '2026-07-10T18:45:00', anchor: A },
  { id: 'when-iso-bad-month', phrase: '2026-13-05', anchor: A },
  { id: 'when-iso-bad-day', phrase: '2026-02-31', anchor: A },
  // Month-day forms.
  { id: 'when-month-day-year', phrase: 'July 10, 2026', anchor: A },
  { id: 'when-month-day', phrase: 'July 10', anchor: A },
  { id: 'when-month-day-future-rolls-back', phrase: 'December 25', anchor: A },
  { id: 'when-month-day-on', phrase: 'on July 10', anchor: A },
  { id: 'when-month-day-ordinal', phrase: 'July 4th', anchor: A },
  { id: 'when-day-month', phrase: '10 July', anchor: A },
  { id: 'when-day-month-year', phrase: '10 July 2025', anchor: A },
  { id: 'when-day-of-month', phrase: '3rd of March', anchor: A },
  { id: 'when-month-day-overflow', phrase: 'February 31', anchor: A },
  { id: 'when-month-unknown', phrase: 'Smarch 5', anchor: A },
  { id: 'when-month-day-zero', phrase: 'July 0', anchor: A },
  // The anchor moment itself.
  { id: 'when-today', phrase: 'today', anchor: A },
  { id: 'when-tonight', phrase: 'tonight', anchor: A },
  { id: 'when-this-morning', phrase: 'this morning', anchor: A },
  { id: 'when-earlier', phrase: 'earlier', anchor: A },
  { id: 'when-earlier-today', phrase: 'earlier today', anchor: A },
  { id: 'when-just-now', phrase: 'just now', anchor: A },
  { id: 'when-now', phrase: 'now', anchor: A },
  { id: 'when-mixed-case', phrase: 'ToDaY', anchor: A },
  // Yesterday / relative counts.
  { id: 'when-yesterday', phrase: 'yesterday', anchor: A },
  { id: 'when-last-night', phrase: 'last night', anchor: A },
  { id: 'when-3-days-ago', phrase: '3 days ago', anchor: A },
  { id: 'when-1-day-ago', phrase: '1 day ago', anchor: A },
  { id: 'when-a-week-ago', phrase: 'a week ago', anchor: A },
  { id: 'when-two-months-ago', phrase: 'two months ago', anchor: A },
  { id: 'when-ten-years-back', phrase: 'ten years back', anchor: A },
  { id: 'when-about-2-weeks-ago', phrase: 'about 2 weeks ago', anchor: A },
  { id: 'when-around-five-days-earlier', phrase: 'around five days earlier', anchor: A },
  { id: 'when-some-days-before', phrase: 'some three days before', anchor: A },
  { id: 'when-couple-days-ago', phrase: 'couple days ago', anchor: A },
  { id: 'when-a-couple-of-days-ago', phrase: 'a couple of days ago', anchor: A },
  { id: 'when-a-few-weeks-back', phrase: 'a few weeks back', anchor: A },
  { id: 'when-zero-days-ago', phrase: '0 days ago', anchor: A },
  { id: 'when-10000-days-ago', phrase: '10000 days ago', anchor: A },
  { id: 'when-9999-days-ago', phrase: '9999 days ago', anchor: A },
  { id: 'when-unknown-word-days-ago', phrase: 'several days ago', anchor: A },
  // last week/month/year, the other day.
  { id: 'when-last-week', phrase: 'last week', anchor: A },
  { id: 'when-last-month', phrase: 'last month', anchor: A },
  { id: 'when-last-year', phrase: 'last year', anchor: A },
  { id: 'when-other-day', phrase: 'the other day', anchor: A },
  // last <weekday> — anchor A is a Tuesday; anchor B is a Monday.
  { id: 'when-last-friday', phrase: 'last friday', anchor: A },
  { id: 'when-last-tuesday', phrase: 'last tuesday', anchor: A },
  { id: 'when-last-monday-from-monday', phrase: 'last monday', anchor: B },
  { id: 'when-last-sunday-from-monday', phrase: 'last sunday', anchor: B },
  // Seasons.
  { id: 'when-last-spring', phrase: 'last spring', anchor: A },
  { id: 'when-last-autumn', phrase: 'last autumn', anchor: A },
  // Unresolvable / future.
  { id: 'when-tomorrow', phrase: 'tomorrow', anchor: A },
  { id: 'when-next-week', phrase: 'next week', anchor: A },
  { id: 'when-gibberish', phrase: 'when the moon was full', anchor: A },
  // Yearless date from an early-January anchor (rollback across year end).
  { id: 'when-dec-from-jan', phrase: 'December 31', anchor: B },
  { id: 'when-jan-from-jan', phrase: 'January 5', anchor: B },
  { id: 'when-jan-6-from-jan-5', phrase: 'January 6', anchor: B },
];

// ---------------------------------------------------------------------------
// eventReferenceTimeMs — the event clock.
// ---------------------------------------------------------------------------

const WRITE_CLOCK_MS = 1_783_000_000_000; // fixed write/reinforce clock

const REF_CASES: { id: string; occurredAt: string | null | undefined }[] = [
  { id: 'ref-null', occurredAt: null },
  { id: 'ref-undefined', occurredAt: undefined },
  { id: 'ref-empty', occurredAt: '' },
  { id: 'ref-garbage', occurredAt: 'not-a-date-at-all' },
  { id: 'ref-iso-datetime', occurredAt: '2026-07-14T09:30:00.000Z' },
  { id: 'ref-iso-date', occurredAt: '2026-07-14' },
  { id: 'ref-iso-seconds', occurredAt: '2020-01-01T00:00:00Z' },
];

function main(): void {
  const lines: string[] = [];
  for (const c of ANCHOR_CASES) {
    lines.push(
      JSON.stringify({ kind: 'anchorLine', id: c.id, result: buildMemoryAnchorLine(c.view as never) }),
    );
  }
  for (const c of EMBED_CASES) {
    lines.push(
      JSON.stringify({
        kind: 'embeddingText',
        id: c.id,
        result: buildMemoryEmbeddingText(c.summary, c.content, c.anchors as never),
      }),
    );
  }
  for (const c of WHEN_CASES) {
    lines.push(
      JSON.stringify({
        kind: 'whenPhrase',
        id: c.id,
        result: resolveWhenPhrase(c.phrase, c.anchor),
      }),
    );
  }
  for (const c of REF_CASES) {
    lines.push(
      JSON.stringify({
        kind: 'eventReferenceTimeMs',
        id: c.id,
        result: eventReferenceTimeMs(c.occurredAt, WRITE_CLOCK_MS),
      }),
    );
  }
  process.stdout.write(lines.join('\n') + '\n');
}

main();
