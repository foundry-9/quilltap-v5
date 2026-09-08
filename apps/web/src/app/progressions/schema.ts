/**
 * Character progressions — the schema (v4 `lib/progressions/schema.ts` at
 * `25f534c0b`, the CLIENT-SAFE module, transcribed).
 *
 * A **progression** is a named, bounded span of time carried by a character: a
 * start instant, an end instant, and the rules for how — and how often — its
 * state is reported back to that character at the top of a turn. A pregnancy
 * that began on 1 August and is due 1 May; a ship's cannon that takes ten
 * minutes to recharge; a fermentation that finishes in three weeks.
 *
 * They live under **one reserved top-level key, `progressions`**, in the
 * character vault's `metadata.json` — Pascal's one character-scoped store, so a
 * custom tool can read and change them without a second hydration, write or
 * export path. Every other metadata key stays freeform.
 *
 * ## Why this reimplements Zod rather than importing it
 *
 * v4 validates with Zod 4.5.4. The SPA has **no `zod` dependency** (measured
 * 2026-09-08: absent from `apps/web/package.json`, imported nowhere; the 4.3.6
 * under `node_modules` is transitive to the Angular builder) — the work order's
 * "the SPA HAS zod" premise is refuted, and this file is the deviation. It is a
 * hand port in `pascal/custom-tool-types.ts`'s idiom, over the leaf validators
 * both now share (`pascal/zod-shim.ts`), and it cannot be approximate: the
 * editor modal renders `${issue.path.join('.') || 'This entry'}: ${message}`
 * straight to the author, so a browser that phrased a rejection differently
 * would be disagreeing with the server about the same entry.
 *
 * Every sentence and every abort/continue flag below was MEASURED against v4's
 * real `ProgressionSchema` at `25f534c0b` under Zod 4.5.4; the recording and
 * its recipe are `schema.oracle.spec.ts`.
 *
 * CLIENT-SAFE, as v4's is: no logging, no I/O, no server imports — the Aurora
 * editor and Pascal's Workbench both run this in the browser.
 *
 * @module progressions/schema
 */

import {
  aborted,
  checkIssue,
  hardIssue,
  hasKey,
  invalidType,
  isPlainObject,
  parseBool,
  parseEnum,
  prefix,
  resHard,
  resOk,
  stringLengthIssues,
  unrecognizedKeys,
  type Issue,
  type Res,
} from '../pascal/zod-shim';

/**
 * The identifier rule for a progression id — the same shape a custom tool's
 * name takes. Pascal addresses a progression by id in a tool file written
 * before the character exists (`progress.cannon.complete`), so the id has to be
 * a stable, typeable token, decoupled from the display `name` the user is free
 * to edit.
 */
export const PROGRESSION_ID_PATTERN = /^[a-z][a-z0-9_-]{0,63}$/;

/** The unit a report *speaks in*. It never affects computation, only phrasing. */
export const TIME_INCREMENTS = [
  'second',
  'minute',
  'hour',
  'day',
  'week',
  'month',
  'year',
] as const;

export type TimeIncrement = (typeof TIME_INCREMENTS)[number];

/**
 * The cadence grammar — a small closed set, not free text and not cron:
 *
 * - `turn` — every prompted turn.
 * - `increment` — only when the whole-unit count of `timeIncrement` has ticked
 *   over since the character last spoke (week 20 → week 21).
 * - `<n><unit>` where unit is `s`/`m`/`h`/`d`/`w` — only when the wall-clock
 *   bucket of that period has changed (`1h`: at most once per clock hour).
 */
export const REPORT_FREQUENCY_PATTERN = /^(turn|increment|[1-9]\d{0,4}[smhdw])$/;

/**
 * What happens once `endTime` has passed.
 *
 * - `keep` — the completed state keeps being reported on the same cadence
 *   ("complete; 3 days past due").
 * - `once` — the completion is reported exactly on the first turn after it
 *   happened, then the progression goes silent. It stays in the file, still
 *   readable by Pascal, until a user or a tool removes it.
 */
