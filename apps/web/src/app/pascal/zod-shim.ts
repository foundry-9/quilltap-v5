/**
 * The hand-rolled Zod 4.5.4 shim — the issue model and leaf validators shared by
 * every browser-side schema twin.
 *
 * The SPA has NO `zod` dependency (verified 2026-09-08: absent from
 * `apps/web/package.json`, imported nowhere; the copy under `node_modules` is a
 * transitive 4.3.6 the Angular builder pulls). Its schema twins therefore
 * reimplement the slice of Zod v4's engine that their sentences depend on —
 * and those sentences are payload, not debugging aids, so a second copy of the
 * engine drifting from this one would be a browser disagreeing with the server
 * about the same file.
 *
 * Extracted from `custom-tool-types.ts` (a pure move, proven by that module's
 * five spec files and the 301-row committed corpus staying green) when
 * `progressions/schema.ts` became the second consumer at P4.D170. The rules
 * carried here are the three `custom-tool-types.ts` documents at length, plus
 * Zod ≥ 4.5.4's code-point string lengths (`./zod-length`).
 *
 * ⚠ v4's own progressions modules use REAL Zod. The work order's premise that
 * "the SPA HAS zod" was refuted by measurement; this shim is the deviation, and
 * every sentence it renders for a progression was measured against v4's real
 * `ProgressionSchema` at `25f534c0b` — see
 * `../progressions/schema.oracle.spec.ts` for the recorded corpus and its
 * recipe.
 *
 * @module pascal/zod-shim
 */

import { zodLenMaxOk, zodLenMinOk } from './zod-length';

/**
 * One rejection, in Zod's shape. `cont` mirrors Zod's `continue` flag: true only
 * for issues raised by a check, which is what makes them non-aborting.
 */
export interface Issue {
  path: string[];
  message: string;
  cont: boolean;
  /**
   * Present only for `invalid_union`: each branch's issues, with paths left
   * RELATIVE to the union (v4's `flattenIssues` re-walks them without the
   * prefix, so they must not be prefixed here).
   */
  unionErrors?: Issue[][];
}

/** An aborting issue — anything not raised by a check. */
export function hardIssue(message: string): Issue {
  return { path: [], message, cont: false };
}

/** A continuable issue — Zod's `ctx.addIssue` / `refine` shape. */
export function checkIssue(message: string, path: string[] = []): Issue {
  return { path, message, cont: true };
}

/** Prefix one issue's path with `seg`. */
export function at(issue: Issue, seg: string): Issue {
  return { ...issue, path: [seg, ...issue.path] };
}

/** v4 `util.aborted`: a payload is aborted once any issue is not continuable. */
export function aborted(issues: readonly Issue[]): boolean {
  return issues.some((i) => !i.cont);
}

/**
 * Prefix every issue's path with `seg`. Deliberately does NOT touch
 * `unionErrors` — Zod keeps branch paths relative to the union.
 */
export function prefix(seg: string, issues: Issue[]): Issue[] {
  return issues.map((i) => at(i, seg));
}

/** A parse result: the value when one could be built, plus any issues. */
export interface Res<T> {
  value: T | undefined;
  issues: Issue[];
}

export function resOk<T>(value: T): Res<T> {
  return { value, issues: [] };
}

/** Shape failure — no value, and the issue aborts. */
export function resHard<T>(message: string): Res<T> {
  return { value: undefined, issues: [hardIssue(message)] };
}

/**
 * v4 `handleUnionResults`. The middle arm is the subtle one: when exactly one
 * branch is non-aborted, Zod returns THAT branch rather than wrapping, which is
 * how a failed `refine` inside one branch surfaces its own sentence instead of a
 * bare "Invalid input" at the union.
 */
export function union<T>(branches: Res<T>[]): Res<T> {
  const clean = branches.find((b) => b.issues.length === 0);
  if (clean) return clean;

  const nonAborted = branches.filter((b) => !aborted(b.issues));
  if (nonAborted.length === 1) return nonAborted[0];

  return {
    value: undefined,
    issues: [
      {
        path: [],
        message: 'Invalid input',
        cont: false,
        unionErrors: branches.map((b) => b.issues),
      },
    ],
  };
}

