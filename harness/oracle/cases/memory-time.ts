/**
 * Tier-1 ORACLE for v4's memory calendar + day-reference rules at the pin
 * (`83334b9a5`) — `lib/memory/time/*` (P4.D289, C1-A item 3).
 *
 * Drives v4's REAL modules by FILE path (`time/index.ts` is I/O-free but the
 * case imports each file it exercises):
 *   lib/memory/time/calendar.ts  — validity, getMemoryTimeZone, zonedParts,
 *     zonedTimeToUtcMs (the two-pass guess), the day / hour-on-day / Monday
 *     week helpers, zonedDateStamp, zonedDayBoundsIso
 *   (the rule / clock / anchors / age sections follow below)
 *
 * Every zone is passed EXPLICITLY. The whole case runs TWICE — `TZ=UTC` and
 * `TZ=America/Chicago`, the second APPENDING — and every row records its leg
 * (the process zone). Rows that read no process state are byte-identical
 * across the legs; the only rows allowed to differ are the ones that read the
 * process zone: getMemoryTimeZone's system rung (the host fallback the Rust
 * side models) and v4's zone-less `Date.parse` (the `ZONELESS_PARSE_AS_UTC`
 * ruled divergence — v5 reads those as UTC on both legs).
 *
 * Non-finite numbers ride as strings (`"NaN"`).
 *
 * Run (both legs in ONE go — the second APPENDS):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-time.ts" > /tmp/oracle-memory-time.ndjson
 *   TZ=America/Chicago $N/npx tsx "$V5W/harness/oracle/cases/memory-time.ts" >> /tmp/oracle-memory-time.ndjson
 */

import {
  isValidTimeZone,
  getMemoryTimeZone,
  zonedParts,
  zonedTimeToUtcMs,
  startOfZonedDayMs,
  zonedTimeOnDayMs,
  startOfZonedWeekMs,
  zonedDateStamp,
  zonedDayBoundsIso,
} from '@/lib/memory/time/calendar';

const LEG = Intl.DateTimeFormat().resolvedOptions().timeZone;
const num = (x: number): number | string => (Number.isFinite(x) ? x : String(x));
const emit = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify({ leg: LEG, ...row }) + '\n');

const ZONES = [
  'UTC',
  'America/Chicago',
  'Asia/Kolkata',
  'Pacific/Chatham',
  'America/Santiago',
  'Europe/London',
  'Australia/Lord_Howe',
];

// ===== calendar: validity (LR-6) ============================================
const VALIDITY = [
  'UTC', 'utc', 'Etc/UTC', 'GMT', 'EST', 'EST5EDT', 'CST6CDT', 'PST8PDT', 'US/Central', 'us/central',
  'america/chicago', 'AMERICA/CHICAGO', 'Etc/GMT+5', 'Etc/GMT-14', 'Asia/Calcutta', 'Asia/Kolkata',
  'America/Ciudad_Juarez', 'Europe/Kyiv', 'Europe/Kiev', 'America/Nuuk', 'America/Godthab',
  'Asia/Rangoon', 'Asia/Yangon', 'Antarctica/Troll', 'ROC', 'Navajo', 'GB', 'Zulu', 'WET',
  'Pacific/Chatham', 'Australia/Lord_Howe', 'America/Santiago', 'Europe/London',
  '+05', '+0500', '+05:00', '-05:00', '-00:00', '+00:00', '+00', '-00', '+23:59', '-23:59', '+14:00',
  '-12:00', '−05:00', '+24:00', '+05:30:00', '+5', '05:00', '+05:0', 'Z', 'z', '', '  ', 'UTC ',
  ' UTC', 'Local', 'localtime', 'Mars/Olympus', 'America/Chicago/', 'Factory', 'Etc/Unknown',
  'posixrules', 'America/Chicago\u0000',
];
for (const tz of VALIDITY) emit({ kind: 'valid', tz, out: isValidTimeZone(tz) });
emit({ kind: 'valid-null', out: [isValidTimeZone(null), isValidTimeZone(undefined)] });