export const ON_COMPLETE_OPTIONS = ['keep', 'once'] as const;

export type OnComplete = (typeof ON_COMPLETE_OPTIONS)[number];

/** A ceiling, not a design constraint — a character carrying 32 timed conditions is already an outlier. */
export const MAX_PROGRESSIONS_PER_CHARACTER = 32;

export const MAX_REPORT_TEMPLATE_LENGTH = 500;
export const MAX_PROGRESSION_NAME_LENGTH = 80;
export const MAX_PROGRESSION_DESCRIPTION_LENGTH = 500;
export const MAX_QUANTITY_UNIT_LENGTH = 16;

/**
 * An ISO-8601 instant with an offset or `Z`. Deliberately stricter than
 * `new Date(x)`, which happily takes `"2026"` and a good deal of prose: a
 * progression's whole value is that its arithmetic is deterministic, and a
 * timestamp with no zone is not an instant.
 */
export const ISO_DATE_TIME_MESSAGE =
  'must be an ISO-8601 date-time with an offset or Z (e.g. 2026-08-01T00:00:00Z)';

const ISO_INSTANT_PATTERN =
  /^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}(:\d{2}(\.\d{1,9})?)?([Zz]|[+-]\d{2}:?\d{2})$/;

/**
 * Parse an ISO-8601 instant to epoch milliseconds, or `NaN`. The one parser —
 * the schema's refinement and the engine's derivation share it, so "what counts
 * as a timestamp" is decided exactly once.
 */
export function parseIsoInstant(value: unknown): number {
  if (typeof value !== 'string') return NaN;
  const trimmed = value.trim();
  if (!ISO_INSTANT_PATTERN.test(trimmed)) return NaN;
  return Date.parse(trimmed);
}

/**
 * An optional scalar the span fills, so a recharge can be reported in
 * megajoules rather than only in percent. `current` is derived —
 * `total × clamp(percent) / 100` — never stored.
 */
export interface ProgressionQuantity {
  total: number;
  unit: string;
  precision: number;
}

/** One progression, as the schema hands it back — defaults filled in. */
export interface Progression {
  /** The display name. Free to change; the id is what tool files address. */
  name: string;
  /** Second person, in the user's own words. Rendered by `{{description}}`. */
  description?: string;
  startTime: string;
  endTime: string;
  timeIncrement: TimeIncrement;
  /** Whether the DEFAULT report includes ", 45% complete". `{{percent}}` ignores it. */
  percentageReport: boolean;
  reportFrequency: string;
  quantity?: ProgressionQuantity;
  /** Overrides the in-progress wording only. Plain substitution, no logic. */
  reportTemplate?: string;
  onComplete: OnComplete;
  /**
   * Stamped by every writer Quilltap controls (the Aurora editor, Pascal's
   * applier). An entry whose `updatedAt` is later than the character's last
   * turn reports regardless of cadence — a re-armed cannon is announced
   * immediately. A hand edit through the file manager may omit it; then the
   * next report simply waits for the cadence.
   */
  updatedAt?: string;
}

/** A record of progressions keyed by identifier — the reserved key's shape. */
export type Progressions = Record<string, Progression>;

/** The reserved key itself, so no reader has to spell it. */
export const PROGRESSIONS_METADATA_KEY = 'progressions';

/** v4's `ProgressionSchema.safeParse` result, in the shim's shape. */
export type ProgressionParseResult =
  { success: true; data: Progression } | { success: false; issues: Issue[] };

/**
 * v4's `z.number()` — which in Zod v4 refuses `NaN` and `±Infinity` at the TYPE
 * check (`zod/v4/core/schemas.js:586`), naming the offending value in
 * `received`, so `.finite()` adds nothing. Aborting.
 */
function parseNumber(input: unknown): Res<number> {
  if (typeof input === 'number' && !Number.isNaN(input) && Number.isFinite(input)) {
    return resOk(input);
  }
  const received =
    typeof input === 'number' ? (Number.isNaN(input) ? 'NaN' : String(input)) : undefined;
  return resHard(
    received === undefined
      ? invalidType('number', input)
      : `Invalid input: expected number, received ${received}`,
  );
}

