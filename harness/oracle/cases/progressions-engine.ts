/**
 * Tier-1 ORACLE for the character-progressions ENGINE (v4 `25f534c0b`,
 * `lib/progressions/{schema,engine}.ts` — P4.D167).
 *
 * Drives v4's REAL exports over the committed corpus
 * (`harness/oracle/fixtures/progressions-engine.json`) and emits one NDJSON row
 * per (op, case). The Rust family
 * (`crates/quilltap-harness/tests/progressions_engine_equivalence.rs`) reads the
 * SAME corpus and compares field for field: strings byte-exact, integers exact,
 * the two genuine floats (`percent`, `quantityCurrent`) at 1e-12.
 *
 * What the corpus asks that a transcription cannot answer on its own: Zod
 * 4.5.4's exact issue sentences AND their order (field checks in declaration
 * order, then the single `(root)` unrecognized-key(s) issue, then the
 * `superRefine`), its code-POINT string lengths on astral text, `Date.parse`'s
 * V8 subset behind v4's stricter regex (the space separator, `+0530`, a 9-digit
 * fraction, a day that rolls past the month end), `Number.prototype.toFixed`'s
 * decimal half-up rounding, `Math.floor` on the fixed-length month/year, and
 * `Intl.DateTimeFormat('en-US', { dateStyle: 'medium', timeStyle: 'short' })`'s
 * exact bytes — including which space character sits before `AM`/`PM`.
 *
 * ⚠ `TZ=UTC` is REQUIRED. `formatInstant`'s `catch` falls back to the host zone
 * when a timezone is unresolvable or absent, and the Rust twin pins that
 * fallback to UTC (the documented harness seam shared with
 * `context_feeders_leaves_equivalence`).
 *
 * Run (Node 24, from the v4 checkout; a pinned worktree while v4 HEAD is past
 * the baseline — the driver rewrites the `cd`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/node --import tsx $V5W/harness/oracle/cases/progressions-engine.ts \
 *     > /tmp/oracle-progressions-engine.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  ProgressionSchema,
  PROGRESSION_ID_PATTERN,
  REPORT_FREQUENCY_PATTERN,
  isWritableProgressionField,
  parseIsoInstant,
  parseProgressKey,
  type Progression,
  type TimeIncrement,
} from '@/lib/progressions/schema';

import {
  defaultInProgressTemplate,
  deriveProgression,
  flattenProgressions,
  formatSpan,
  formatSpanWhole,
  inferIncrement,
  parseProgressions,
  parseReportPeriodMs,
  progressionPlaceholders,
  renderProgressionReport,
  shouldReportProgression,
} from '@/lib/progressions/engine';

interface Corpus {
  parseIsoInstant: Array<{ label: string; value: unknown }>;
  parseProgression: Array<{ label: string; entry: unknown }>;
  parseProgressions: Array<{ label: string; metadata: unknown }>;
  formatSpan: Array<{ label: string; ms: number | string; unit: TimeIncrement }>;
  formatSpanWhole: Array<{ label: string; ms: number | string; unit: TimeIncrement }>;
  deriveProgression: Array<{ label: string; id: string; entry: unknown; nowMs: number }>;
  shouldReportProgression: Array<{
    label: string;
    id: string;
    entry: unknown;
    nowMs: number;
    lastTurnMs: number | null;
    /** v4's own test casts a malformed `updatedAt` onto an already-parsed
     *  progression; the schema refuses one at the door, so rule 2's NaN arm is
     *  reachable only by forcing the field after parsing. */
    updatedAtOverride: string | null;
  }>;
  parseReportPeriodMs: Array<{ label: string; value: string }>;
  renderProgressionReport: Array<{
    label: string;
    id: string;
    entry: unknown;
    nowMs: number;
    timezone: string | null;
  }>;
  progressionPlaceholders: Corpus['renderProgressionReport'];
  defaultInProgressTemplate: Array<{ label: string; entry: unknown }>;
  flattenProgressions: Array<{ label: string; metadata: unknown; nowMs: number }>;
  inferIncrement: Array<{ label: string; spanMs: number }>;
  parseProgressKey: Array<{ label: string; key: string }>;
  isWritableProgressionField: Array<{ label: string; field: string }>;
  isProgressionId: Array<{ label: string; id: string }>;
  isReportFrequency: Array<{ label: string; value: string }>;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'progressions-engine.json'), 'utf8'),
) as Corpus;

const out = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify(row) + '\n');

/** A JS number, or `null` where the value is NaN/±Infinity — JSON has neither. */
const num = (x: number): number | null => (Number.isFinite(x) ? x : null);

/** The corpus spells a non-finite `ms` as a string; JSON cannot hold one. */
const ms = (v: number | string): number =>
  typeof v === 'number' ? v : v === 'NaN' ? NaN : v === 'Infinity' ? Infinity : -Infinity;

/** v4's own joined-issue sentence — what `parseProgressions` hands `onIssue`. */
const joined = (issues: Array<{ path: PropertyKey[]; message: string }>): string =>
  issues.map((i) => `${i.path.join('.') || '(root)'}: ${i.message}`).join('; ');

/** A parsed progression, for a case whose corpus entry is known valid. */
const parsed = (entry: unknown): Progression => ProgressionSchema.parse(entry);

for (const c of corpus.parseIsoInstant) {
  out({ op: 'parseIsoInstant', label: c.label, result: num(parseIsoInstant(c.value)) });
}