// ===== calendar: getMemoryTimeZone (QUILLTAP_TIMEZONE unset / set) ===========
// The cache is never set in this process (no refresh runs), so the chain is
// explicit arg → env → system. `system` is this leg's zone; the Rust side
// models the host fallback as `valid(env) ? env : system`.
const ENVS: Array<string | null> = [null, 'Asia/Kolkata', 'Mars/Olympus', ''];
const INSTANCES: Array<string | null> = [null, '', 'Europe/London', 'not/a-zone', 'us/central', '+05:30'];
for (const env of ENVS) {
  if (env === null) delete process.env.QUILLTAP_TIMEZONE;
  else process.env.QUILLTAP_TIMEZONE = env;
  for (const instance of INSTANCES) {
    emit({ kind: 'mtz', env, instance, system: LEG, out: getMemoryTimeZone(instance) });
  }
}
delete process.env.QUILLTAP_TIMEZONE;

// ===== calendar: the zoned helpers over anchors =============================
// Instants around both 2026 transitions of every DST zone in the set, local
// 23:30 / 00:30 in each, the Chicago 25-hour day (1 Nov 2026) and the
// Santiago midnight gap (6 Sep 2026).
const ANCHORS = [
  '2026-01-15T05:30:00.000Z', '2026-01-15T06:30:00.000Z', '2026-01-15T12:00:00.000Z',
  '2026-03-08T05:30:00.000Z', '2026-03-08T06:30:00.000Z', '2026-03-08T07:59:59.999Z', '2026-03-08T08:00:00.000Z',
  '2026-03-09T04:30:00.000Z',
  '2026-11-01T04:30:00.000Z', '2026-11-01T05:30:00.000Z', '2026-11-01T06:30:00.000Z', '2026-11-01T07:30:00.000Z',
  '2026-11-02T05:30:00.000Z', '2026-11-01T18:00:00.000Z',
  '2026-03-28T23:30:00.000Z', '2026-03-29T00:30:00.000Z', '2026-03-29T01:00:00.000Z', '2026-03-29T23:30:00.000Z',
  '2026-10-24T23:30:00.000Z', '2026-10-25T00:30:00.000Z', '2026-10-25T01:30:00.000Z', '2026-10-25T23:30:00.000Z',
  '2026-04-05T02:30:00.000Z', '2026-04-05T03:30:00.000Z', '2026-04-05T04:30:00.000Z',
  '2026-09-06T03:30:00.000Z', '2026-09-06T04:00:00.000Z', '2026-09-06T04:30:00.000Z', '2026-09-07T02:30:00.000Z',
  '2026-06-15T18:00:00.000Z', '2026-06-15T18:30:00.000Z', '2026-06-15T19:00:00.000Z',
  '2026-04-04T13:30:00.000Z', '2026-04-04T14:30:00.000Z', '2026-09-26T13:30:00.000Z', '2026-09-26T14:30:00.000Z',
  '2026-04-04T14:45:00.000Z', '2026-04-04T15:15:00.000Z', '2026-10-03T15:15:00.000Z', '2026-10-03T15:45:00.000Z',
  '2026-12-31T23:59:59.999Z', '2027-01-01T00:00:00.000Z', '2028-02-29T12:00:00.000Z', '2026-07-14T09:30:00.250Z',
];
for (const tz of ZONES) {
  for (const iso of ANCHORS) {
    const ms = Date.parse(iso);
    emit({
      kind: 'zoned', tz, at: iso,
      parts: zonedParts(ms, tz),
      dayStart: [-2, -1, 0, 1, 2].map(o => startOfZonedDayMs(ms, tz, o)),
      onDay: [[-1, 18], [0, 6], [0, 0], [1, 23], [-14, 0]].map(([o, h]) => zonedTimeOnDayMs(ms, tz, o, h)),
      weekStart: [-1, 0, 1].map(w => startOfZonedWeekMs(ms, tz, w)),
      stamp: zonedDateStamp(ms, tz),
    });
  }
}

