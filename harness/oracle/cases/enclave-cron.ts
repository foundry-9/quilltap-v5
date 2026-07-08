/**
 * Oracle case: enclave cron slot computation (U4.2).
 *
 * Drives v4's REAL cron library — croner 10.0.1, resolved from the v4
 * checkout's node_modules — through the exact one-shot call-site shape shared
 * by `nextCronFireFrom` (autonomous-room-schedule-tick.ts), `recomputeNextRun`
 * (autonomous-room-turn.ts), and the manual-start slot consumption
 * (autonomous-room.service.ts):
 *
 *   try { return new Cron(cronExpr).nextRun(anchor) ?? null } catch { return null }
 *
 * v4 passes no timezone option, so croner evaluates in Node's system-local
 * timezone (plain JS Date semantics). Node caches TZ at startup, so run this
 * once per corpus timezone with the TZ env var set:
 *
 *   cd ~/source/quilltap-server
 *   TZ=America/Chicago npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-cron.ts \
 *     > /tmp/oracle-enclave-cron-chicago.ndjson
 *   TZ=Asia/Kolkata npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-cron.ts \
 *     > /tmp/oracle-enclave-cron-kolkata.ndjson
 *
 * The first NDJSON row is a probe recording the timezone Node actually
 * resolved (and the installed croner version); the Rust differential asserts
 * both before diffing. Every subsequent row is
 *   { kind: "row", id, expr, anchorIso, tz, nextIso | null }.
 *
 * The corpus is committed here (not generated): plain 5-field patterns,
 * 6-field seconds, steps/ranges/lists (incl. croner 10's strict-stepping
 * rejections), month/day names (incl. the sequential-replace JANFEB quirk),
 * dom+dow union vs the `+` AND modifier, `L`/`LW`/`W`/`#`/`5L` modifiers, the
 * `?`-before-starDOM quirk, month-end wraps, leap Feb 29 / never-matching
 * Feb 30, DST spring-forward (America/Chicago 2026-03-08) and fall-back
 * (2026-11-01, incl. the fold row whose nextRun lands BEFORE the anchor),
 * anchors exactly on a slot / at odd ms offsets, croner's one-off datetime
 * branch (":" in the pattern), nicknames, 7-field years, and a battery of
 * malformed inputs that must collapse to null.
 */

import { createRequire } from 'node:module';

// Resolve croner from the v4 checkout (run this script with cwd there).
const requireFromV4 = createRequire(process.cwd() + '/');
const { Cron } = requireFromV4('croner');
const cronerVersion: string = requireFromV4('croner/package.json').version;

/** The v4 call-site shape, verbatim (throw → null, undefined → null). */
function nextCronFireFrom(cronExpr: string, anchor: Date): Date | null {
  try {
    const job = new Cron(cronExpr);
    const next = job.nextRun(anchor);
    return next ?? null;
  } catch (_error) {
    return null;
  }
}

interface Row {
  id: string;
  expr: string;
  anchor: string;
}

