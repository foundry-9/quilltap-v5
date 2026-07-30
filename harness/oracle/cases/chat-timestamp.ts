/**
 * Oracle case: chat timestamp utilities (chat-orchestration wave 1.2, Part A).
 *
 * Drives the REAL exported functions from v4's lib/chat/timestamp-utils.ts:
 *   resolveTimezone, parseTimestampInTimezone, calculateCurrentTimestamp,
 *   shouldInjectTimestamp, formatTimestampForSystemPrompt,
 *   ensureFictionalBaseRealTime.
 *
 * Widened by P4.d18 (the `e3a9654f` fictional-story-clock drift): the parse-in-
 * timezone family, zone-less `datetime-local` fictional bases in the calc
 * family (the case class whose absence let `parse_date_ms → 0` survive a green
 * differential), and the ensure-anchor family replacing the deleted
 * initializeFictionalTime.
 *
 * Widened again by P4.d28 (the `b3ee00f1` Markdown-transcript drift): the
 * `calcAt` family drives the newly extracted `calculateTimestampAt` over
 * ARBITRARY historical instants with an explicit fallback anchor — the shape the
 * transcript renderer calls it in. Those rows deliberately pin the wall clock to
 * a sentinel far from every recorded instant: `calculateTimestampAt` must read
 * no clock at all, so a port (or a v4 regression) that reached for `Date.now()`
 * would land in 1969 and the diff would say so.
 *
 * The impure clock is pinned: `Date.now()` is overridden to a fixed epoch-ms per
 * row and the value recorded in the NDJSON so the Rust port injects the identical
 * `now_ms`. The `timezone === undefined` path reads the host TZ, so this case MUST
 * be run under `TZ=UTC` (getTimezoneOffset() === 0); the recorded `localOffsetMin`
 * is what the Rust side injects. `QUILLTAP_TIMEZONE` is cleared so the
 * resolveTimezone env branch is exercised deterministically via an explicit
 * override row.
 *
 * Run from inside the server checkout (TZ=UTC is required):
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-timestamp.ts \
 *     > /tmp/oracle-chat-timestamp.ndjson
 */

import {
  resolveTimezone,
  parseTimestampInTimezone,
  calculateCurrentTimestamp,
  calculateTimestampAt,
  shouldInjectTimestamp,
  formatTimestampForSystemPrompt,
  ensureFictionalBaseRealTime,
  type CalculatedTimestamp,
} from '@/lib/chat/timestamp-utils'
import type { TimestampConfig } from '@/lib/schemas/types'

// Pin the wall clock. v4 reads the clock two ways: `Date.now()` (calculate) and
// `new Date()` (the fictionalBaseRealTime fallback + ensureFictionalBaseRealTime's
// default anchor). BOTH must be pinned so every row is
// deterministic, so we override the whole Date constructor: a no-arg `new Date()`
// returns the pinned instant while all argument forms keep native behavior.
let PINNED_NOW = 0
const RealDate = Date
class MockDate extends RealDate {
  constructor(...args: any[]) {
    if (args.length === 0) {
      super(PINNED_NOW)
    } else {
      // @ts-expect-error variadic forward to the native Date constructor
      super(...args)
    }
  }
  static now(): number {
    return PINNED_NOW
  }
}
;(globalThis as any).Date = MockDate

// The host offset the undefined-timezone path reads (must be 0 under TZ=UTC).
const LOCAL_OFFSET_MIN = new Date(0).getTimezoneOffset()
if (LOCAL_OFFSET_MIN !== 0) {
  throw new Error(`chat-timestamp oracle must run under TZ=UTC (getTimezoneOffset=${LOCAL_OFFSET_MIN})`)
}
// Clear the env var so the resolveTimezone chain is fully controlled.
delete process.env.QUILLTAP_TIMEZONE