// ===== calendar: zonedTimeToUtcMs grid (gaps, overlaps, overflow) ============
const WALLS: Array<[number, number, number, number]> = [
  [2026, 2, 8, 0], [2026, 2, 8, 1], [2026, 2, 8, 2], [2026, 2, 8, 3],   // Chicago spring gap 02:00
  [2026, 10, 1, 0], [2026, 10, 1, 1], [2026, 10, 1, 2],                 // Chicago overlap 01:00
  [2026, 2, 29, 0], [2026, 2, 29, 1], [2026, 2, 29, 2],                 // London gap 01:00
  [2026, 9, 25, 0], [2026, 9, 25, 1], [2026, 9, 25, 2],                 // London overlap 01:00
  [2026, 3, 4, 23], [2026, 3, 5, 0], [2026, 3, 5, 1],                   // Santiago overlap (23:00 Apr 4)
  [2026, 8, 5, 23], [2026, 8, 6, 0], [2026, 8, 6, 1],                   // Santiago midnight gap
  [2026, 3, 5, 1], [2026, 3, 5, 2], [2026, 9, 4, 2], [2026, 9, 4, 3],   // Lord Howe 30-min
  [2026, 3, 5, 2], [2026, 3, 5, 3], [2026, 8, 27, 2], [2026, 8, 27, 3], // Chatham 45-min offsets
  [2026, 0, 0, 0], [2026, 0, 32, 0], [2026, 12, 1, 0], [2026, -1, 1, 0], [2026, 1, 29, 0], [2028, 1, 29, 12],
  [2026, 5, 15, 24], [2026, 5, 15, -1], [50, 2, 3, 0], [99, 0, 1, 0], [100, 0, 1, 0], [1200, 2, 3, 0],
];
for (const tz of ZONES) {
  for (const [y, m, d, h] of WALLS) emit({ kind: 'toUtc', tz, wall: [y, m, d, h], out: num(zonedTimeToUtcMs(y, m, d, h, tz)) });
}

// ===== calendar: zonedDayBoundsIso ============================================
const DATES = [
  '2026-02-31', '2026-02-29', '2028-02-29', '0099-01-01', '0100-01-01', '2026-1-01', '2026-01-1', '2026-13-01',
  '2026-00-10', '2026-01-00', '2026-07-14', '2026-03-08', '2026-11-01', '2026-03-29', '2026-10-25',
  '2026-04-05', '2026-09-06', '2026-09-27', '2026-10-04', '9999-12-31', ' 2026-07-14', '2026-07-14T00:00',
  '２０２６-07-14',
];
for (const tz of ZONES) {
  for (const date of DATES) emit({ kind: 'bounds', tz, date, out: zonedDayBoundsIso(date, tz) });
}

// 7417e9a1b removed the MEMORY_CALENDAR_DAY_MS export; measured through the real calendar.
const MEMORY_CALENDAR_DAY_MS = zonedTimeToUtcMs(2026, 1, 2, 0, 'UTC') - zonedTimeToUtcMs(2026, 1, 1, 0, 'UTC');
emit({ kind: 'constants', MEMORY_CALENDAR_DAY_MS });

// =============================================================================
// ===== rules / clock / anchors (P4.D289 Tier-1 unit 4) =======================
// =============================================================================
// v4's REAL `lib/memory/time/{rules,clock,anchors}.ts`, by FILE path. Every
// zone is explicit except where v4 itself falls back to the process zone
// (`clockFor`'s chain and `resolveEpisodicAnchors` without `timeZone` — the
// Rust side passes the leg zone as the host fallback there). Rows that carry
// an `id` are the hand-picked zone-less `Date.parse` rows (and their negative
// controls); the Rust side's `ZONELESS_PARSE_AS_UTC` table names the ones
// whose v4 output differs between the two legs.
import {
  DAY_REFERENCE_TABLE,
  DAY_NUMBER_WORDS,
  MAX_TABLE_DAYS_AGO,
  matchDayReferenceRule,
  DAY_REFERENCE_SCAN_PATTERN,
} from '@/lib/memory/time/rules';
import {
  clockFor,
  resolveEventTime,
  resolveWhenPhrase,
  resolveReferenceWindow,
  normaliseTimeRange,
} from '@/lib/memory/time/clock';
import { resolveEpisodicAnchors } from '@/lib/memory/time/anchors';