const corpus: Row[] = [
  // --- A: plain 5-field patterns, slot-boundary anchors -------------------
  { id: 'a1_every_minute', expr: '* * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'a2_every_minute_midsecond', expr: '* * * * *', anchor: '2026-06-15T12:00:30.500Z' },
  { id: 'a3_daily_midnight', expr: '0 0 * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'a4_just_before_slot', expr: '30 14 * * *', anchor: '2026-06-15T19:29:59.999Z' },
  { id: 'a5_exactly_on_slot_skips', expr: '30 14 * * *', anchor: '2026-06-15T19:30:00.000Z' },
  { id: 'a6_on_slot_exact', expr: '0 12 * * *', anchor: '2026-06-15T17:00:00.000Z' },
  { id: 'a7_on_slot_plus_1ms', expr: '0 12 * * *', anchor: '2026-06-15T17:00:00.001Z' },
  { id: 'a8_1ms_before_slot', expr: '0 12 * * *', anchor: '2026-06-15T16:59:59.999Z' },
  { id: 'a9_dow_only', expr: '5 4 * * 1', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'a10_dom_only', expr: '0 9 15 * *', anchor: '2026-06-20T00:00:00.000Z' },

  // --- B: 6-field seconds --------------------------------------------------
  { id: 'b1_step5s', expr: '*/5 * * * * *', anchor: '2026-06-15T12:00:01.200Z' },
  { id: 'b2_second_before', expr: '30 30 12 * * *', anchor: '2026-06-15T17:30:29.999Z' },
  { id: 'b3_second_on_slot', expr: '30 30 12 * * *', anchor: '2026-06-15T17:30:30.000Z' },
  { id: 'b4_year_end_second', expr: '59 59 23 31 12 *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'b5_new_year_second', expr: '0 0 0 1 1 *', anchor: '2026-06-15T12:00:00.000Z' },

  // --- C: steps / ranges / lists -------------------------------------------
  { id: 'c1_step15', expr: '*/15 * * * *', anchor: '2026-06-15T12:07:00.000Z' },
  { id: 'c2_hour_range', expr: '0 9-17 * * *', anchor: '2026-06-15T23:30:00.000Z' },
  { id: 'c3_dom_list', expr: '0 0 1,15 * *', anchor: '2026-06-02T12:00:00.000Z' },
  { id: 'c4_range_step_inside', expr: '10-40/10 * * * *', anchor: '2026-06-15T12:35:00.000Z' },
  { id: 'c5_range_step_wraps', expr: '10-40/10 * * * *', anchor: '2026-06-15T12:41:00.000Z' },
  { id: 'c6_weekday_range', expr: '0 0 * * 1-5', anchor: '2026-06-13T12:00:00.000Z' },
  { id: 'c7_dow_name_list', expr: '0 12 * * MON,WED,FRI', anchor: '2026-06-15T18:00:00.000Z' },
  { id: 'c8_step60_is_hourly', expr: '*/60 * * * *', anchor: '2026-06-15T12:10:00.000Z' },
  { id: 'c9_hour_range_step', expr: '0 0-23/6 * * *', anchor: '2026-06-15T13:00:00.000Z' },
  { id: 'c10_mixed_list', expr: '1,2-5,*/20 * * * *', anchor: '2026-06-15T12:06:00.000Z' },

  // --- D: month/day names (sequential-replace quirks included) -------------
  { id: 'd1_month_name', expr: '0 0 1 JAN *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'd2_dow_name', expr: '0 0 * * SUN', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'd3_dow_name_range_sun_upper', expr: '0 0 * * FRI-SUN', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'd4_janfeb_sequential_quirk', expr: '0 0 1 JANFEB *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'd5_month_name_may', expr: '0 0 1 MAY *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'd6_dow_name_lowercase', expr: '0 0 * * sat', anchor: '2026-06-15T12:00:00.000Z' },

  // --- E: dom/dow combination rules ----------------------------------------
  { id: 'e1_union_dow_first', expr: '0 0 13 * 5', anchor: '2026-06-10T12:00:00.000Z' },
  { id: 'e2_union_dom_second', expr: '0 0 13 * 5', anchor: '2026-06-12T12:00:00.000Z' },
  { id: 'e3_plus_and_friday_13th', expr: '0 0 13 * +5', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'e4_last_friday', expr: '0 0 * * 5L', anchor: '2026-06-01T12:00:00.000Z' },
  { id: 'e5_second_friday', expr: '0 0 * * 5#2', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'e6_empty_nth_quirk', expr: '0 0 * * 5#', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'e7_star_dom_fridays_only', expr: '0 0 * * 5', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'e8_qmark_dom_every_day_quirk', expr: '0 0 ? * 5', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'e9_dow7_is_sunday', expr: '0 0 * * 7', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'e10_fifth_saturday', expr: '0 0 * * 6#5', anchor: '2026-04-10T12:00:00.000Z' },
  { id: 'e11_nth_list_mix', expr: '0 0 * * 1#1,5L', anchor: '2026-06-20T12:00:00.000Z' },
  { id: 'e12_dow_step', expr: '0 0 * * */2', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'e13_last_day_or_friday', expr: '0 0 L * 5', anchor: '2026-06-01T12:00:00.000Z' },

  // --- F: L / LW / W day-of-month modifiers + calendar wraps ---------------
  { id: 'f1_last_day', expr: '0 0 L * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'f2_last_weekday', expr: '0 0 LW * *', anchor: '2026-05-01T12:00:00.000Z' },
  { id: 'f3_last_weekday_lowercase', expr: '0 0 lw * *', anchor: '2026-05-01T12:00:00.000Z' },
  { id: 'f4_nearest_weekday_15', expr: '0 0 15W * *', anchor: '2026-08-01T12:00:00.000Z' },
  { id: 'f5_nearest_weekday_sat_1st', expr: '0 0 1W 8 *', anchor: '2026-07-15T12:00:00.000Z' },
  { id: 'f6_nearest_weekday_absent_day', expr: '0 0 30W 2 *', anchor: '2026-01-01T12:00:00.000Z' },
  { id: 'f7_leap_feb29', expr: '0 0 29 2 *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'f8_feb30_never', expr: '0 0 30 2 *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'f9_dom31_wraps_april', expr: '0 0 31 * *', anchor: '2026-04-02T12:00:00.000Z' },
  { id: 'f10_dom31_wraps_feb', expr: '0 0 31 * *', anchor: '2026-02-05T12:00:00.000Z' },
  { id: 'f11_april_31_never', expr: '0 0 31 4 *', anchor: '2026-01-01T12:00:00.000Z' },

  // --- G: DST spring-forward (America/Chicago 2026-03-08 02:00 → 03:00) ----
  { id: 'g1_gap_daily_0230', expr: '30 2 * * *', anchor: '2026-03-08T07:00:00.000Z' },
  { id: 'g2_gap_specific_6field', expr: '0 30 2 8 3 *', anchor: '2026-03-07T12:00:00.000Z' },
  { id: 'g3_gap_hourly', expr: '0 * * * *', anchor: '2026-03-08T07:30:00.000Z' },
  { id: 'g4_gap_after_transition', expr: '30 2 * * *', anchor: '2026-03-08T09:00:00.000Z' },
  { id: 'g5_gap_0215', expr: '15 2 * * *', anchor: '2026-03-08T07:50:00.000Z' },
  { id: 'g6_gap_anchor_in_mapped_hour', expr: '30 2 * * *', anchor: '2026-03-08T08:29:00.000Z' },
  { id: 'g7_gap_specific_5field', expr: '30 2 8 3 *', anchor: '2026-03-07T12:00:00.000Z' },

  // --- H: DST fall-back (America/Chicago 2026-11-01 02:00 → 01:00) ---------
  { id: 'h1_fold_before_first', expr: '30 1 * * *', anchor: '2026-11-01T05:00:00.000Z' },
  { id: 'h2_fold_between_lands_in_past', expr: '30 1 * * *', anchor: '2026-11-01T07:00:00.000Z' },
  { id: 'h3_fold_after_both', expr: '30 1 * * *', anchor: '2026-11-01T07:45:00.000Z' },
  { id: 'h4_fold_hourly_skips_repeat', expr: '0 * * * *', anchor: '2026-11-01T06:30:00.000Z' },
  { id: 'h5_fold_specific_next_year', expr: '0 30 1 1 11 *', anchor: '2026-11-01T06:45:00.000Z' },
  { id: 'h6_fold_one_minute_before', expr: '30 1 * * *', anchor: '2026-11-01T06:29:00.000Z' },

  // --- I: the one-off datetime branch (":" in the pattern) -----------------
  { id: 'i1_oneoff_local_future', expr: '2026-12-25T09:00:00', anchor: '2026-06-15T00:00:00.000Z' },
  { id: 'i2_oneoff_local_past', expr: '2026-12-25T09:00:00', anchor: '2027-01-01T00:00:00.000Z' },
  { id: 'i3_oneoff_utc', expr: '2026-12-25T09:00:00Z', anchor: '2026-06-15T00:00:00.000Z' },
  { id: 'i4_oneoff_offset', expr: '2026-12-25T09:00:00+05:30', anchor: '2026-06-15T00:00:00.000Z' },
  { id: 'i5_oneoff_ms_dropped_makes_past', expr: '2026-06-15T10:30:00.500', anchor: '2026-06-15T15:30:00.200Z' },
  { id: 'i6_oneoff_ms_dropped_still_future', expr: '2026-06-15T10:30:00.500', anchor: '2026-06-15T15:29:59.900Z' },
  { id: 'i7_oneoff_exactly_equal', expr: '2026-06-15T10:30:00', anchor: '2026-06-15T15:30:00.000Z' },
  { id: 'i8_oneoff_in_dst_gap', expr: '2026-03-08T02:30:00', anchor: '2026-03-01T00:00:00.000Z' },
  { id: 'i9_oneoff_in_dst_fold', expr: '2026-11-01T01:30:00', anchor: '2026-10-01T00:00:00.000Z' },
  { id: 'i10_oneoff_z_second_occurrence_shifts', expr: '2026-11-01T07:30:00Z', anchor: '2026-10-01T00:00:00.000Z' },
  { id: 'i11_oneoff_time_only_invalid', expr: '12:34', anchor: '2026-06-15T00:00:00.000Z' },
  { id: 'i12_oneoff_garbage_colon', expr: 'foo:bar', anchor: '2026-06-15T00:00:00.000Z' },
  { id: 'i13_colon_at_index_0_is_pattern', expr: ':30 * * * *', anchor: '2026-06-15T00:00:00.000Z' },
  { id: 'i14_oneoff_day_overflow_normalizes', expr: '2026-02-30T10:00:00', anchor: '2026-01-01T00:00:00.000Z' },
  { id: 'i15_oneoff_no_seconds', expr: '2026-06-15T10:30', anchor: '2026-06-01T00:00:00.000Z' },
  { id: 'i16_oneoff_hour_24', expr: '2026-06-15T24:00:00', anchor: '2026-06-01T00:00:00.000Z' },
  { id: 'i17_oneoff_hour_24_with_minutes_invalid', expr: '2026-06-15T24:30:00', anchor: '2026-06-01T00:00:00.000Z' },
  { id: 'i18_oneoff_day_32_invalid', expr: '2026-01-32T10:00:00', anchor: '2026-01-01T00:00:00.000Z' },

  // --- J: malformed inputs → null, and nicknames ----------------------------
  { id: 'j1_empty', expr: '', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j2_garbage', expr: 'garbage', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j3_four_fields', expr: '* * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j4_eight_fields', expr: '* * * * * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j5_minute_60', expr: '60 * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j6_hour_24', expr: '0 24 * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j7_dom_32', expr: '0 0 32 * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j8_dom_0', expr: '0 0 0 * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j9_month_13', expr: '0 0 1 13 *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j10_month_0', expr: '0 0 1 0 *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j11_dow_8', expr: '0 0 * * 8', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j12_bare_negative', expr: '-5 * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j13_reversed_range', expr: '5-1 * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j14_step_zero', expr: '*/0 * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j15_step_over_len', expr: '*/70 * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j16_numeric_prefix_step', expr: '0/10 * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j17_missing_prefix_step', expr: '/10 * * * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j18_at_daily', expr: '@daily', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j19_at_hourly', expr: '@hourly', anchor: '2026-06-15T12:10:00.000Z' },
  { id: 'j20_at_weekly', expr: '@weekly', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j21_at_monthly', expr: '@monthly', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j22_at_yearly', expr: '@yearly', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j23_at_annually', expr: '@annually', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j24_at_midnight', expr: '@midnight', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j25_at_daily_uppercase', expr: '@DAILY', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j26_at_reboot_throws', expr: '@reboot', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j27_at_unknown', expr: '@every5', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'j28_all_question_marks', expr: '0 0 ? * ?', anchor: '2026-06-15T12:00:00.000Z' },

  // --- K: whitespace, 7-field years, classic unions, index-based steps ------
  { id: 'k1_tab_separated', expr: '0\t12\t*\t*\t*', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k2_padded', expr: '  0 12 * * *  ', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k3_year_field', expr: '0 0 0 1 1 * 2027', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k4_year_in_past', expr: '0 0 0 1 1 * 2020', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k5_year_range', expr: '0 0 0 1 1 * 2027-2028', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k6_year_10000_invalid', expr: '0 0 0 1 1 * 10000', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k7_classic_union', expr: '0 5 4 * 1', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k8_month_step_is_index_based', expr: '0 0 1 */3 *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k9_day_step_is_index_based', expr: '0 0 */10 * *', anchor: '2026-06-15T12:00:00.000Z' },
  { id: 'k10_multi_space', expr: '0  12   * * *', anchor: '2026-06-15T12:00:00.000Z' },
];

// Probe row: the Rust side asserts the resolved timezone and croner version
// before trusting the corpus.
console.log(
  JSON.stringify({
    kind: 'probe',
    tz: Intl.DateTimeFormat().resolvedOptions().timeZone,
    croner: cronerVersion,
    node: process.version,
  }),
);

const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
for (const row of corpus) {
  const anchor = new Date(row.anchor);
  const next = nextCronFireFrom(row.expr, anchor);
  console.log(
    JSON.stringify({
      kind: 'row',
      id: row.id,
      expr: row.expr,
      anchorIso: anchor.toISOString(),
      tz,
      nextIso: next ? next.toISOString() : null,
    }),
  );
}