type Row =
  | { kind: 'resolve'; id: string; config: string | null; settings: string | null; out: string | null }
  | {
      kind: 'calc'
      id: string
      config: TimestampConfig
      timezone: string | null
      nowMs: number
      localOffsetMin: number
      out?: CalculatedTimestamp
      threw?: boolean
    }
  | {
      kind: 'calcAt'
      id: string
      config: TimestampConfig
      timezone: string | null
      realInstantMs: number
      fallbackAnchorMs: number | null
      localOffsetMin: number
      out?: CalculatedTimestamp
      threw?: boolean
    }
  | {
      kind: 'inject'
      id: string
      config: TimestampConfig | null
      isInitial: boolean
      minutesSince: number | null | undefined
      out: boolean
    }
  | { kind: 'format'; id: string; formatted: string; isoValue: string; isFictional: boolean; autoPrepend: boolean; out: string }
  | {
      kind: 'parseTz'
      id: string
      value: string
      timezone: string | null
      localOffsetMin: number
      out: number
    }
  | {
      kind: 'ensure'
      id: string
      config: unknown
      anchorMs: number | null
      nowMs: number
      out: unknown
    }

const rows: Row[] = []

// Base config helper (mirrors the Zod defaults the caller would materialize).
function base(overrides: Partial<TimestampConfig> = {}): TimestampConfig {
  return {
    mode: 'EVERY_MESSAGE',
    format: 'FRIENDLY',
    customFormat: null,
    useFictionalTime: false,
    fictionalBaseTimestamp: null,
    fictionalBaseRealTime: null,
    autoPrepend: true,
    timezone: null,
    intervalMinutes: 15,
    ...overrides,
  } as TimestampConfig
}

// ---- resolveTimezone ------------------------------------------------------
// (env var is cleared, so the third branch resolves to undefined.)
const resolveCases: Array<[string, string | null, string | null]> = [
  ['config-wins', 'America/New_York', 'Europe/Paris'],
  ['settings-fallback', null, 'Europe/Paris'],
  ['config-empty-string-falls-through', '', 'Europe/Paris'],
  ['both-null', null, null],
  ['settings-empty-null', null, ''],
]
for (const [id, config, settings] of resolveCases) {
  const out = resolveTimezone(config, settings) ?? null
  rows.push({ kind: 'resolve', id, config, settings, out })
}

// ---- calculateCurrentTimestamp -------------------------------------------
// Fixed instants (UTC epoch-ms), including both US DST boundaries.
const T_MAIN = new Date('2026-07-02T12:34:56.789Z').getTime()
const T_WINTER = new Date('2026-01-15T00:00:00.000Z').getTime()
const T_SPRING_BEFORE = new Date('2026-03-08T06:59:00.000Z').getTime() // NY 1:59 EST
const T_SPRING_AFTER = new Date('2026-03-08T07:00:00.000Z').getTime() // NY 3:00 EDT
const T_FALL_BEFORE = new Date('2026-11-01T05:59:00.000Z').getTime() // NY 1:59 EDT
const T_FALL_AFTER = new Date('2026-11-01T06:00:00.000Z').getTime() // NY 1:00 EST

const zones: Array<string | null> = ['UTC', 'America/New_York', 'Europe/Paris', 'Asia/Kolkata', 'Pacific/Chatham', null]

function pushCalc(id: string, config: TimestampConfig, timezone: string | null, nowMs: number) {
  PINNED_NOW = nowMs
  const off = new Date(nowMs).getTimezoneOffset()
  try {
    const out = calculateCurrentTimestamp(config, timezone ?? undefined)
    rows.push({ kind: 'calc', id, config, timezone, nowMs, localOffsetMin: off, out })
  } catch {
    rows.push({ kind: 'calc', id, config, timezone, nowMs, localOffsetMin: off, threw: true })
  }
}

// Each format across every zone (+ the undefined-zone path), at T_MAIN.
for (const fmt of ['ISO8601', 'FRIENDLY', 'DATE_ONLY', 'TIME_ONLY'] as const) {
  for (const z of zones) {
    pushCalc(`${fmt}-${z ?? 'none'}`, base({ format: fmt, timezone: z }), z, T_MAIN)
  }
}