/** v4 `util.parsedType`. `undefined` is a missing key. */
export function parsedType(v: unknown): string {
  if (v === undefined) return 'undefined';
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  const t = typeof v;
  if (t === 'boolean' || t === 'number' || t === 'string' || t === 'object') return t;
  return t;
}

export function invalidType(expected: string, got: unknown): string {
  return `Invalid input: expected ${expected}, received ${parsedType(got)}`;
}

/** A JSON object — `typeof 'object'` minus null and arrays. */
export function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** Mirrors Rust's `map.contains_key` / Zod's own key-presence test. */
export function hasKey(obj: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(obj, key);
}

// ------------------------------------------------------------ leaf validators

/**
 * v4's `z.number().finite()` expectation string. See `custom-tool-types.ts`'s
 * module note: this is the one arm the oracle corpus does not pin.
 */
export const FINITE_EXPECTED = 'number';

/**
 * The `min`/`max` half of `z.string()`, as issues — Zod's own built-in
 * sentences, measured in CODE POINTS since 4.5.4 (`./zod-length`). Both checks
 * run, so a string can carry two.
 */
export function stringLengthIssues(
  input: string,
  min: number | undefined,
  max: number | undefined,
): Issue[] {
  const issues: Issue[] = [];
  if (min !== undefined && !zodLenMinOk(input, min)) {
    issues.push(checkIssue(`Too small: expected string to have >=${min} characters`));
  }
  if (max !== undefined && !zodLenMaxOk(input, max)) {
    issues.push(checkIssue(`Too big: expected string to have <=${max} characters`));
  }
  return issues;
}

export function parseBool(input: unknown): Res<boolean> {
  if (typeof input === 'boolean') return resOk(input);
  return resHard(invalidType('boolean', input));
}

/**
 * v4 `z.number().finite()`. Unlike the Rust port, this arm IS reachable here:
 * `JSON.parse('{"gt":1e999}')` yields `Infinity` in a browser exactly as it does
 * in v4's Node. See the module note — it is the one message the corpus cannot
 * pin.
 */
export function parseFiniteNumber(input: unknown): Res<number> {
  if (typeof input !== 'number') return resHard(invalidType('number', input));
  if (!Number.isFinite(input)) return resHard(invalidType(FINITE_EXPECTED, input));
  return resOk(input);
}

export function parseEnum<T extends string>(input: unknown, options: readonly T[]): Res<T> {
  if (typeof input === 'string' && (options as readonly string[]).includes(input)) {
    return resOk(input as T);
  }
  const list = options.map((o) => `"${o}"`).join('|');
  return resHard(`Invalid option: expected one of ${list}`);
}

/**
 * The strict-object tail: report keys outside `known` as ONE issue, in the
 * input's own order. Zod raises this AFTER the shape. Since Zod 4.5.4 (v4
 * `6e1a64ea6`, `zod` 4.4.3 → 4.5.4) the issue carries `continue: true`: the
 * object is NOT aborted, its refines still run, and inside a union it stays a
 * live branch — so a `when: true | {…}` with a stray key now reports the object
 * branch alone (with its refine) where 4.4.3 wrapped both branches under
 * `invalid_union`. The server twin (`custom_tool_types.rs::unrecognized_keys`)
 * carries the same flag; `pascal_custom_tool_definition_equivalence` is the pin.
 */
export function unrecognizedKeys(obj: Record<string, unknown>, known: readonly string[]): Issue[] {
  const extra = Object.keys(obj).filter((k) => !known.includes(k));
  if (extra.length === 0) return [];
  const quoted = extra.map((k) => `"${k}"`).join(', ');
  return [
    checkIssue(extra.length === 1 ? `Unrecognized key: ${quoted}` : `Unrecognized keys: ${quoted}`),
  ];
}