/** `z.string()` plus the optional length caps — the length issues are CHECKS. */
function parseString(input: unknown, min?: number, max?: number): Res<string> {
  if (typeof input !== 'string') return resHard(invalidType('string', input));
  return { value: input, issues: stringLengthIssues(input, min, max) };
}

/**
 * `IsoDateTimeSchema` — `z.string().refine(…)`. The type failure aborts; the
 * refinement is a check, so a zoneless timestamp still lets the object's own
 * `superRefine` run (measured: `zoneless-both` reports both fields and no
 * third issue, because `parseIsoInstant` is `NaN` on each).
 */
function parseIsoDateTime(input: unknown): Res<string> {
  const parsed = parseString(input);
  if (parsed.value === undefined) return parsed;
  const issues = [...parsed.issues];
  if (!Number.isFinite(parseIsoInstant(parsed.value))) {
    issues.push(checkIssue(ISO_DATE_TIME_MESSAGE));
  }
  return { value: parsed.value, issues };
}

/**
 * `ProgressionQuantitySchema` — a `z.strictObject`. `precision` defaults to 1;
 * `z.number().int()` is a FORMAT check and aborts (measured: `int-plus-refine`
 * suppresses the outer refine), while `>0`, `>=0` and `<=6` are ordinary checks
 * and do not.
 */
function parseQuantity(input: unknown): Res<ProgressionQuantity> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];

  const total = parseNumber(input['total']);
  const totalIssues = [...total.issues];
  if (total.value !== undefined && !(total.value > 0)) {
    totalIssues.push(checkIssue('Too small: expected number to be >0'));
  }
  issues.push(...prefix('total', totalIssues));

  const unit = parseString(input['unit'], 1, MAX_QUANTITY_UNIT_LENGTH);
  issues.push(...prefix('unit', unit.issues));

  let precision = 1;
  if (hasKey(input, 'precision') && input['precision'] !== undefined) {
    const parsed = parseNumber(input['precision']);
    const own = [...parsed.issues];
    if (parsed.value !== undefined) {
      if (!Number.isInteger(parsed.value)) {
        own.push(hardIssue('Invalid input: expected int, received number'));
      } else {
        if (parsed.value < 0) own.push(checkIssue('Too small: expected number to be >=0'));
        if (parsed.value > 6) own.push(checkIssue('Too big: expected number to be <=6'));
      }
      precision = parsed.value;
    }
    issues.push(...prefix('precision', own));
  }

  issues.push(...unrecognizedKeys(input, ['total', 'unit', 'precision']));

  const value =
    total.value !== undefined && unit.value !== undefined && !aborted(issues)
      ? { total: total.value, unit: unit.value, precision }
      : undefined;
  return { value, issues };
}

/** Every key `ProgressionSchema` declares, in declaration order. */
const PROGRESSION_KEYS = [
  'name',
  'description',
  'startTime',
  'endTime',
  'timeIncrement',
  'percentageReport',
  'reportFrequency',
  'quantity',
  'reportTemplate',
  'onComplete',
  'updatedAt',
] as const;

/**
 * v4's `ProgressionSchema.safeParse`. Key order in the returned object is the
 * DECLARATION order above, with absent optionals omitted — measured against
 * v4's own `JSON.stringify(parsed)`.
 */