// DST boundary instants (FRIENDLY + ISO8601) in America/New_York and Europe/Paris.
for (const [label, t] of [
  ['spring-before', T_SPRING_BEFORE],
  ['spring-after', T_SPRING_AFTER],
  ['fall-before', T_FALL_BEFORE],
  ['fall-after', T_FALL_AFTER],
] as const) {
  for (const z of ['America/New_York', 'Europe/Paris'] as const) {
    pushCalc(`dst-${label}-friendly-${z}`, base({ format: 'FRIENDLY', timezone: z }), z, t)
    pushCalc(`dst-${label}-iso-${z}`, base({ format: 'ISO8601', timezone: z }), z, t)
  }
}

// Midnight / noon 12-hour edges (UTC).
pushCalc('midnight-friendly-utc', base({ format: 'FRIENDLY', timezone: 'UTC' }), 'UTC', T_WINTER)
pushCalc('noon-time-utc', base({ format: 'TIME_ONLY', timezone: 'UTC' }), 'UTC', new Date('2026-01-15T12:00:00Z').getTime())

// CUSTOM formats (incl. the sequential-replace quirk).
const customFmts: Array<[string, string]> = [
  ['custom-full', 'YYYY-MM-DD HH:mm dddd'],
  ['custom-friendlyish', 'MMMM D, YYYY h:mm A'],
  ['custom-short', 'ddd, MMM D'],
  ['custom-12h', 'hh:mm:ss a'],
  ['custom-yy', "YY 'x' M/D"],
  ['custom-tokens-in-names', 'dddd MMMM'],
]
for (const [id, cf] of customFmts) {
  pushCalc(id, base({ format: 'CUSTOM', customFormat: cf, timezone: 'UTC' }), 'UTC', T_MAIN)
}
// CUSTOM with no format string → FRIENDLY fallback.
pushCalc('custom-empty-falls-friendly', base({ format: 'CUSTOM', customFormat: null, timezone: 'UTC' }), 'UTC', T_MAIN)
pushCalc('custom-emptystr-falls-friendly', base({ format: 'CUSTOM', customFormat: '', timezone: 'UTC' }), 'UTC', T_MAIN)

// Invalid zone → throw.
pushCalc('invalid-zone-throws', base({ format: 'FRIENDLY', timezone: 'Not/AZone' }), 'Not/AZone', T_MAIN)

// Fictional time paths.
pushCalc(
  'fictional-elapsed-zero',
  base({
    format: 'ISO8601',
    timezone: null,
    useFictionalTime: true,
    fictionalBaseTimestamp: '2000-01-01T00:00:00.000Z',
    fictionalBaseRealTime: '2026-07-02T12:34:56.789Z',
  }),
  null,
  T_MAIN,
)
pushCalc(
  'fictional-elapsed-hour',
  base({
    format: 'ISO8601',
    timezone: 'UTC',
    useFictionalTime: true,
    fictionalBaseTimestamp: '2000-01-01T00:00:00.000Z',
    fictionalBaseRealTime: '2026-07-02T11:34:56.789Z', // now is 1h later → fictional +1h
  }),
  'UTC',
  T_MAIN,
)
// `fictionalBaseRealTime: null` → v4 falls back to `new Date()` (pinned above),
// so realBase == now → elapsed 0. The Rust port models the fallback as the
// injected now_ms, matching.
pushCalc(
  'fictional-no-realbase',
  base({
    format: 'ISO8601',
    timezone: null,
    useFictionalTime: true,
    fictionalBaseTimestamp: '2000-01-01T00:00:00.000Z',
    fictionalBaseRealTime: null,
  }),
  null,
  T_MAIN,
)
pushCalc(
  'fictional-flag-but-no-base',
  base({ format: 'ISO8601', timezone: null, useFictionalTime: true, fictionalBaseTimestamp: null }),
  null,
  T_MAIN,
)

