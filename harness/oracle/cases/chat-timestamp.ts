/**
 * Oracle case: chat timestamp utilities (chat-orchestration wave 1.2, Part A).
 *
 * Drives the REAL exported functions from v4's lib/chat/timestamp-utils.ts:
 *   resolveTimezone, calculateCurrentTimestamp, shouldInjectTimestamp,
 *   formatTimestampForSystemPrompt, initializeFictionalTime.
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
  calculateCurrentTimestamp,
  shouldInjectTimestamp,
  formatTimestampForSystemPrompt,
  initializeFictionalTime,
  type CalculatedTimestamp,
} from '@/lib/chat/timestamp-utils'
import type { TimestampConfig } from '@/lib/schemas/types'

// Pin the wall clock. v4 reads the clock two ways: `Date.now()` (calculate) and
// `new Date()` (the fictionalBaseRealTime fallback + initializeFictionalTime's
// `new Date().toISOString()`). BOTH must be pinned so every row is
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
      kind: 'inject'
      id: string
      config: TimestampConfig | null
      isInitial: boolean
      minutesSince: number | null | undefined
      out: boolean
    }
  | { kind: 'format'; id: string; formatted: string; isoValue: string; isFictional: boolean; autoPrepend: boolean; out: string }
  | { kind: 'init'; id: string; base: TimestampConfig; fictional: string; nowMs: number; out: TimestampConfig }

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

// ---- initializeFictionalTime ---------------------------------------------
PINNED_NOW = T_MAIN
{
  const b = base({ mode: 'START_ONLY', format: 'ISO8601', autoPrepend: false })
  const out = initializeFictionalTime(b, '1885-11-05T00:00:00.000Z')
  rows.push({ kind: 'init', id: 'init-basic', base: b, fictional: '1885-11-05T00:00:00.000Z', nowMs: T_MAIN, out })
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