export function safeParseProgression(input: unknown): ProgressionParseResult {
  if (!isPlainObject(input)) {
    return { success: false, issues: [hardIssue(invalidType('object', input))] };
  }

  const issues: Issue[] = [];
  const out: Record<string, unknown> = {};

  const name = parseString(input['name'], 1, MAX_PROGRESSION_NAME_LENGTH);
  issues.push(...prefix('name', name.issues));
  if (name.value !== undefined) out['name'] = name.value;

  if (hasKey(input, 'description') && input['description'] !== undefined) {
    const description = parseString(
      input['description'],
      undefined,
      MAX_PROGRESSION_DESCRIPTION_LENGTH,
    );
    issues.push(...prefix('description', description.issues));
    if (description.value !== undefined) out['description'] = description.value;
  }

  const startTime = parseIsoDateTime(input['startTime']);
  issues.push(...prefix('startTime', startTime.issues));
  if (startTime.value !== undefined) out['startTime'] = startTime.value;

  const endTime = parseIsoDateTime(input['endTime']);
  issues.push(...prefix('endTime', endTime.issues));
  if (endTime.value !== undefined) out['endTime'] = endTime.value;

  const timeIncrement = parseEnum(input['timeIncrement'], TIME_INCREMENTS);
  issues.push(...prefix('timeIncrement', timeIncrement.issues));
  if (timeIncrement.value !== undefined) out['timeIncrement'] = timeIncrement.value;

  if (hasKey(input, 'percentageReport') && input['percentageReport'] !== undefined) {
    const flag = parseBool(input['percentageReport']);
    issues.push(...prefix('percentageReport', flag.issues));
    out['percentageReport'] = flag.value ?? true;
  } else {
    out['percentageReport'] = true;
  }

  if (hasKey(input, 'reportFrequency') && input['reportFrequency'] !== undefined) {
    const frequency = parseString(input['reportFrequency']);
    const own = [...frequency.issues];
    if (frequency.value !== undefined && !REPORT_FREQUENCY_PATTERN.test(frequency.value)) {
      own.push(checkIssue(`Invalid string: must match pattern ${REPORT_FREQUENCY_PATTERN}`));
    }
    issues.push(...prefix('reportFrequency', own));
    out['reportFrequency'] = frequency.value ?? 'turn';
  } else {
    out['reportFrequency'] = 'turn';
  }

  if (hasKey(input, 'quantity') && input['quantity'] !== undefined) {
    const quantity = parseQuantity(input['quantity']);
    issues.push(...prefix('quantity', quantity.issues));
    if (quantity.value !== undefined) out['quantity'] = quantity.value;
  }

  if (hasKey(input, 'reportTemplate') && input['reportTemplate'] !== undefined) {
    const template = parseString(input['reportTemplate'], 1, MAX_REPORT_TEMPLATE_LENGTH);
    issues.push(...prefix('reportTemplate', template.issues));
    if (template.value !== undefined) out['reportTemplate'] = template.value;
  }

  if (hasKey(input, 'onComplete') && input['onComplete'] !== undefined) {
    const onComplete = parseEnum(input['onComplete'], ON_COMPLETE_OPTIONS);
    issues.push(...prefix('onComplete', onComplete.issues));
    out['onComplete'] = onComplete.value ?? 'keep';
  } else {
    out['onComplete'] = 'keep';
  }

  if (hasKey(input, 'updatedAt') && input['updatedAt'] !== undefined) {
    const updatedAt = parseIsoDateTime(input['updatedAt']);
    issues.push(...prefix('updatedAt', updatedAt.issues));
    if (updatedAt.value !== undefined) out['updatedAt'] = updatedAt.value;
  }

  issues.push(...unrecognizedKeys(input, PROGRESSION_KEYS));

  // The cross-field rule is a `superRefine`, so Zod skips it once the object
  // carries an ABORTING issue — but not for a merely continuable one, which is
  // why an unrecognized key still lets it speak (measured:
  // `unknown-key-plus-backwards` reports both).
  if (!aborted(issues)) {
    const start = parseIsoInstant(out['startTime']);
    const end = parseIsoInstant(out['endTime']);
    if (Number.isFinite(start) && Number.isFinite(end) && end <= start) {
      issues.push(checkIssue('must be strictly after startTime', ['endTime']));
    }
  }

  if (issues.length > 0) return { success: false, issues };
  return { success: true, data: reorder(out) };
}