/** v4 output, or `{ throws }` when v4 throws (the Rust side pins which). */
const attempt = (f: () => unknown): unknown => {
  try {
    return f();
  } catch (e) {
    return { throws: `${(e as Error).name}: ${(e as Error).message}` };
  }
};
const ms = (iso: string | null): number => (iso === null ? Number.NaN : Date.parse(iso));

// ----- the table itself --------------------------------------------------------
emit({
  kind: 'rule-table',
  rules: DAY_REFERENCE_TABLE.map(r => ({ name: r.name, pattern: r.pattern })),
  words: DAY_NUMBER_WORDS,
  max: MAX_TABLE_DAYS_AGO,
  scanSource: DAY_REFERENCE_SCAN_PATTERN.source,
  scanFlags: DAY_REFERENCE_SCAN_PATTERN.flags,
});

// ----- matchDayReferenceRule (anchored, /i, no u) ------------------------------
const MATCH_PHRASES = [
  'day before yesterday', 'last night', 'earlier today', 'this morning', 'this afternoon', 'this evening',
  'tonight', 'today', 'yesterday', '1 day ago', '1 days ago', '2 day ago', '01 days ago', '0 days ago',
  '99 days ago', '100 days ago', 'one days ago', 'ten days ago', 'eleven days ago', 'twelve days ago',
  'thirteen days ago', 'fourteen days ago', 'fifteen days ago', 'this week', 'last week', 'next week', 'tomorrow',
  'YESTERDAY', 'Day Before Yesterday', 'THREE DAYS AGO', '3 Days Ago', 'Last Night', 'TONIGHT',
  'ye\u017fterday', 'last wee\u212a', 'last weeK', 'the yesterday', ' yesterday', 'yesterday ', 'last  night',
  'last night', 'last\tnight', 'earlier  today', 'today yesterday', '', 'tomorrow!', 'days ago', '١ days ago',
];
for (const phrase of MATCH_PHRASES) {
  const hit = matchDayReferenceRule(phrase);
  emit({
    kind: 'rule-match',
    phrase,
    out: hit ? { name: hit.rule.name, groups: Array.from(hit.match).map(g => (g === undefined ? null : g)) } : null,
  });
}

// ----- both directions: the write lands inside the read window -----------------
// rule-table.test.ts's PAST_PHRASES + the two future rules, over its four
// anchors plus DST / midnight-ish anchors for the other zones.
const BOTH_PHRASES = [
  'day before yesterday', 'last night', 'earlier today', 'this morning', 'this afternoon', 'this evening',
  'tonight', 'today', 'yesterday', '1 day ago', '3 days ago', 'eleven days ago', 'fourteen days ago',
  'this week', 'last week', 'tomorrow', 'next week',
];
const BOTH_ANCHORS = [
  '2026-10-12T19:00:00.000Z', '2026-11-02T05:30:00.000Z', '2026-03-09T04:00:00.000Z', '2026-07-21T15:00:00.000Z',
  '2026-03-29T00:30:00.000Z', '2026-10-25T00:30:00.000Z', '2026-04-05T02:30:00.000Z', '2026-09-06T04:30:00.000Z',
  '2026-09-26T14:30:00.000Z', '2026-01-01T05:30:00.000Z', '2026-11-01T06:30:00.000Z', '2026-03-08T08:30:00.000Z',
];
for (const tz of ZONES) {
  for (const at of BOTH_ANCHORS) {
    const nowMs = Date.parse(at);
    for (const phrase of BOTH_PHRASES) {
      emit({
        kind: 'rule-both', tz, at, phrase,
        written: resolveEventTime(phrase, nowMs, { timeZone: tz }),
        read: resolveReferenceWindow(`we talked about it ${phrase}, remember`, { timeZone: tz, nowMs }),
      });
    }
  }
}

