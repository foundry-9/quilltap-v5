/**
 * The v4-side recorder behind `src/app/wardrobe/wear-display.oracle.spec.ts`
 * (P4.D277).
 *
 * Bug 183 (`ed8b15b50`) taught v4's `lib/wardrobe/wear-display.ts` a new
 * exported `formatWornRelative`: the shared day ladder
 * (`lib/format-time.ts`'s `formatRelativeDays`, untouched) answers "last week"
 * and "last month" for the 7–13 and 30–59 day rungs, which read "last last
 * week" after the wear line's own "last", so those two rungs become "a week
 * ago" / "a month ago". `formatWornWhen` now delegates to it, so the row's
 * wear line, the Prospero manager's line AND the item editor's Wear-history
 * dates all move. This recorder drives v4's REAL module (which imports the
 * REAL ladder) over the whole day ladder, every rung edge, and the empty /
 * unparseable inputs, so the SPA's transcription is compared row for row.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Every input is either a `Z`-suffixed ISO string or a date-only string
 * (UTC by spec) — a local-time ISO would make the two runs TZ-dependent.
 * Day offsets are recorded as INTEGER milliseconds.
 *
 * Run it from a pinned v4 worktree at `01a83539d` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 01a83539d}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/wear-display.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/wear-display.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx wear-display.recorder.ts > "$OUT")
 * wc -l "$OUT"          # expect 112
 * grep -c 'a week ago' "$OUT"; grep -c 'a month ago' "$OUT"   # both > 0
 * ```
 *
 * Expect 112 lines (32 `formatWornRelative` + 38 `formatWornWhen` + 42
 * `formatWearLine`); a shorter file means the recorder errored and the
 * redirect already truncated the old one (the empty-file trap).
 */

import { formatWearLine, formatWornRelative, formatWornWhen } from '@/lib/wardrobe/wear-display';

/** v4's jest clock (`__tests__/unit/lib/wardrobe/wear-display.test.ts`). */
const NOW = Date.parse('2026-10-07T12:00:00.000Z');
const DAY_MS = 86_400_000;

/** `undefined` cannot ride JSON — the spec maps this tag back. */
const UNDEFINED = { $undefined: true } as const;

/** Every ladder rung and both sides of each rung edge, in days. */
const LADDER_DAYS = [
  -1, 0, 0.5, 0.999, 1, 1.5, 1.999, 2, 3, 6.9, 6.999, 7, 9, 13, 13.9, 13.999, 14, 15, 21, 29,
  29.999, 30, 45, 59, 59.999, 60, 120, 364, 364.999, 365, 730, 800,
];

const offsetMs = (days: number): number => Math.round(days * DAY_MS);
const isoAgo = (days: number): string => new Date(NOW - offsetMs(days)).toISOString();

const emit = (fn: string, id: string, args: unknown[], out: string): void => {
  console.log(JSON.stringify({ fn, id, args, out }));
};

// formatWornRelative(ts, nowMs) — 32 rows.
for (const days of LADDER_DAYS) {
  const ts = NOW - offsetMs(days);
  emit('formatWornRelative', `days=${days}`, [ts, NOW], formatWornRelative(ts, NOW));
}

// formatWornWhen(iso, nowMs) — 6 empty/odd inputs + 32 ladder ISOs = 38 rows.
const WHEN_INPUTS: Array<[string, unknown]> = [
  ['null', null],
  ['undefined', UNDEFINED],
  ['empty', ''],
  ['not a date', 'not a date'],
  ['out-of-range ISO', '2026-13-40T00:00:00Z'],
  ['date-only (UTC)', '2026-09-30'],
];
for (const [id, arg] of WHEN_INPUTS) {
  const real = arg === UNDEFINED ? undefined : (arg as string | null);
  emit('formatWornWhen', id, [arg, NOW], formatWornWhen(real, NOW));
}
for (const days of LADDER_DAYS) {
  const iso = isoAgo(days);
  emit('formatWornWhen', `days=${days}`, [iso, NOW], formatWornWhen(iso, NOW));
}

// formatWearLine(summary, nowMs) — 2 absent + 4 counts × 10 dates = 42 rows.
emit('formatWearLine', 'undefined', [UNDEFINED, NOW], formatWearLine(undefined, NOW));
emit('formatWearLine', 'null', [null, NOW], formatWearLine(null, NOW));
const LINE_DAYS: Array<number | null> = [null, 0, 3, 9, 13, 21, 30, 59, 120, 400];
for (const count of [0, 1, 2, 8]) {
  for (const days of LINE_DAYS) {
    const at = days === null ? null : isoAgo(days);
    const summary = { wearCount: count, firstWornAt: at, lastWornAt: at, lastWornChatId: null };
    emit('formatWearLine', `count=${count} days=${days}`, [summary, NOW], formatWearLine(summary, NOW));
  }
}