// ---- fictional bases in the shape the UI actually produces (P4.d18) --------
// `<input type="datetime-local">` emits a ZONE-LESS wall-clock string. This is
// the case class the corpus lacked, and its absence is why the port's
// `parse_date_ms → 0` bug (every fictional clock reading 1970-adjacent
// nonsense) survived a green differential. Each zone gets an ANCHORED row (a
// real elapsed span, so the clock is proven to advance) and an UNANCHORED one
// (v4's `new Date()` fallback → elapsed 0 → the base itself).
const NAIVE_BASES: Array<[string, string, string | null]> = [
  // id-suffix, fictionalBaseTimestamp, timezone
  ['istanbul-1550', '1550-07-25T10:15', 'Europe/Istanbul'], // pre-standardisation LMT (+01:55:52)
  ['ny-modern', '2026-07-25T10:15', 'America/New_York'],
  ['kolkata-halfhour', '2026-07-25T10:15', 'Asia/Kolkata'],
  ['chatham-45', '2026-07-25T10:15', 'Pacific/Chatham'],
  ['utc', '2026-07-25T10:15', 'UTC'],
  ['no-zone', '2026-07-25T10:15', null], // system-local fall-through (TZ=UTC here)
  ['with-seconds', '1885-11-05T01:20:59', 'America/Chicago'],
  ['space-separator', '1885-11-05 01:20', 'America/Chicago'],
  ['dst-spring-gap', '2026-03-08T02:30', 'America/New_York'], // the hour that does not exist
  ['dst-fall-overlap', '2026-11-01T01:30', 'America/New_York'], // the hour that happens twice
]
for (const [suffix, fictionalBaseTimestamp, z] of NAIVE_BASES) {
  for (const fmt of ['ISO8601', 'FRIENDLY'] as const) {
    // Anchored 90 minutes back → the story clock must read base + 1h30m.
    pushCalc(
      `naive-${suffix}-anchored-${fmt}`,
      base({
        format: fmt,
        timezone: z,
        useFictionalTime: true,
        fictionalBaseTimestamp,
        fictionalBaseRealTime: new Date(T_MAIN - 90 * 60_000).toISOString(),
      }),
      z,
      T_MAIN,
    )
  }
  pushCalc(
    `naive-${suffix}-unanchored`,
    base({
      format: 'ISO8601',
      timezone: z,
      useFictionalTime: true,
      fictionalBaseTimestamp,
      fictionalBaseRealTime: null,
    }),
    z,
    T_MAIN,
  )
}

// Absolute bases still pass through parseTimestampInTimezone untouched, even
// with a story timezone set.
pushCalc(
  'fictional-absolute-z-with-zone',
  base({
    format: 'ISO8601',
    timezone: 'Europe/Istanbul',
    useFictionalTime: true,
    fictionalBaseTimestamp: '1776-07-04T16:30:00.000Z',
    fictionalBaseRealTime: '2026-07-02T11:34:56.789Z',
  }),
  'Europe/Istanbul',
  T_MAIN,
)
pushCalc(
  'fictional-absolute-offset-with-zone',
  base({
    format: 'ISO8601',
    timezone: 'America/Chicago',
    useFictionalTime: true,
    fictionalBaseTimestamp: '2026-01-15T12:00:00+05:00',
    fictionalBaseRealTime: '2026-07-02T12:34:56.789Z',
  }),
  'America/Chicago',
  T_MAIN,
)
// JS truthiness on both fictional guards: '' is falsy, so an empty base means
// "not fictional" and an empty anchor falls back to `new Date()`.
pushCalc(
  'fictional-empty-base-is-realtime',
  base({ format: 'ISO8601', timezone: 'UTC', useFictionalTime: true, fictionalBaseTimestamp: '' }),
  'UTC',
  T_MAIN,
)
pushCalc(
  'fictional-empty-realbase-falls-back',
  base({
    format: 'ISO8601',
    timezone: 'UTC',
    useFictionalTime: true,
    fictionalBaseTimestamp: '2026-07-25T10:15',
    fictionalBaseRealTime: '',
  }),
  'UTC',
  T_MAIN,
)