// ----- the write side (resolveEventTime) ----------------------------------------
const EVENT_PHRASES: Array<string | null> = [
  // table phrases, the `the ` strip, case, whitespace runs (incl. U+00A0)
  'yesterday', 'the yesterday', 'The Yesterday', 'THE  YESTERDAY', 'the yesterday', 'theyesterday',
  'the day before yesterday', 'day before yesterday', 'Last Night', '  last   night  ', 'last night',
  'last\tweek', 'last\nweek', 'the last week', 'this week', 'today', 'tonight', 'this morning', 'earlier today',
  'the 3 days ago', '1 day ago', 'one day ago', 'eleven days ago', 'twelve days ago', 'thirteen days ago',
  'fourteen days ago', 'Fourteen Days Ago', 'fifteen days ago', '15 days ago', '97 days ago', '0 days ago',
  '00 days ago', '01 days ago', 'tomorrow', 'next week', 'the tomorrow',
  // LR-9 on the write side: lower-cased FIRST (KELVIN SIGN U+212A → k matches; U+017F stays)
  'ye\u017fterday', 'last wee\u212a', 'last weeK', 'YESTERDAY',
  // ago family
  'two weeks ago', 'a couple of days ago', 'a couple days back', 'a few weeks earlier', 'a few days before',
  'about 3 months back', 'around two years ago', 'some 5 days earlier', 'a week ago', 'an hour ago',
  'a month ago', 'couple of days ago', 'a couplex days ago', 'about days ago', 'constructor days ago',
  '9999 years ago', '10000 days ago', '9999 days ago', '20 days ago', 'twenty days ago', 'a  few  weeks  earlier',
  // fixed phrases
  'last month', 'last year', 'the other day', 'The Other Day', 'last friday', 'last Friday', 'last sunday',
  'last monday', 'last tuesday', 'last wednesday', 'last thursday', 'last saturday', 'last summer',
  'last fall', 'last autumn', 'last winter', 'last spring', 'earlier', 'just now', 'now', 'this night', 'Now',
  // ISO (zone-designated only here; the zone-less rows are below, with ids)
  '2026-07-14', '2026-07-14T09:30:00.000Z', '2026-07-14t09:30z', '2026-07-14T09:30:00+05:30',
  '2026-07-14t09:30+0530', '2026-07-14t09:30+05', '2026-07-14T09', '2026-07-14t9:30', '2026-07-14T24:00Z', '2026-07-14t24:30z', '2026-07-14T24:00:01Z',
  '2026-02-31', '2026-02-30T00:00Z', '0099-01-01', '2026-13-01', '2026-07-14t09:30:61z', '2026-07-14t::z',
  // named dates
  'October 1', 'December 25', 'on July 14th', '14 July 2026', '14th of July', 'July 14, 2026', 'July 14 2026',
  'on 3rd of march', 'july 4, 0000', 'July 4, 0099', 'February 30', 'July 32', 'June 0', 'Smarch 5',
  'January 1', 'december 31', 'March 8', 'November 1',
  // v4 throws: MONTH_NAMES is a plain object
  'constructor 5', '5 constructor',
  // nothing
  'the third night at sea', 'when the moons align', '', '   ', null, 'yesterday!', 'not yet',
];
const EVENT_ANCHORS: Array<string | null> = [
  '2026-10-12T19:00:00.000Z', '2026-03-08T07:30:00.000Z', '2026-11-01T06:30:00.000Z', '2026-01-01T03:00:00.000Z',
  '2026-07-21T15:00:00.000Z',
];
for (const tz of ZONES) {
  for (const at of EVENT_ANCHORS) {
    for (const phrase of EVENT_PHRASES) {
      emit({ kind: 'event', tz, at, phrase, out: attempt(() => resolveEventTime(phrase, ms(at), { timeZone: tz })) });
    }
  }
  // A non-finite anchor resolves nothing.
  emit({ kind: 'event', tz, at: null, phrase: 'yesterday', out: attempt(() => resolveEventTime('yesterday', Number.NaN, { timeZone: tz })) });
}
// Zone-less datetimes (`Date.parse` in the PROCESS zone) — `ZONELESS_PARSE_AS_UTC`.
for (const [id, phrase] of [
  ['zoneless-datetime', '2026-07-14T09:30'],
  ['zoneless-datetime-fraction', '2026-07-14t09:30:00.5'],
  ['zoneless-datetime-seconds', '2026-07-14T09:30:00'],
] as const) {
  emit({ kind: 'event', id, tz: 'America/Chicago', at: EVENT_ANCHORS[0], phrase, out: attempt(() => resolveEventTime(phrase, ms(EVENT_ANCHORS[0]), { timeZone: 'America/Chicago' })) });
}