/** Rebuild the parsed object in v4's declaration order, omitting absent optionals. */
function reorder(out: Record<string, unknown>): Progression {
  const ordered: Record<string, unknown> = {};
  for (const key of PROGRESSION_KEYS) {
    if (hasKey(out, key)) ordered[key] = out[key];
  }
  return ordered as unknown as Progression;
}

/** The whole reserved key. Only the mirror-agreement spec needs this — the
 * fail-soft reader (`engine.ts`'s `parseProgressions`) never uses it. */
export type ProgressionsParseResult =
  { success: true; data: Progressions } | { success: false; issues: Issue[] };

/**
 * `ProgressionsSchema` — `z.record(id, ProgressionSchema)` refined to ≤ 32
 * entries. A key that fails the identifier rule reports `Invalid key in record`
 * at that key's path and its VALUE is not reported (measured:
 * `rec-bad-key-and-bad-value`).
 */
export function safeParseProgressions(input: unknown): ProgressionsParseResult {
  if (!isPlainObject(input)) {
    return { success: false, issues: [hardIssue(invalidType('record', input))] };
  }

  const issues: Issue[] = [];
  const data: Progressions = {};

  for (const [id, entry] of Object.entries(input)) {
    if (!PROGRESSION_ID_PATTERN.test(id)) {
      issues.push({ path: [id], message: 'Invalid key in record', cont: false });
      continue;
    }
    const parsed = safeParseProgression(entry);
    if (parsed.success) data[id] = parsed.data;
    else issues.push(...prefix(id, parsed.issues));
  }

  if (!aborted(issues) && Object.keys(input).length > MAX_PROGRESSIONS_PER_CHARACTER) {
    issues.push(
      checkIssue(`a character may carry at most ${MAX_PROGRESSIONS_PER_CHARACTER} progressions`),
    );
  }

  if (issues.length > 0) return { success: false, issues };
  return { success: true, data };
}

/**
 * Split a `"<id>.<field>"` progress key into its halves, or say why it is not
 * one. The single parser for that shape: Pascal's read schema
 * (`ProgressKeySchema`), the Workbench's condition validator, and anything that
 * follows all come through here, so the identifier rule lives in exactly one
 * place and cannot drift between them.
 *
 * The `<field>` half is checked for shape only, deliberately — which derived
 * fields exist is the engine's business, and an unknown one fails soft at run
 * time exactly as an absent metadata key does. What IS worth catching at load
 * time is a key that names no id or no field at all.
 */
export function parseProgressKey(
  key: string,
): { ok: true; id: string; field: string } | { ok: false; reason: string } {
  const dot = key.indexOf('.');
  if (dot < 0) {
    return {
      ok: false,
      reason: `"${key}" names no field — write "<progression id>.<field>", e.g. "cannon.complete"`,
    };
  }
  const id = key.slice(0, dot);
  const field = key.slice(dot + 1);
  if (!PROGRESSION_ID_PATTERN.test(id)) {
    return {
      ok: false,
      reason: `"${id}" is not a progression id — lowercase, starting with a letter, then letters, digits, _ or - (at most 64)`,
    };
  }
  if (!/^[a-zA-Z]+$/.test(field)) {
    return { ok: false, reason: `"${field}" is not a progression field name` };
  }
  return { ok: true, id, field };
}

/**
 * The fields a Pascal effect may write, plus the `remove` pseudo-field. Kept
 * here beside the schema so the writable set and the schema can never drift.
 */
export const WRITABLE_PROGRESSION_FIELDS = [
  'name',
  'description',
  'startTime',
  'endTime',
  'timeIncrement',
  'percentageReport',
  'reportFrequency',
  'onComplete',
  'reportTemplate',
  'quantity.total',
  'quantity.unit',
  'quantity.precision',
  'remove',
] as const;

export type WritableProgressionField = (typeof WRITABLE_PROGRESSION_FIELDS)[number];

export function isWritableProgressionField(field: string): field is WritableProgressionField {
  return (WRITABLE_PROGRESSION_FIELDS as readonly string[]).includes(field);
}