// ---- calculateTimestampAt (P4.d28) ---------------------------------------
// The extraction `b3ee00f1` made so a transcript can translate historical
// messages. Three differences from the live clock, all tabled here: the
// `realBase` fallback is the caller's anchor (not `new Date()`), elapsed time is
// measured from the passed instant, and the non-fictional arm returns that
// instant itself.
//
// The clock is pinned to a SENTINEL these rows never legitimately produce: this
// function must read no clock, and a row that came out 1969-adjacent would prove
// otherwise on either side.
const CLOCK_SENTINEL = -86_400_000 // 1969-12-31T00:00:00.000Z

function pushCalcAt(
  id: string,
  config: TimestampConfig,
  timezone: string | null,
  realInstantMs: number,
  fallbackAnchorMs: number | null,
) {
  PINNED_NOW = CLOCK_SENTINEL
  const off = new Date(realInstantMs).getTimezoneOffset()
  try {
    const out =
      fallbackAnchorMs === null
        ? calculateTimestampAt(new Date(realInstantMs), config, timezone ?? undefined)
        : calculateTimestampAt(
            new Date(realInstantMs),
            config,
            timezone ?? undefined,
            new Date(fallbackAnchorMs),
          )
    rows.push({
      kind: 'calcAt',
      id,
      config,
      timezone,
      realInstantMs,
      fallbackAnchorMs,
      localOffsetMin: off,
      out,
    })
  } catch {
    rows.push({
      kind: 'calcAt',
      id,
      config,
      timezone,
      realInstantMs,
      fallbackAnchorMs,
      localOffsetMin: off,
      threw: true,
    })
  }
}

// v4's own new test block (timestamp-utils.test.ts:429-480), row-for-row.
pushCalcAt(
  'v4test-historical-onto-fictional',
  base({
    format: 'ISO8601',
    timezone: 'UTC',
    useFictionalTime: true,
    fictionalBaseTimestamp: '1920-05-01T20:00',
    fictionalBaseRealTime: '2026-01-01T00:00:00.000Z',
  }),
  'UTC',
  new Date('2026-01-01T00:45:00.000Z').getTime(),
  null,
)
pushCalcAt(
  'v4test-fallback-anchor',
  base({
    format: 'ISO8601',
    timezone: 'UTC',
    useFictionalTime: true,
    fictionalBaseTimestamp: '1920-05-01T20:00',
    fictionalBaseRealTime: null,
  }),
  'UTC',
  new Date('2026-01-01T01:30:00.000Z').getTime(),
  new Date('2026-01-01T00:00:00.000Z').getTime(),
)
pushCalcAt(
  'v4test-real-instant-passthrough',
  base({ format: 'ISO8601', timezone: null, useFictionalTime: false }),
  null,
  new Date('2026-01-01T12:00:00.000Z').getTime(),
  null,
)

// The anchor is only consulted when the config carries none: a stamped
// `fictionalBaseRealTime` wins over the passed anchor.
pushCalcAt(
  'anchor-ignored-when-stamped',
  base({
    format: 'ISO8601',
    timezone: 'UTC',
    useFictionalTime: true,
    fictionalBaseTimestamp: '1920-05-01T20:00',
    fictionalBaseRealTime: '2026-01-01T00:00:00.000Z',
  }),
  'UTC',
  new Date('2026-01-01T00:45:00.000Z').getTime(),
  new Date('1990-01-01T00:00:00.000Z').getTime(), // a decades-off anchor, deliberately
)
// No config anchor AND no passed anchor → elapsed collapses to zero (the base).
pushCalcAt(
  'unanchored-no-fallback-is-base',
  base({
    format: 'ISO8601',
    timezone: 'UTC',
    useFictionalTime: true,
    fictionalBaseTimestamp: '1920-05-01T20:00',
    fictionalBaseRealTime: null,
  }),
  'UTC',
  new Date('2026-01-01T01:30:00.000Z').getTime(),
  null,
)
// An anchor AFTER the instant → negative elapsed (the story clock runs backward
// from its base). The transcript reaches this whenever a message predates the
// chat row's own createdAt.
pushCalcAt(
  'fallback-anchor-after-instant',
  base({
    format: 'ISO8601',
    timezone: 'UTC',
    useFictionalTime: true,
    fictionalBaseTimestamp: '1920-05-01T20:00',
    fictionalBaseRealTime: null,
  }),
  'UTC',
  new Date('2026-01-01T00:00:00.000Z').getTime(),
  new Date('2026-01-01T02:15:00.000Z').getTime(),
)