for (const c of corpus.parseProgression) {
  const r = ProgressionSchema.safeParse(c.entry);
  out({
    op: 'parseProgression',
    label: c.label,
    ok: r.success,
    // The parsed record, serialized in `strictObject`'s own key order — the
    // Tier-2 key-order pin the Rust `Progression`'s `Serialize` answers.
    parsedValue: r.success ? r.data : null,
    issues: r.success ? null : joined(r.error.issues),
  });
}

for (const c of corpus.parseProgressions) {
  const issues: Array<[string, string]> = [];
  const kept = parseProgressions(c.metadata, (id, issue) => issues.push([id, issue]));
  out({ op: 'parseProgressions', label: c.label, ids: Object.keys(kept), issues });
}

for (const c of corpus.formatSpan) {
  out({ op: 'formatSpan', label: c.label, result: formatSpan(ms(c.ms), c.unit) });
}
for (const c of corpus.formatSpanWhole) {
  out({ op: 'formatSpanWhole', label: c.label, result: formatSpanWhole(ms(c.ms), c.unit) });
}

const derivedRow = (id: string, entry: unknown, nowMs: number) => {
  const d = deriveProgression(id, parsed(entry), nowMs);
  return {
    id: d.id,
    name: d.name,
    startMs: num(d.startMs),
    endMs: num(d.endMs),
    nowMs: d.nowMs,
    elapsedMs: num(d.elapsedMs),
    remainingMs: num(d.remainingMs),
    percent: num(d.percent),
    percentClamped: num(d.percentClamped),
    state: d.state,
    started: d.started,
    complete: d.complete,
    // `undefined` when the progression carries no quantity — the absent/`null`
    // distinction the Rust `Option` answers.
    quantityCurrent: d.quantityCurrent === undefined ? null : num(d.quantityCurrent),
    hasQuantityCurrent: d.quantityCurrent !== undefined,
    elapsed: d.elapsed,
    elapsedWhole: d.elapsedWhole,
    remaining: d.remaining,
    remainingWhole: d.remainingWhole,
  };
};

for (const c of corpus.deriveProgression) {
  out({ op: 'deriveProgression', label: c.label, derived: derivedRow(c.id, c.entry, c.nowMs) });
}

for (const c of corpus.shouldReportProgression) {
  const base = parsed(c.entry);
  const p =
    c.updatedAtOverride === null
      ? base
      : ({ ...base, updatedAt: c.updatedAtOverride } as Progression);
  const d = deriveProgression(c.id, p, c.nowMs);
  const r = shouldReportProgression(p, d, c.lastTurnMs);
  out({ op: 'shouldReportProgression', label: c.label, report: r.report, reason: r.reason });
}

for (const c of corpus.parseReportPeriodMs) {
  out({ op: 'parseReportPeriodMs', label: c.label, result: parseReportPeriodMs(c.value) });
}

for (const c of corpus.renderProgressionReport) {
  const p = parsed(c.entry);
  const d = deriveProgression(c.id, p, c.nowMs);
  const opts = c.timezone === null ? {} : { timezone: c.timezone };
  out({
    op: 'renderProgressionReport',
    label: c.label,
    result: renderProgressionReport(p, d, opts),
  });
}

for (const c of corpus.progressionPlaceholders) {
  const p = parsed(c.entry);
  const d = deriveProgression(c.id, p, c.nowMs);
  const opts = c.timezone === null ? {} : { timezone: c.timezone };
  const values = progressionPlaceholders(p, d, opts);
  out({
    op: 'progressionPlaceholders',
    label: c.label,
    // Key ORDER is a comparand — v4's declaration order, which the Rust twin
    // emits as an ordered pair list.
    keys: Object.keys(values),
    values,
  });
}

for (const c of corpus.defaultInProgressTemplate) {
  out({
    op: 'defaultInProgressTemplate',
    label: c.label,
    result: defaultInProgressTemplate(parsed(c.entry)),
  });
}

for (const c of corpus.flattenProgressions) {
  const issues: Array<[string, string]> = [];
  const sheet = flattenProgressions(c.metadata, c.nowMs, (id, issue) => issues.push([id, issue]));
  out({
    op: 'flattenProgressions',
    label: c.label,
    keys: Object.keys(sheet),
    sheet,
    issues,
  });
}

for (const c of corpus.inferIncrement) {
  out({ op: 'inferIncrement', label: c.label, result: inferIncrement(c.spanMs) });
}

for (const c of corpus.parseProgressKey) {
  const r = parseProgressKey(c.key);
  out({
    op: 'parseProgressKey',
    label: c.label,
    ok: r.ok,
    id: r.ok ? r.id : null,
    field: r.ok ? r.field : null,
    reason: r.ok ? null : r.reason,
  });
}

for (const c of corpus.isWritableProgressionField) {
  out({
    op: 'isWritableProgressionField',
    label: c.label,
    result: isWritableProgressionField(c.field),
  });
}
for (const c of corpus.isProgressionId) {
  out({ op: 'isProgressionId', label: c.label, result: PROGRESSION_ID_PATTERN.test(c.id) });
}
for (const c of corpus.isReportFrequency) {
  out({ op: 'isReportFrequency', label: c.label, result: REPORT_FREQUENCY_PATTERN.test(c.value) });
}