// ----- resolveWhenPhrase (the string form; Date.parse on the anchor) ------------
const WHEN_ANCHORS = ['2026-10-12T19:00:00.000Z', '2026-10-12T19:00:00Z', '2026-10-12T19:00:00+02:00', '2026-10-12', 'not a date', ''];
for (const tz of ZONES) {
  for (const anchorIso of WHEN_ANCHORS) {
    for (const phrase of ['yesterday', 'last night', 'just now', 'two weeks ago', 'December 25', 'last Friday', null]) {
      emit({ kind: 'when', tz, anchorIso, phrase, out: resolveWhenPhrase(phrase, anchorIso, tz) });
    }
  }
}
for (const [id, anchorIso, phrase] of [
  ['zoneless-anchor', '2026-10-12T02:00', 'yesterday'],
  ['zoneless-anchor-just-now', '2026-10-12T02:00', 'just now'],
] as const) {
  emit({ kind: 'when', id, tz: 'America/Chicago', anchorIso, phrase, out: resolveWhenPhrase(phrase, anchorIso, 'America/Chicago') });
}

// ----- the read side (resolveReferenceWindow) -----------------------------------
const READ_TEXTS = [
  'No, no. I mean the mission today. I want to hear about it.', 'earlier today we talked', 'how did it go this morning?',
  'you seemed rattled this afternoon', 'that thing this evening', 'what a mess tonight', 'about yesterday',
  'the day before yesterday', 'we met 3 days ago', 'we met three days ago', 'that was 1 day ago',
  'fourteen days ago exactly', 'we met eleven days ago', 'twelve days ago', 'Thirteen Days Ago', 'fifteen days ago',
  'you were quiet last night', 'busy this week', 'what you said last week', 'that was 97 days ago',
  'that was 15 days ago', 'that was 0 days ago', 'that was 100 days ago', "let's do it tomorrow",
  'we can start next week', 'we said yesterday that we would start tomorrow',
  'we covered that yesterday — but today, the mission', 'we met yesterday, or was it 97 days ago',
  'YESTERDAY was long', 'Today is better', 'we drove through Yesterdayville', 'the Todayer gazette',
  '_yesterday', 'yesterday_', '1yesterday', 'éyesterday was odd', 'it was yesterdayé', 'three days agoo',
  'how are the soil samples looking', '', 'last  night', 'last night', 'last\tnight', 'last\nweek',
  'ye\u017fterday was long', 'last wee\u212a we met', 'last wee\u212as plans', 'last weeK we met', 'yesterday\nand the mission today',
];
const READ_ANCHORS: Array<string | null> = [
  '2026-07-29T02:44:00.000Z', '2026-03-09T17:00:00.000Z', '2026-11-01T12:00:00.000Z', '2026-09-06T04:30:00.000Z', null,
];
for (const tz of ZONES) {
  for (const at of READ_ANCHORS) {
    for (const text of READ_TEXTS) {
      emit({ kind: 'read', tz, at, text, out: resolveReferenceWindow(text, { timeZone: tz, nowMs: ms(at) }) });
    }
  }
}