// Every format at a historical instant, in each zone and the zone-less path —
// the transcript's real workload (FRIENDLY after the DATE_ONLY/TIME_ONLY
// promotion, ISO8601 when the operator picked it).
const T_HIST = new Date('1926-03-14T19:05:07.000Z').getTime()
for (const fmt of ['ISO8601', 'FRIENDLY', 'DATE_ONLY', 'TIME_ONLY'] as const) {
  for (const z of zones) {
    pushCalcAt(
      `hist-${fmt}-${z ?? 'none'}`,
      base({ format: fmt, timezone: z }),
      z,
      T_HIST,
      new Date('2026-01-01T00:00:00.000Z').getTime(),
    )
  }
}

// Historical instants across both US DST boundaries, in a named zone: the
// offset must be the one in force AT THE MESSAGE, not at export time.
for (const [label, t] of [
  ['spring-before', T_SPRING_BEFORE],
  ['spring-after', T_SPRING_AFTER],
  ['fall-before', T_FALL_BEFORE],
  ['fall-after', T_FALL_AFTER],
] as const) {
  pushCalcAt(
    `hist-dst-${label}`,
    base({ format: 'FRIENDLY', timezone: 'America/New_York' }),
    'America/New_York',
    t,
    new Date('2026-01-01T00:00:00.000Z').getTime(),
  )
}

// The production fictional shape: a zone-less `datetime-local` base, anchored at
// the chat's createdAt, read at a message instant hours later.
for (const [suffix, fictionalBaseTimestamp, z] of NAIVE_BASES) {
  pushCalcAt(
    `hist-naive-${suffix}`,
    base({
      format: 'FRIENDLY',
      timezone: z,
      useFictionalTime: true,
      fictionalBaseTimestamp,
      fictionalBaseRealTime: null,
    }),
    z,
    new Date('2026-07-02T15:04:56.789Z').getTime(),
    new Date('2026-07-02T12:34:56.789Z').getTime(), // 2h30m of story time
  )
}

// CUSTOM (and the empty-format FRIENDLY fallback) at a historical instant.
pushCalcAt(
  'hist-custom',
  base({ format: 'CUSTOM', customFormat: 'YYYY-MM-DD HH:mm dddd', timezone: 'UTC' }),
  'UTC',
  T_HIST,
  null,
)
pushCalcAt(
  'hist-custom-empty-falls-friendly',
  base({ format: 'CUSTOM', customFormat: null, timezone: 'UTC' }),
  'UTC',
  T_HIST,
  null,
)
// An unresolvable zone throws — the transcript's 500 arm.
pushCalcAt(
  'hist-invalid-zone-throws',
  base({ format: 'FRIENDLY', timezone: 'Not/AZone' }),
  'Not/AZone',
  T_HIST,
  null,
)

// ---- shouldInjectTimestamp -----------------------------------------------
const injectCases: Array<[string, TimestampConfig | null, boolean, number | null | undefined]> = [
  ['null-config', null, true, null],
  ['none-mode', base({ mode: 'NONE' }), true, null],
  ['start-only-initial', base({ mode: 'START_ONLY' }), true, null],
  ['start-only-not-initial', base({ mode: 'START_ONLY' }), false, null],
  ['every-message', base({ mode: 'EVERY_MESSAGE' }), false, null],
  ['n-min-initial', base({ mode: 'EVERY_N_MINUTES' }), true, 3],
  ['n-min-null-since', base({ mode: 'EVERY_N_MINUTES' }), false, null],
  ['n-min-undef-since', base({ mode: 'EVERY_N_MINUTES' }), false, undefined],
  ['n-min-below', base({ mode: 'EVERY_N_MINUTES', intervalMinutes: 15 }), false, 14],
  ['n-min-at', base({ mode: 'EVERY_N_MINUTES', intervalMinutes: 15 }), false, 15],
  ['n-min-above', base({ mode: 'EVERY_N_MINUTES', intervalMinutes: 15 }), false, 30],
  ['n-min-custom-interval', base({ mode: 'EVERY_N_MINUTES', intervalMinutes: 60 }), false, 45],
]
for (const [id, config, isInitial, minutesSince] of injectCases) {
  const out = shouldInjectTimestamp(config, isInitial, minutesSince as number | null | undefined)
  rows.push({ kind: 'inject', id, config, isInitial, minutesSince, out })
}

// ---- formatTimestampForSystemPrompt --------------------------------------
const ts: CalculatedTimestamp = { formatted: 'July 2, 2026 at 12:34 PM', isoValue: '2026-07-02T12:34:56+00:00', isFictional: false }
for (const ap of [true, false]) {
  rows.push({
    kind: 'format',
    id: `format-${ap}`,
    formatted: ts.formatted,
    isoValue: ts.isoValue,
    isFictional: ts.isFictional,
    autoPrepend: ap,
    out: formatTimestampForSystemPrompt(ts, ap),
  })
}

// ---- parseTimestampInTimezone (P4.d18) ------------------------------------
// The returned Date is compared as epoch-ms. Every input here is well-formed:
// v4's unparseable fall-through is `NaN`, which has no i64 counterpart in the
// Rust port (it yields 0), so the differential deliberately does not table it.
const parseTzCases: Array<[string, string, string | null]> = [
  // Zone-less, read as a clock in the target zone.
  ['naive-istanbul-1550', '1550-07-25T10:15', 'Europe/Istanbul'],
  ['naive-istanbul-modern', '2026-07-25T10:15', 'Europe/Istanbul'],
  ['naive-ny', '2026-07-25T10:15', 'America/New_York'],
  ['naive-kolkata', '2026-07-25T10:15', 'Asia/Kolkata'],
  ['naive-chatham', '2026-07-25T10:15', 'Pacific/Chatham'],
  ['naive-utc', '2026-07-25T10:15', 'UTC'],
  // Optional seconds, and the `[T ]` space separator.
  ['naive-with-seconds', '1885-11-05T01:20:59', 'America/Chicago'],
  ['naive-space-separator', '1885-11-05 01:20', 'America/Chicago'],
  // Leading/trailing whitespace: trimmed before the match.
  ['naive-whitespace', '  2026-07-25T10:15  ', 'Europe/Paris'],
  // DST boundaries — the two-iteration cases.
  ['naive-dst-spring-gap', '2026-03-08T02:30', 'America/New_York'],
  ['naive-dst-spring-edge', '2026-03-08T03:00', 'America/New_York'],
  ['naive-dst-fall-overlap', '2026-11-01T01:30', 'America/New_York'],
  ['naive-dst-paris-spring', '2026-03-29T02:30', 'Europe/Paris'],
  // Absolute strings pass through untouched.
  ['absolute-z', '1776-07-04T16:30:00.000Z', 'Europe/Istanbul'],
  ['absolute-offset', '2026-01-15T12:00:00+05:00', 'America/Chicago'],
  ['absolute-negative-offset', '2026-01-15T12:00:00-08:00', 'Asia/Kolkata'],
  // No timezone → system-local parsing (TZ=UTC on this oracle run).
  ['no-timezone-naive', '2026-07-25T10:15', null],
  ['no-timezone-absolute', '2026-07-25T10:15:00.000Z', null],
  // Shapes the naive pattern rejects, so they fall through to new Date():
  ['fallthrough-millis', '2026-07-25T10:15:30.500Z', 'Europe/Istanbul'],
  ['fallthrough-date-only', '2026-07-25', 'Europe/Istanbul'],
]
for (const [id, value, timezone] of parseTzCases) {
  PINNED_NOW = T_MAIN
  const out = parseTimestampInTimezone(value, timezone ?? undefined).getTime()
  rows.push({ kind: 'parseTz', id, value, timezone, localOffsetMin: LOCAL_OFFSET_MIN, out })
}