// ----- normaliseTimeRange ----------------------------------------------------------
type Range = { from?: string | null; to?: string | null } | null;
const RANGES: Range[] = [
  { from: '2026-10-11', to: '2026-10-11' },
  { from: '2026-10-11T00:00:00.000Z', to: '2026-10-12' },
  { from: '2026-10-12', to: '2026-10-11' },
  { from: 'last week', to: '2026-10-11' },
  { from: '2026-02-30', to: '2026-03-01' },
  null,
  {},
  { from: '2026-10-11' },
  { to: '2026-10-11' },
  { from: null, to: '2026-10-11' },
  { from: '', to: '' },
  { from: '  2026-10-11  ', to: ' 2026-10-12\n' },
  { from: '2026-10-11T00:00:00.000Z', to: '2026-10-11T00:00:00.000Z' },
  { from: '2026-10-11T00:00:00.001Z', to: '2026-10-11T00:00:00.000Z' },
  { from: '2026-02-29', to: '2026-03-01' },
  { from: '2028-02-29', to: '2028-03-01' },
  { from: '2026-1-01', to: '2026-01-02' },
  { from: '2026-07-14T09:30:00+05:30', to: '2026-07-14T05:00:00Z' },
  { from: '2026-07-14T09:30:00+05:30', to: '2026-07-14T03:59:59Z' },
  { from: '2026-10-11x', to: '2026-10-12' },
  { from: '2026-03-08', to: '2026-03-08' },
  { from: '2026-11-01', to: '2026-11-01' },
  { from: '2026-09-06', to: '2026-09-06' },
  { from: '0099-01-01', to: '2026-01-01' },
  { from: '2026-10-11T00:00:00.000Z', to: '2026-13-01' },
];
for (const tz of ZONES) {
  for (const range of RANGES) emit({ kind: 'normalise', tz, range, out: normaliseTimeRange(range, { timeZone: tz }) });
}
for (const [id, tz, from, to] of [
  ['zoneless-from-flips-order', 'Asia/Kolkata', '2026-07-14T16:00', '2026-07-14'],
  ['zoneless-from-same-order', 'Asia/Kolkata', '2026-07-14T09:30', '2026-07-15'],
] as const) {
  emit({ kind: 'normalise', id, tz, range: { from, to }, out: normaliseTimeRange({ from, to }, { timeZone: tz }) });
}

// ----- clockFor (QUILLTAP_TIMEZONE unset / set; the cache never set) -------------
const CLOCK_CHATS: Array<unknown> = ['__omit__', null, {}, { timelineMode: null }, { timelineMode: 'narrative' }, { timelineMode: 'realtime' }, { timelineMode: 'bogus' }];
const CLOCK_INSTANCES: Array<unknown> = ['__omit__', {}, { timeZone: null }, { timeZone: 'America/Chicago' }, { timeZone: 'Not/AZone' }, { timeZone: '' }];
for (const env of [null, 'Europe/Paris', 'Mars/Olympus']) {
  if (env === null) delete process.env.QUILLTAP_TIMEZONE;
  else process.env.QUILLTAP_TIMEZONE = env;
  for (const chat of CLOCK_CHATS) {
    for (const instance of CLOCK_INSTANCES) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const out = instance === '__omit__'
        ? clockFor(chat === '__omit__' ? undefined : (chat as any), undefined, 5)
        : clockFor(chat === '__omit__' ? undefined : (chat as any), instance as any, 5);
      emit({ kind: 'clock', env, chat, instance, system: LEG, out });
    }
  }
}
delete process.env.QUILLTAP_TIMEZONE;

// ----- resolveEpisodicAnchors (mirrors anchors.test.ts, crossed) ------------------
const A_ANCHOR = '2026-07-20T09:05:00.000Z';
const A_WINDOW_START = '2026-07-19T18:00:00.000Z';
const ABSENT = '__absent__';
for (const when of [null, '', 'last Tuesday', 'when the moons align', 'yesterday', 'the third night of the siege', 'last night', '2026-07-14', 'tomorrow']) {
  for (const referenceIso of [A_ANCHOR, null, '', 'not a date']) {
    for (const fallbackIso of [A_ANCHOR, A_WINDOW_START, null]) {
      for (const timelineMode of ['realtime', 'narrative'] as const) {
        for (const narrativeTime of [ABSENT, null, 'Midwinter Eve', '']) {
          for (const timeZone of [ABSENT, 'America/Chicago', 'Asia/Kolkata', 'Pacific/Chatham']) {
            const input: Record<string, unknown> = { when, referenceIso, fallbackIso, timelineMode };
            if (narrativeTime !== ABSENT) input.narrativeTime = narrativeTime;
            if (timeZone !== ABSENT) input.timeZone = timeZone;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            emit({ kind: 'anchors', input, system: LEG, out: resolveEpisodicAnchors(input as any) });
          }
        }
      }
    }
  }
}
{
  const input = { when: 'yesterday', referenceIso: '2026-07-20T15:00', fallbackIso: null, timelineMode: 'realtime' as const, timeZone: 'Asia/Kolkata' };
  emit({ kind: 'anchors', id: 'zoneless-reference', input, system: LEG, out: resolveEpisodicAnchors(input) });
}