// ---- ensureFictionalBaseRealTime (P4.d18) ---------------------------------
// Raw objects, not typed configs: v4's `{...config}` spread carries unknown
// keys through and the Rust port operates on the same serde_json::Value, so the
// diff is whole-object (key order included).
const ANCHOR_MS = new Date('2026-01-02T03:04:05.000Z').getTime()
const ensureCases: Array<[string, unknown, number | null]> = [
  // Guard 1+2+3 all pass → stamped with `new Date()` (the pinned now).
  [
    'stamps-unanchored',
    {
      mode: 'EVERY_MESSAGE',
      format: 'FRIENDLY',
      useFictionalTime: true,
      fictionalBaseTimestamp: '1776-07-04T16:30',
      autoPrepend: true,
    },
    null,
  ],
  // Unknown keys ride along; the new key lands last.
  [
    'unknown-keys-ride-along',
    {
      useFictionalTime: true,
      fictionalBaseTimestamp: '1776-07-04T16:30',
      somethingWeDoNotModel: { nested: [1, 2, 3] },
      intervalMinutes: 15,
    },
    null,
  ],
  // Explicit anchor (the retro-fit arm).
  [
    'explicit-anchor',
    { useFictionalTime: true, fictionalBaseTimestamp: '1776-07-04T16:30' },
    ANCHOR_MS,
  ],
  // Guard 3: an existing anchor is never re-stamped.
  [
    'already-anchored',
    {
      useFictionalTime: true,
      fictionalBaseTimestamp: '1776-07-04T16:30',
      fictionalBaseRealTime: '2020-05-05T05:05:05.000Z',
    },
    null,
  ],
  // Guard 1: real-time clock.
  [
    'real-time-untouched',
    { useFictionalTime: false, fictionalBaseTimestamp: '1776-07-04T16:30' },
    null,
  ],
  ['fictional-flag-absent', { fictionalBaseTimestamp: '1776-07-04T16:30' }, null],
  // Guard 2: no base to count from (null, absent, and the falsy empty string).
  ['base-null', { useFictionalTime: true, fictionalBaseTimestamp: null }, null],
  ['base-absent', { useFictionalTime: true }, null],
  ['base-empty-string', { useFictionalTime: true, fictionalBaseTimestamp: '' }, null],
  // Guard 3 is a truthiness test too: a falsy anchor is re-stamped IN PLACE,
  // so the key keeps its original slot rather than moving to the end.
  [
    'anchor-explicit-null-restamped-in-place',
    {
      useFictionalTime: true,
      fictionalBaseTimestamp: '1776-07-04T16:30',
      fictionalBaseRealTime: null,
      trailingKey: 'kept',
    },
    null,
  ],
  [
    'anchor-empty-string-restamped',
    {
      useFictionalTime: true,
      fictionalBaseTimestamp: '1776-07-04T16:30',
      fictionalBaseRealTime: '',
    },
    null,
  ],
  // null / undefined pass straight through.
  ['null-config', null, null],
]
for (const [id, config, anchorMs] of ensureCases) {
  PINNED_NOW = T_MAIN
  const out =
    anchorMs === null
      ? ensureFictionalBaseRealTime(config as TimestampConfig)
      : ensureFictionalBaseRealTime(config as TimestampConfig, new Date(anchorMs))
  rows.push({ kind: 'ensure', id, config, anchorMs, nowMs: T_MAIN, out })
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
