/**
 * Custom Tools — the definition schema for Pascal's table (v4
 * `lib/pascal/custom-tool.types.ts`, the CLIENT-SAFE validation half).
 *
 * A custom tool is a single JSON document (`Tools/*.tool.json` at the root of
 * any document store) describing a named action with parameters, a random roll,
 * and an ordered table of outcomes mapping the roll to a message and a semantic
 * state. This module is the browser's copy of the format's single source of
 * truth: Pascal's Workbench validates a draft with the same rules the roster
 * loader runs on the server, which is the whole design.
 *
 * Design constraint that shapes everything below: **no expression evaluation,
 * anywhere.** Outcome tests are AND-composed comparator objects, and indirection
 * is limited to two closed forms — a `{ "$param": "name" }` reference and a
 * `{ "$state": "path", "fallback": <literal> }` reference. There is no string
 * grammar to parse, so there is nothing to inject into.
 *
 * # Why this file reimplements a slice of Zod
 *
 * v4 validates with Zod; the SPA has NO zod dependency (verified: not in
 * package.json, imported nowhere). So this is a hand port — and it cannot be an
 * approximate one, because [`formatDefinitionIssues`] is not a debugging aid:
 * `loadToolsFromMount` stores its output as a load error's `reason`, and both
 * the chat roster route and the Workbench library route return that string
 * VERBATIM. The Workbench editor renders the same sentence for a draft the
 * author is repairing, so a browser that phrased it differently would be
 * disagreeing with the server about the same file.
 *
 * Three of Zod 4.4.3's rules are therefore load-bearing here, and are why this
 * is a small engine rather than a pile of `if`s:
 *
 * 1. An issue is **aborting** unless it came from a check (a `refine` /
 *    `superRefine`), which marks its issues `continue: true`.
 * 2. **Checks are skipped when the value already has an aborting issue** — that
 *    is what stops the top-level `superRefine` from walking an `outcomes` that
 *    failed to parse. When only continuable issues exist, the value still
 *    exists and the checks DO run.
 * 3. A union whose branches all fail reports `invalid_union` — **unless exactly
 *    one branch is non-aborted**, in which case that branch's issues are hoisted
 *    verbatim and no union wrapper appears (Zod's `handleUnionResults`). This is
 *    why a malformed dice notation reads as `roll: must be dice notation …`
 *    rather than being buried under an `Invalid input` at the union.
 *
 * The port is pinned byte-for-byte against v4's REAL Zod by
 * `custom-tool-types.corpus.spec.ts`, which replays the committed 115-row
 * oracle corpus (105 definitions + 10 titles) and compares the verdict, the
 * parsed data, the unknown-key report, and the full rejection sentence.
 *
 * # The one arm the corpus cannot cover: overflow literals
 *
 * `z.number().finite()` is reachable only via a JSON literal that overflows to
 * `Infinity` (JSON has no `Infinity`/`NaN` literal), e.g. `{"gt": 1e999}`.
 * Being JS, this port reaches that arm exactly as v4 does — the Rust port
 * cannot, since serde_json rejects the literal one layer earlier at the parse,
 * which is the divergence recorded in `pascal/custom_tool_types.rs`. The oracle
 * corpus carries no row for it (there is nothing there for the Rust side to
 * assert), so {@link FINITE_EXPECTED} is carried faithfully from that port but
 * is the one message in this file NOT pinned by the differential.
 */

import { MAX_DIE_SIDES, MIN_DIE_SIDES, parseDiceNotation } from './dice-notation';

/** Well-known folder, at a store's root, holding custom-tool definitions. */
export const TOOLS_FOLDER = 'Tools';

/** Filename suffix that marks a document as a custom-tool definition. */
export const TOOL_FILE_SUFFIX = '.tool.json';

/** Cap on parameters per tool. */
export const MAX_PARAMETERS = 8;

/** Cap on outcomes per tool. */
export const MAX_OUTCOMES = 32;

/** Cap on tools in a single resolved roster. */
export const MAX_ROSTER_SIZE = 64;

/** Cap on an outcome message, in characters. */
export const MAX_MESSAGE_LENGTH = 1000;

/** Cap on a tool description, in characters. */
export const MAX_DESCRIPTION_LENGTH = 500;

/** Cap on a tool's display title, in characters. */
export const MAX_TITLE_LENGTH = 80;

/** Cap on an LLM consult prompt, in characters (measured before templating). */
export const MAX_LLM_PROMPT_LENGTH = 4000;

/**
 * Default cap on the answer an LLM consult may contribute, in characters. The
 * model's reply is trimmed and truncated before it is tested or rendered.
 * A definition that wants a longer (or shorter) leash sets `llm.maxOutput`;
 * this is only what applies when it doesn't say.
 */
export const MAX_LLM_OUTPUT_LENGTH = 8000;

/**
 * Hard ceiling on `llm.maxOutput`. Generous on purpose — a consult may well BE
 * the deliverable (a generated document, a deep outline) — but still bounded:
 * the answer lands in `pascalMeta` on a chat_messages row, and rows must not
 * become arbitrarily large because one oracle would not stop talking.
 */
export const MAX_LLM_OUTPUT_CEILING = 100_000;

/**
 * Identifier rules shared by tool names and parameter names: lowercase, starts
 * with a letter, 1–64 characters.
 */
export const IDENTIFIER_PATTERN = /^[a-z][a-z0-9_-]{0,63}$/;

const IDENTIFIER_MESSAGE = 'must be lowercase, start with a letter, and be 1–64 characters';

const AT_LEAST_ONE = 'must specify at least one comparator (gt, gte, lt, lte, eq, neq)';

const AT_LEAST_ONE_WIDE =
  'must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains)';

/** v4's `StringOperandSchema` min-length message. */
const EMPTY_SUBSTRING = 'the substring to look for must not be empty';

/**
 * v4's `z.number().finite()` expectation string. See the module note: this is
 * the one arm the oracle corpus does not pin.
 */
const FINITE_EXPECTED = 'number';

/** The comparator keys, in the order tests are described to a reader. */
export const COMPARATOR_KEYS = [
  'gt',
  'gte',
  'lt',
  'lte',
  'eq',
  'neq',
  'contains',
  'ncontains',
] as const;

/**
 * The six keys v4's `NUMERIC_COMPARATOR_SHAPE` declares — the strict-object key
 * set for the numeric comparator AND for a `when`'s bare comparator keys.
 * Deliberately narrower than {@link COMPARATOR_KEYS}: containment never appears
 * on a subject that is always a number, so `{ "contains": "x" }` on the bare
 * value (or inside `roll`) is an unrecognized key, not a type error.
 */
const NUMERIC_COMPARATOR_KEYS = ['gt', 'gte', 'lt', 'lte', 'eq', 'neq'] as const;

/** The four keys that order two values, and so demand numbers on both sides. */
const ORDERING_KEYS: readonly string[] = ['gt', 'gte', 'lt', 'lte'];

/** The two keys that search one string inside another, and so demand strings. */
const CONTAINMENT_KEYS: readonly string[] = ['contains', 'ncontains'];

// ---------------------------------------------------------------- the types

/** The four parameter types a definition may declare. */
export type ParameterType = 'number' | 'integer' | 'string' | 'boolean';

/** Default visibility for a tool's result. */
export type Visibility = 'public' | 'whisper';

/** The semantic states an outcome may carry. Maps to qt classes at render. */
export type OutcomeState = 'success' | 'partial' | 'failure' | 'info';

/**
 * A reference to a declared numeric parameter, usable anywhere a roll takes a
 * number. One of the two forms of indirection in the format (the other is
 * {@link StateRef}).
 */
export interface ParamRef {
  $param: string;
}

/**
 * A reference into the merged persistent state (chat → project → group →
 * general), usable anywhere a `$param` reference is. Its `fallback` is required
 * and load-bearing: it types the reference at load time (a numeric fallback
 * makes it a number, a string fallback a string, and so on) and guarantees that
 * run-time resolution can never fail — an absent path or a value of the wrong
 * type simply resolves to the fallback. A run is therefore always dealable.
 */
export interface StateRef {
  $state: string;
  fallback: number | string | boolean;
}

/** A roll field: a literal number, a `$param`, or a `$state` reference. */
export type NumberOrParamRef = number | ParamRef | StateRef;

/**
 * A comparator operand for eq/neq, which may address a parameter of any
 * declared type — `{ "eq": "brass" }` is a legitimate test of a string — or a
 * `$state` reference typed by its fallback.
 */
export type AnyOperand = number | string | boolean | ParamRef | StateRef;

/**
 * A comparator operand for contains/ncontains: the substring to look for — a
 * literal string, a `$param` reference to a declared string parameter, or a
 * `$state` reference whose fallback is a string. The literal must be non-empty
 * because every string contains "", which makes an empty needle a typo wearing
 * a comparator's clothes.
 */
export type StringOperand = string | ParamRef | StateRef;

/** True when a roll field is a `$param` reference rather than a literal. */
export function isParamRef(value: unknown): value is ParamRef {
  return typeof value === 'object' && value !== null && '$param' in value;
}

/** True when a value is a `$state` reference rather than a literal or `$param`. */
export function isStateRef(value: unknown): value is StateRef {
  return typeof value === 'object' && value !== null && '$state' in value;
}

/**
 * A declared parameter. Every parameter requires a `default` so that a
 * zero-argument run is always possible — the model may reach for a tool without
 * supplying anything, and the table must still deal.
 */
export interface CustomToolParameter {
  type: ParameterType;
  default: number | string | boolean | StateRef;
  description?: string;
  min?: number;
  max?: number;
}

/**
 * Form A — a uniform roll over a numeric range, with an optional transform.
 * The transform runs in a fixed order: multiply, then offset, then round.
 */
export interface RollRange {
  min?: NumberOrParamRef;
  max?: NumberOrParamRef;
  multiplier?: NumberOrParamRef;
  offset?: NumberOrParamRef;
  round?: boolean;
}

/** Either form of roll: dice notation, or a range object. */
export type Roll = string | RollRange;

/**
 * A comparator against a number — the rolled value, or the raw draw. Keys AND
 * together: `>= 0.3 && <= 0.6` is `{ gte: 0.3, lte: 0.6 }`.
 */
export interface NumericComparator {
  gt?: NumberOrParamRef;
  gte?: NumberOrParamRef;
  lt?: NumberOrParamRef;
  lte?: NumberOrParamRef;
  eq?: NumberOrParamRef;
  neq?: NumberOrParamRef;
}

/**
 * A comparator against a declared parameter. Identical to the numeric form
 * except that eq/neq widen to strings and booleans, since a parameter need not
 * be a number — and contains/ncontains appear, testing whether a string
 * parameter holds (or lacks) a substring. The substring is a literal or a
 * `$param` reference, so a table can ask whether one input appears inside
 * another.
 */
export interface ParamComparator {
  gt?: NumberOrParamRef;
  gte?: NumberOrParamRef;
  lt?: NumberOrParamRef;
  lte?: NumberOrParamRef;
  eq?: AnyOperand;
  neq?: AnyOperand;
  contains?: StringOperand;
  ncontains?: StringOperand;
}

/**
 * A comparator against one key of the invoking character's metadata sheet.
 * Shape-identical to {@link ParamComparator} — the same keys, the same widened
 * eq/neq, the same contains/ncontains, the same `$param` operands.
 *
 * It is a separate name because the two differ entirely in what can be known at
 * load time. A `params` test names something the file itself declares, so a
 * misspelling is a rejection. A `metadata` test names a key on a character the
 * file has never met: nothing here can be checked beyond the comparator's own
 * shape, and the run-time rule (a key that is absent, non-primitive, or of the
 * wrong type simply fails to match) closes the gap.
 */
export type MetadataComparator = ParamComparator;

/**
 * A comparator in an availability gate. The same eight keys as everywhere else,
 * but every operand is a LITERAL.
 *
 * A gate is evaluated before a run exists: there are no resolved parameters to
 * point a `$param` at, and no run whose state cascade a `$state` reference could
 * be resolved against. Rejecting those forms at load time is the honest move —
 * silently tolerating one would leave an author with a gate that reads as though
 * it consults the caller's input when nothing of the sort can have happened yet.
 */
export interface GateComparator {
  gt?: number;
  gte?: number;
  lt?: number;
  lte?: number;
  eq?: number | string | boolean;
  neq?: number | string | boolean;
  contains?: string;
  ncontains?: string;
}

/**
 * An availability gate: whether this invoker is offered the tool at all.
 *
 * Keyed by metadata key, AND-composed, and fail-soft in exactly the way an
 * outcome's `metadata` test is — a key the character lacks does not match. The
 * subject is `metadata` and only `metadata` because a gate is answered BEFORE
 * the deal: there is no roll to test, no parameters (nobody has called
 * anything), and no consult. `metadata` is what a character carries into the
 * room, so it is the one thing that can be asked about before they sit down.
 *
 * The subject lives under its own key rather than at the top of the object so a
 * later build can add a second one without re-shaping every file already
 * written.
 */
export interface ToolGate {
  metadata: Record<string, GateComparator>;
}

/**
 * A comparator against the LLM consult's result. The comparator keys test the
 * answer string; `ok` is an extra, non-comparator key testing whether the
 * consult succeeded at all.
 *
 * Like `metadata`, the subject's run-time type is unknowable at load time — the
 * answer is whatever the model says — so type rules are enforced fail-soft at
 * run time: an ordering comparator against an answer that does not parse as a
 * number simply declines the row. eq/neq compare numerically when both sides
 * are numbers, and otherwise case-insensitively as trimmed strings, because an
 * author who asked for "YES" should not lose to a model that said "yes".
 * contains/ncontains search the answer for a substring under the same
 * case-insensitive reconciliation — the natural test of prose, where equality
 * would demand the whole sentence verbatim.
 */
export interface LlmComparator extends ParamComparator {
  ok?: boolean;
}

/**
 * The LLM consult block. When present, every run renders `prompt` (the same
 * placeholder families an outcome message takes, minus `{{llm}}` itself) and
 * poses it to the instance's cheap utility model AFTER the roll and BEFORE the
 * outcome table is evaluated. The result is a pair `{ ok, output }`:
 *
 * - success → `ok: true`, `output` is the model's trimmed answer;
 * - failure (provider error, timeout, empty answer, no model configured) →
 *   `ok: false`, `output` is this block's `errorMessage` — the AUTHOR's words,
 *   never the provider's stack trace, because whatever lands in the fiction is
 *   the author's to write.
 *
 * Either way the pair is testable in `when` (the `llm` subject) and renderable
 * in messages (`{{llm}}`), so a failed consult is an outcome the table can deal
 * with rather than an error bubble: the run itself never fails because the
 * oracle went quiet.
 */
export interface CustomToolLlm {
  prompt: string;
  errorMessage: string;
  maxOutput?: number;
}

/**
 * An outcome test's object form: one or more subjects, ALL of which must hold.
 *
 * Bare comparator keys test the final value, so the common case stays as short
 * as it ever was and every definition written before this key existed still
 * means what it meant. `roll` tests the raw pre-transform draw; `params` tests
 * what the caller supplied, keyed by parameter name; `metadata` tests the
 * invoking character's own fact sheet (`metadata.json`), keyed by whatever the
 * user called it.
 *
 * A `metadata` key that this character simply doesn't have is not an error —
 * the comparator is false and the table falls through to its catch-all. That is
 * the whole point: a lockpicking table branches on the key its author invented,
 * and must still deal sensibly to the character who's never heard of it.
 *
 * There is still no OR and no nesting: ordered, first-match-wins outcomes make
 * OR unnecessary, and a flat AND of comparators keeps the evaluator eval-free.
 */
export interface WhenObject {
  gt?: NumberOrParamRef;
  gte?: NumberOrParamRef;
  lt?: NumberOrParamRef;
  lte?: NumberOrParamRef;
  eq?: NumberOrParamRef;
  neq?: NumberOrParamRef;
  roll?: NumericComparator;
  params?: Record<string, ParamComparator>;
  metadata?: Record<string, MetadataComparator>;
  llm?: LlmComparator;
}

/** An outcome test: either the literal `true` (catch-all) or a {@link WhenObject}. */
export type When = true | WhenObject;

export interface CustomToolOutcome {
  when: When;
  message: string;
  state: OutcomeState;
}

/**
 * The custom-tool definition.
 *
 * Unknown TOP-LEVEL keys are tolerated, which reserves room for v2 keys —
 * notably `persist` — without breaking older builds. {@link collectUnknownKeys}
 * surfaces them for a debug log.
 *
 * That tolerance stops at the top level: every nested object below is strict.
 * The forward-compatibility argument does not reach them, and inside a `when`
 * an unrecognised key is overwhelmingly a misspelled comparator — which, if
 * tolerated, silently drops the test and leaves a row of the outcome table
 * looking like a dead branch. It is also what the published JSON Schema has
 * always claimed (`additionalProperties: false`), so an author's editor and the
 * loader now agree.
 */
export interface QtapCustomTool {
  $schema?: string;
  name: string;
  title?: string;
  description: string;
  disabled?: boolean;
  /** Offer this tool ONLY to an invoker whose metadata satisfies every test. */
  availableWhen?: ToolGate;
  /** Withhold this tool from an invoker whose metadata satisfies every test. */
  withheldWhen?: ToolGate;
  revealOdds?: boolean;
  defaultVisibility?: Visibility;
  parameters?: Record<string, CustomToolParameter>;
  roll?: Roll;
  llm?: CustomToolLlm;
  outcomes: CustomToolOutcome[];
}

// ---------------------------------------------------------------- issue model

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
function hardIssue(message: string): Issue {
  return { path: [], message, cont: false };
}

/** A continuable issue — Zod's `ctx.addIssue` / `refine` shape. */
function checkIssue(message: string, path: string[] = []): Issue {
  return { path, message, cont: true };
}

/** Prefix one issue's path with `seg`. */
function at(issue: Issue, seg: string): Issue {
  return { ...issue, path: [seg, ...issue.path] };
}

/** v4 `util.aborted`: a payload is aborted once any issue is not continuable. */
function aborted(issues: readonly Issue[]): boolean {
  return issues.some((i) => !i.cont);
}

/**
 * Prefix every issue's path with `seg`. Deliberately does NOT touch
 * `unionErrors` — Zod keeps branch paths relative to the union.
 */
function prefix(seg: string, issues: Issue[]): Issue[] {
  return issues.map((i) => at(i, seg));
}

/** A parse result: the value when one could be built, plus any issues. */
interface Res<T> {
  value: T | undefined;
  issues: Issue[];
}

function resOk<T>(value: T): Res<T> {
  return { value, issues: [] };
}

/** Shape failure — no value, and the issue aborts. */
function resHard<T>(message: string): Res<T> {
  return { value: undefined, issues: [hardIssue(message)] };
}

/**
 * v4 `handleUnionResults`. The middle arm is the subtle one: when exactly one
 * branch is non-aborted, Zod returns THAT branch rather than wrapping, which is
 * how a failed `refine` inside one branch surfaces its own sentence instead of a
 * bare "Invalid input" at the union.
 */
function union<T>(branches: Res<T>[]): Res<T> {
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
function parsedType(v: unknown): string {
  if (v === undefined) return 'undefined';
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  const t = typeof v;
  if (t === 'boolean' || t === 'number' || t === 'string' || t === 'object') return t;
  return t;
}

function invalidType(expected: string, got: unknown): string {
  return `Invalid input: expected ${expected}, received ${parsedType(got)}`;
}

/** A JSON object — `typeof 'object'` minus null and arrays. */
function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** Mirrors Rust's `map.contains_key` / Zod's own key-presence test. */
function hasKey(obj: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(obj, key);
}

// ------------------------------------------------------------ leaf validators

/**
 * v4 `z.string()` plus the optional length caps and the identifier regex.
 * `min`/`max` are Zod built-ins (so their sentences are Zod's); the regex
 * carries the author's message. The regex is a CHECK, so its issue is
 * continuable — which is what lets a malformed `$param` still reach the
 * cross-field rules and pick up a second sentence.
 */
function parseString(
  input: unknown,
  min: number | undefined,
  max: number | undefined,
  identifier: boolean,
): Res<string> {
  if (typeof input !== 'string') return resHard(invalidType('string', input));

  const issues: Issue[] = [];
  // JS `.length` is UTF-16 code units — which is exactly what Zod counts.
  const len = input.length;
  if (min !== undefined && len < min) {
    issues.push(checkIssue(`Too small: expected string to have >=${min} characters`));
  }
  if (max !== undefined && len > max) {
    issues.push(checkIssue(`Too big: expected string to have <=${max} characters`));
  }
  if (identifier && !IDENTIFIER_PATTERN.test(input)) {
    issues.push(checkIssue(IDENTIFIER_MESSAGE));
  }

  return { value: input, issues };
}

function parseBool(input: unknown): Res<boolean> {
  if (typeof input === 'boolean') return resOk(input);
  return resHard(invalidType('boolean', input));
}

/**
 * v4 `z.number().finite()`. Unlike the Rust port, this arm IS reachable here:
 * `JSON.parse('{"gt":1e999}')` yields `Infinity` in a browser exactly as it does
 * in v4's Node. See the module note — it is the one message the corpus cannot
 * pin.
 */
function parseFiniteNumber(input: unknown): Res<number> {
  if (typeof input !== 'number') return resHard(invalidType('number', input));
  if (!Number.isFinite(input)) return resHard(invalidType(FINITE_EXPECTED, input));
  return resOk(input);
}

function parseEnum<T extends string>(input: unknown, options: readonly T[]): Res<T> {
  if (typeof input === 'string' && (options as readonly string[]).includes(input)) {
    return resOk(input as T);
  }
  const list = options.map((o) => `"${o}"`).join('|');
  return resHard(`Invalid option: expected one of ${list}`);
}

/**
 * The strict-object tail: report keys outside `known` as ONE issue, in the
 * input's own order. Zod raises this AFTER the shape, and it is aborting.
 */
function unrecognizedKeys(obj: Record<string, unknown>, known: readonly string[]): Issue[] {
  const extra = Object.keys(obj).filter((k) => !known.includes(k));
  if (extra.length === 0) return [];
  const quoted = extra.map((k) => `"${k}"`).join(', ');
  return [hardIssue(extra.length === 1 ? `Unrecognized key: ${quoted}` : `Unrecognized keys: ${quoted}`)];
}

// --------------------------------------------------------- schema validators

function parseParamRef(input: unknown): Res<ParamRef> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const r = parseString(input['$param'], undefined, undefined, true);
  issues.push(...prefix('$param', r.issues));
  issues.push(...unrecognizedKeys(input, ['$param']));

  const value = r.value !== undefined && !aborted(issues) ? { $param: r.value } : undefined;
  return { value, issues };
}

/**
 * v4 `StateRefSchema`'s `fallback` — `z.union([z.number().finite(), z.string(),
 * z.boolean()])`, in that order. Note the `.finite()` (unlike a parameter
 * `default`), so an overflow literal fails the number branch.
 */
function parseStateFallback(input: unknown): Res<number | string | boolean> {
  const n = parseFiniteNumber(input);
  const s = parseString(input, undefined, undefined, false);
  const b = parseBool(input);
  return union<number | string | boolean>([n, s, b]);
}

/**
 * v4 `StateRefSchema` — `z.strictObject({ $state: z.string().min(1), fallback:
 * <the union above> })`. The `$state` `min(1)` is a CHECK (continuable), which
 * is what lets an empty path stay the single non-aborted branch in a union and
 * hoist its own sentence rather than being buried under "Invalid input".
 */
function parseStateRef(input: unknown): Res<StateRef> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const path = parseString(input['$state'], 1, undefined, false);
  issues.push(...prefix('$state', path.issues));

  const fallback = parseStateFallback(input['fallback']);
  issues.push(...prefix('fallback', fallback.issues));

  issues.push(...unrecognizedKeys(input, ['$state', 'fallback']));

  const value =
    path.value !== undefined && fallback.value !== undefined && !aborted(issues)
      ? { $state: path.value, fallback: fallback.value }
      : undefined;
  return { value, issues };
}

/**
 * v4 `NumberOrParamRefSchema` / `NumberOperandSchema` — the two are the same
 * union, and both roll fields and ordering operands take it.
 */
function parseNumberOrParamRef(input: unknown): Res<NumberOrParamRef> {
  const n = parseFiniteNumber(input);
  const p = parseParamRef(input);
  const st = parseStateRef(input);
  return union<NumberOrParamRef>([n, p, st]);
}

/**
 * v4 `AnyOperandSchema` — number | string | boolean | ParamRef | StateRef, in
 * that order.
 */
function parseAnyOperand(input: unknown): Res<AnyOperand> {
  const n = parseFiniteNumber(input);
  const s = parseString(input, undefined, undefined, false);
  const b = parseBool(input);
  const p = parseParamRef(input);
  const st = parseStateRef(input);
  return union<AnyOperand>([n, s, b, p, st]);
}

/**
 * v4 `StringOperandSchema` — `z.string().min(1, …) | ParamRefSchema |
 * StateRefSchema`. The empty-string case leans on the union's hoist rule: the
 * string branch's `min` is a CHECK (continuable) while the `$param`/`$state`
 * branches abort on shape, so exactly one branch is non-aborted and its
 * authored sentence surfaces instead of a bare "Invalid input".
 */
function parseStringOperand(input: unknown): Res<StringOperand> {
  const s: Res<StringOperand> =
    typeof input === 'string'
      ? { value: input, issues: input.length < 1 ? [checkIssue(EMPTY_SUBSTRING)] : [] }
      : resHard(invalidType('string', input));
  const p = parseParamRef(input);
  const st = parseStateRef(input);
  return union<StringOperand>([s, p, st]);
}

function parseNumericComparator(input: unknown): Res<NumericComparator> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: NumericComparator = {};
  for (const key of NUMERIC_COMPARATOR_KEYS) {
    if (!hasKey(input, key)) continue;
    const r = parseNumberOrParamRef(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  // Six keys, not eight: containment never reaches a subject that is always a
  // number, so `{ "contains": … }` here is an unrecognized key.
  issues.push(...unrecognizedKeys(input, NUMERIC_COMPARATOR_KEYS));

  if (aborted(issues)) return { value: undefined, issues };
  // `.refine(hasComparator, AT_LEAST_ONE)` — runs only when nothing aborted.
  if (!COMPARATOR_KEYS.some((k) => hasKey(input, k))) issues.push(checkIssue(AT_LEAST_ONE));
  return { value: out, issues };
}

/**
 * The comparator shape shared by `params`, `metadata`, and `llm`: the ordering
 * keys numeric, eq/neq widened, contains/ncontains taking a string operand.
 * `llm` adds one non-comparator key (`ok`) and carries the wider at-least-one
 * message all three share.
 */
function parseWideComparatorKeys(
  input: Record<string, unknown>,
  out: Record<string, unknown>,
  issues: Issue[],
): void {
  // Ordering keys stay numeric on both sides; eq/neq widen to any operand.
  for (const key of ['gt', 'gte', 'lt', 'lte'] as const) {
    if (!hasKey(input, key)) continue;
    const r = parseNumberOrParamRef(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  for (const key of ['eq', 'neq'] as const) {
    if (!hasKey(input, key)) continue;
    const r = parseAnyOperand(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  for (const key of ['contains', 'ncontains'] as const) {
    if (!hasKey(input, key)) continue;
    const r = parseStringOperand(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
}

function parseParamComparator(input: unknown): Res<ParamComparator> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: ParamComparator = {};
  parseWideComparatorKeys(input, out as Record<string, unknown>, issues);
  issues.push(...unrecognizedKeys(input, COMPARATOR_KEYS));

  if (aborted(issues)) return { value: undefined, issues };
  if (!COMPARATOR_KEYS.some((k) => hasKey(input, k))) issues.push(checkIssue(AT_LEAST_ONE_WIDE));
  return { value: out, issues };
}

/**
 * v4 `LlmComparatorSchema` — the wide comparator plus `ok`, and a refine that
 * accepts either a comparator on the answer OR a bare `ok` test, since "only
 * when the consult failed" is a complete thought.
 */
function parseLlmComparator(input: unknown): Res<LlmComparator> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: LlmComparator = {};
  parseWideComparatorKeys(input, out as Record<string, unknown>, issues);
  if (hasKey(input, 'ok')) {
    const r = parseBool(input['ok']);
    issues.push(...prefix('ok', r.issues));
    if (r.value !== undefined) out.ok = r.value;
  }
  issues.push(...unrecognizedKeys(input, [...COMPARATOR_KEYS, 'ok']));

  if (aborted(issues)) return { value: undefined, issues };
  if (!COMPARATOR_KEYS.some((k) => hasKey(input, k)) && out.ok === undefined) {
    issues.push(checkIssue('must test something: a comparator on the answer, or `ok`'));
  }
  return { value: out, issues };
}

/**
 * v4's gate eq/neq operand — `z.union([z.number().finite(), z.string(),
 * z.boolean()])`. Structurally the same three literal arms, in the same order,
 * that a `$state` fallback takes; named apart because the REASON differs (a
 * gate admits no reference form at all, rather than being the fallback FOR one).
 */
function parseGateLiteral(input: unknown): Res<number | string | boolean> {
  return parseStateFallback(input);
}

/**
 * v4 `GateComparatorSchema` — the eight comparator keys with LITERAL operands
 * only: ordering takes a bare finite number (no `$param`, no `$state`), eq/neq
 * take one of the three literal types, and containment takes a non-empty string
 * carrying the author's own `min` sentence.
 */
function parseGateComparator(input: unknown): Res<GateComparator> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: GateComparator = {};

  for (const key of ['gt', 'gte', 'lt', 'lte'] as const) {
    if (!hasKey(input, key)) continue;
    const r = parseFiniteNumber(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  for (const key of ['eq', 'neq'] as const) {
    if (!hasKey(input, key)) continue;
    const r = parseGateLiteral(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  for (const key of ['contains', 'ncontains'] as const) {
    if (!hasKey(input, key)) continue;
    // `z.string().min(1, EMPTY_SUBSTRING)`: the min is a CHECK carrying the
    // authored sentence, not Zod's built-in "Too small" one.
    const raw = input[key];
    if (typeof raw !== 'string') {
      issues.push(...prefix(key, [hardIssue(invalidType('string', raw))]));
    } else {
      if (raw.length < 1) issues.push(...prefix(key, [checkIssue(EMPTY_SUBSTRING)]));
      out[key] = raw;
    }
  }

  issues.push(...unrecognizedKeys(input, COMPARATOR_KEYS));

  if (aborted(issues)) return { value: undefined, issues };
  if (!COMPARATOR_KEYS.some((k) => hasKey(input, k))) issues.push(checkIssue(AT_LEAST_ONE_WIDE));
  return { value: out, issues };
}

/**
 * v4 `ToolGateSchema` — `z.strictObject({ metadata: z.record(…).refine(…) })`.
 * The "must test at least one metadata key" refine sits on the RECORD, so it is
 * skipped once an entry has aborted, and its issue path is `metadata`.
 */
function parseToolGate(input: unknown): Res<ToolGate> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  let metadata: Record<string, GateComparator> | undefined;

  const raw = input['metadata'];
  if (isPlainObject(raw)) {
    // Gate keys take `z.string().min(1)`, the same non-empty grammar a `when`'s
    // `metadata` keys take — the user hand-authors `metadata.json`.
    const r = parseRecord(raw, (k) => k.length > 0, parseGateComparator);
    const metaIssues = [...r.issues];
    if (!aborted(metaIssues) && Object.keys(r.value ?? {}).length === 0) {
      metaIssues.push(checkIssue('must test at least one metadata key'));
    }
    issues.push(...prefix('metadata', metaIssues));
    metadata = r.value;
  } else {
    // "record", not "object": Zod names a `z.record` by its own type, and the
    // captured rows pin it. (The three PRE-EXISTING `z.record` sites in this
    // file — `when.params`, `when.metadata`, top-level `parameters` — say
    // "object" instead; the Rust port says "object" at the same three, so the
    // two v5 halves agree with each other while both diverge from v4. No corpus
    // row covers them. Recorded, deliberately not fixed here: a one-sided fix
    // would put the browser at odds with the server. See the lane record.)
    issues.push(...prefix('metadata', [hardIssue(invalidType('record', raw))]));
  }

  issues.push(...unrecognizedKeys(input, ['metadata']));

  const value = metadata !== undefined && !aborted(issues) ? { metadata } : undefined;
  return { value, issues };
}

/**
 * v4 `z.record(keySchema, …)`: a key failing `keyValid` is an `invalid_key`
 * issue — aborting, unlike the same regex used as a field check. `params` keys
 * take the identifier grammar; `metadata` keys take `z.string().min(1)`
 * (non-empty), since the user hand-authors `metadata.json` and `HOUSE` or
 * `Clearance Level` are perfectly ordinary keys. Zod's message is the same
 * "Invalid key in record" either way.
 */
function parseRecord<T>(
  obj: Record<string, unknown>,
  keyValid: (key: string) => boolean,
  parseValue: (v: unknown) => Res<T>,
): Res<Record<string, T>> {
  const issues: Issue[] = [];
  const out: Record<string, T> = {};
  for (const key of Object.keys(obj)) {
    if (!keyValid(key)) {
      issues.push(at(hardIssue('Invalid key in record'), key));
      continue;
    }
    const r = parseValue(obj[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  return { value: aborted(issues) ? undefined : out, issues };
}

function parseParameter(input: unknown): Res<CustomToolParameter> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];

  const ty = parseEnum<ParameterType>(input['type'], ['number', 'integer', 'string', 'boolean']);
  issues.push(...prefix('type', ty.issues));

  // `default: z.union([number, string, boolean, StateRefSchema])` — no
  // `.finite()` on the number arm here (unlike a `$state` fallback).
  let defaultValue: number | string | boolean | StateRef | undefined;
  const rawDefault = input['default'];
  if (typeof rawDefault === 'number' || typeof rawDefault === 'string' || typeof rawDefault === 'boolean') {
    defaultValue = rawDefault;
  } else {
    const branches: Res<number | string | boolean | StateRef>[] = [
      resHard(invalidType('number', rawDefault)),
      resHard(invalidType('string', rawDefault)),
      resHard(invalidType('boolean', rawDefault)),
      parseStateRef(rawDefault),
    ];
    const r = union<number | string | boolean | StateRef>(branches);
    issues.push(...prefix('default', r.issues));
    defaultValue = r.value;
  }

  let description: string | undefined;
  if (hasKey(input, 'description')) {
    const r = parseString(input['description'], undefined, MAX_DESCRIPTION_LENGTH, false);
    issues.push(...prefix('description', r.issues));
    description = r.value;
  }

  const numField = (key: 'min' | 'max'): number | undefined => {
    if (!hasKey(input, key)) return undefined;
    const r = parseFiniteNumber(input[key]);
    issues.push(...prefix(key, r.issues));
    return r.value;
  };
  const min = numField('min');
  const max = numField('max');

  issues.push(...unrecognizedKeys(input, ['type', 'default', 'description', 'min', 'max']));

  if (ty.value === undefined || defaultValue === undefined) return { value: undefined, issues };
  if (aborted(issues)) return { value: undefined, issues };

  // ---- superRefine (v4 `:99-129`), in source order ------------------------
  const paramType = ty.value;
  const numeric = paramType === 'number' || paramType === 'integer';

  // `min`/`max` are meaningless on a string or boolean; silently ignoring them
  // would let an author believe a bound is in force when it is not.
  if (!numeric && (min !== undefined || max !== undefined)) {
    issues.push(
      checkIssue(`min/max are only valid on number/integer parameters, not ${paramType}`),
    );
  }
  if (min !== undefined && max !== undefined && min > max) {
    issues.push(checkIssue('min must not exceed max', ['min']));
  }

  // The declared default must satisfy the parameter's own declared type. A
  // `$state` default is typed by its fallback (which run-time resolution is
  // guaranteed to fall back to), so the fallback is what must match the type.
  const effectiveDefault = isStateRef(defaultValue) ? defaultValue.fallback : defaultValue;
  const defaultType = typeof effectiveDefault;
  if (numeric && defaultType !== 'number') {
    issues.push(checkIssue(`default must be a number for type ${paramType}`, ['default']));
  }
  if (paramType === 'integer' && defaultType === 'number' && !Number.isInteger(effectiveDefault)) {
    issues.push(checkIssue('default must be a whole number for type integer', ['default']));
  }
  if (paramType === 'string' && defaultType !== 'string') {
    issues.push(checkIssue('default must be a string for type string', ['default']));
  }
  if (paramType === 'boolean' && defaultType !== 'boolean') {
    issues.push(checkIssue('default must be a boolean for type boolean', ['default']));
  }

  // Built in declaration order, absent optionals omitted — the parsed data is
  // byte-compared against Zod's own output by the corpus differential.
  const value: CustomToolParameter = { type: paramType, default: defaultValue };
  if (description !== undefined) value.description = description;
  if (min !== undefined) value.min = min;
  if (max !== undefined) value.max = max;
  return { value, issues };
}

/**
 * Form B — dice notation, validated here so a typo is a load-time rejection
 * rather than a run-time surprise. The `refine` is a check, so a bad notation
 * hoists out of the `roll` union with its own sentence.
 */
function parseRollDice(input: unknown): Res<string> {
  const r = parseString(input, undefined, undefined, false);
  if (r.value === undefined) return { value: undefined, issues: r.issues };
  const issues = [...r.issues];
  if (parseDiceNotation(r.value) === null) {
    issues.push(
      checkIssue(
        `must be dice notation like "3d6+2" or "1d20" (${MIN_DIE_SIDES}–${MAX_DIE_SIDES} sides, 1–100 dice)`,
      ),
    );
  }
  return { value: r.value, issues };
}

function parseRollRange(input: unknown): Res<RollRange> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: RollRange = {};
  for (const key of ['min', 'max', 'multiplier', 'offset'] as const) {
    if (!hasKey(input, key)) continue;
    const r = parseNumberOrParamRef(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  if (hasKey(input, 'round')) {
    const r = parseBool(input['round']);
    issues.push(...prefix('round', r.issues));
    if (r.value !== undefined) out.round = r.value;
  }
  issues.push(...unrecognizedKeys(input, ['min', 'max', 'multiplier', 'offset', 'round']));

  return { value: aborted(issues) ? undefined : out, issues };
}

function parseRoll(input: unknown): Res<Roll> {
  const d = parseRollDice(input);
  const r = parseRollRange(input);
  return union<Roll>([d, r]);
}

/**
 * v4 `CustomToolLlmSchema` — the consult block. `maxOutput` is
 * `z.number().int().min(1).max(MAX_LLM_OUTPUT_CEILING)`, and the non-integer
 * complaint SUPPRESSES the bound checks: empirically, `maxOutput: 100001.5`
 * reports only `expected int, received number`, never the ceiling as well.
 * (Whether Zod marks that issue aborting is not observable from here — either
 * way the parse fails with the same sentence — so only the suppression is
 * modelled.)
 */
function parseCustomToolLlm(input: unknown): Res<CustomToolLlm> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];

  const prompt = parseString(input['prompt'], 1, MAX_LLM_PROMPT_LENGTH, false);
  issues.push(...prefix('prompt', prompt.issues));

  const errorMessage = parseString(input['errorMessage'], 1, MAX_MESSAGE_LENGTH, false);
  issues.push(...prefix('errorMessage', errorMessage.issues));

  let maxOutput: number | undefined;
  if (hasKey(input, 'maxOutput')) {
    const raw = input['maxOutput'];
    const own: Issue[] = [];
    if (typeof raw !== 'number') {
      own.push(hardIssue(invalidType('number', raw)));
    } else if (!Number.isInteger(raw)) {
      own.push(hardIssue(invalidType('int', raw)));
    } else {
      if (raw < 1) own.push(checkIssue('Too small: expected number to be >=1'));
      if (raw > MAX_LLM_OUTPUT_CEILING) {
        own.push(checkIssue(`Too big: expected number to be <=${MAX_LLM_OUTPUT_CEILING}`));
      }
      maxOutput = raw;
    }
    issues.push(...prefix('maxOutput', own));
  }

  issues.push(...unrecognizedKeys(input, ['prompt', 'errorMessage', 'maxOutput']));

  if (prompt.value === undefined || errorMessage.value === undefined) {
    return { value: undefined, issues };
  }
  if (aborted(issues)) return { value: undefined, issues };

  const value: CustomToolLlm = { prompt: prompt.value, errorMessage: errorMessage.value };
  if (maxOutput !== undefined) value.maxOutput = maxOutput;
  return { value, issues };
}

function parseWhenObject(input: unknown): Res<WhenObject> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: WhenObject = {};
  // Bare keys address the final value — always a number, so the six.
  for (const key of NUMERIC_COMPARATOR_KEYS) {
    if (!hasKey(input, key)) continue;
    const r = parseNumberOrParamRef(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  if (hasKey(input, 'roll')) {
    const r = parseNumericComparator(input['roll']);
    issues.push(...prefix('roll', r.issues));
    if (r.value !== undefined) out.roll = r.value;
  }
  let paramsPresent = false;
  if (hasKey(input, 'params')) {
    paramsPresent = true;
    const raw = input['params'];
    if (isPlainObject(raw)) {
      const r = parseRecord(raw, (k) => IDENTIFIER_PATTERN.test(k), parseParamComparator);
      issues.push(...prefix('params', r.issues));
      if (r.value !== undefined) out.params = r.value;
    } else {
      issues.push(...prefix('params', [hardIssue(invalidType('object', raw))]));
    }
  }
  let metadataPresent = false;
  if (hasKey(input, 'metadata')) {
    metadataPresent = true;
    const raw = input['metadata'];
    if (isPlainObject(raw)) {
      // Metadata keys are `z.string().min(1)` — any non-empty string, unlike
      // `params`' identifier grammar.
      const r = parseRecord(raw, (k) => k.length > 0, parseParamComparator);
      issues.push(...prefix('metadata', r.issues));
      if (r.value !== undefined) out.metadata = r.value;
    } else {
      issues.push(...prefix('metadata', [hardIssue(invalidType('object', raw))]));
    }
  }

  if (hasKey(input, 'llm')) {
    const r = parseLlmComparator(input['llm']);
    issues.push(...prefix('llm', r.issues));
    if (r.value !== undefined) out.llm = r.value;
  }

  issues.push(
    ...unrecognizedKeys(input, [...NUMERIC_COMPARATOR_KEYS, 'roll', 'params', 'metadata', 'llm']),
  );

  if (aborted(issues)) return { value: undefined, issues };

  // `.refine(…)` — must test something.
  const hasComparator = COMPARATOR_KEYS.some((k) => hasKey(input, k));
  const hasParams = paramsPresent && out.params !== undefined && Object.keys(out.params).length > 0;
  const hasMetadata =
    metadataPresent && out.metadata !== undefined && Object.keys(out.metadata).length > 0;
  if (!hasComparator && out.roll === undefined && out.llm === undefined && !hasParams && !hasMetadata) {
    issues.push(
      checkIssue(
        'must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, or a non-empty `metadata`',
      ),
    );
  }
  return { value: out, issues };
}

function parseWhen(input: unknown): Res<When> {
  // `z.literal(true)`.
  const lit: Res<When> =
    input === true
      ? resOk<When>(true)
      : { value: undefined, issues: [hardIssue('Invalid input: expected true')] };
  const o = parseWhenObject(input);
  return union<When>([lit, o]);
}

function parseOutcome(input: unknown): Res<CustomToolOutcome> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const when = parseWhen(input['when']);
  issues.push(...prefix('when', when.issues));

  const message = parseString(input['message'], 1, MAX_MESSAGE_LENGTH, false);
  issues.push(...prefix('message', message.issues));

  const state = parseEnum<OutcomeState>(input['state'], ['success', 'partial', 'failure', 'info']);
  issues.push(...prefix('state', state.issues));

  issues.push(...unrecognizedKeys(input, ['when', 'message', 'state']));

  if (
    when.value !== undefined &&
    message.value !== undefined &&
    state.value !== undefined &&
    !aborted(issues)
  ) {
    return { value: { when: when.value, message: message.value, state: state.value }, issues };
  }
  return { value: undefined, issues };
}

// -------------------------------------------------------------- the top level

/** A successful parse, or the rejection list in Zod's order. */
export type SafeParseResult =
  | { success: true; data: QtapCustomTool }
  | { success: false; issues: Issue[] };

/** v4 `QtapCustomToolSchema.safeParse`. */
export function safeParse(raw: unknown): SafeParseResult {
  if (!isPlainObject(raw)) {
    return { success: false, issues: [hardIssue(invalidType('object', raw))] };
  }
  const obj = raw;
  const issues: Issue[] = [];

  let schema: string | undefined;
  if (hasKey(obj, '$schema')) {
    const r = parseString(obj['$schema'], undefined, undefined, false);
    issues.push(...prefix('$schema', r.issues));
    schema = r.value;
  }

  const name = parseString(obj['name'], undefined, undefined, true);
  issues.push(...prefix('name', name.issues));

  let title: string | undefined;
  if (hasKey(obj, 'title')) {
    const r = parseString(obj['title'], 1, MAX_TITLE_LENGTH, false);
    issues.push(...prefix('title', r.issues));
    title = r.value;
  }

  const description = parseString(obj['description'], 1, MAX_DESCRIPTION_LENGTH, false);
  issues.push(...prefix('description', description.issues));

  const optBool = (key: 'disabled' | 'revealOdds'): boolean | undefined => {
    if (!hasKey(obj, key)) return undefined;
    const r = parseBool(obj[key]);
    issues.push(...prefix(key, r.issues));
    return r.value;
  };
  const disabled = optBool('disabled');

  // Both clauses sit between `disabled` and `revealOdds` in the schema's
  // declaration order, so they parse — and serialize — there.
  const optGate = (key: 'availableWhen' | 'withheldWhen'): ToolGate | undefined => {
    if (!hasKey(obj, key)) return undefined;
    const r = parseToolGate(obj[key]);
    issues.push(...prefix(key, r.issues));
    return r.value;
  };
  const availableWhen = optGate('availableWhen');
  const withheldWhen = optGate('withheldWhen');

  const revealOdds = optBool('revealOdds');

  let defaultVisibility: Visibility | undefined;
  if (hasKey(obj, 'defaultVisibility')) {
    const r = parseEnum<Visibility>(obj['defaultVisibility'], ['public', 'whisper']);
    issues.push(...prefix('defaultVisibility', r.issues));
    defaultVisibility = r.value;
  }

  let parameters: Record<string, CustomToolParameter> | undefined;
  if (hasKey(obj, 'parameters')) {
    const raw2 = obj['parameters'];
    if (isPlainObject(raw2)) {
      const r = parseRecord(raw2, (k) => IDENTIFIER_PATTERN.test(k), parseParameter);
      const paramIssues = [...r.issues];
      // `.refine(… <= MAX_PARAMETERS)` — a check on the record itself, so it is
      // skipped when an entry already aborted.
      if (!aborted(paramIssues) && Object.keys(raw2).length > MAX_PARAMETERS) {
        paramIssues.push(checkIssue(`at most ${MAX_PARAMETERS} parameters`));
      }
      issues.push(...prefix('parameters', paramIssues));
      parameters = r.value;
    } else {
      issues.push(...prefix('parameters', [hardIssue(invalidType('object', raw2))]));
    }
  }

  let roll: Roll | undefined;
  if (hasKey(obj, 'roll')) {
    const r = parseRoll(obj['roll']);
    issues.push(...prefix('roll', r.issues));
    roll = r.value;
  }

  let llm: CustomToolLlm | undefined;
  if (hasKey(obj, 'llm')) {
    const r = parseCustomToolLlm(obj['llm']);
    issues.push(...prefix('llm', r.issues));
    llm = r.value;
  }

  let outcomes: CustomToolOutcome[] | undefined;
  const rawOutcomes = obj['outcomes'];
  if (Array.isArray(rawOutcomes)) {
    const list: CustomToolOutcome[] = [];
    const arrIssues: Issue[] = [];
    rawOutcomes.forEach((item, i) => {
      const r = parseOutcome(item);
      arrIssues.push(...prefix(String(i), r.issues));
      if (r.value !== undefined) list.push(r.value);
    });
    // `.min(1, …)` / `.max(MAX_OUTCOMES, …)` are checks with authored messages,
    // so they are skipped once an element aborted.
    if (!aborted(arrIssues)) {
      if (rawOutcomes.length === 0) {
        arrIssues.push(checkIssue('at least one outcome is required'));
      } else if (rawOutcomes.length > MAX_OUTCOMES) {
        arrIssues.push(checkIssue(`at most ${MAX_OUTCOMES} outcomes`));
      }
    }
    const ok = !aborted(arrIssues);
    issues.push(...prefix('outcomes', arrIssues));
    if (ok) outcomes = list;
  } else {
    issues.push(...prefix('outcomes', [hardIssue(invalidType('array', rawOutcomes))]));
  }

  // The top level is NOT strict — unknown keys are tolerated here on purpose.

  if (name.value === undefined || description.value === undefined || outcomes === undefined) {
    return { success: false, issues };
  }
  if (aborted(issues)) return { success: false, issues };

  // Built in declaration order, absent optionals omitted, so `JSON.stringify` of
  // this matches Zod's own output byte for byte.
  const tool = {} as QtapCustomTool;
  if (schema !== undefined) tool.$schema = schema;
  tool.name = name.value;
  if (title !== undefined) tool.title = title;
  tool.description = description.value;
  if (disabled !== undefined) tool.disabled = disabled;
  if (availableWhen !== undefined) tool.availableWhen = availableWhen;
  if (withheldWhen !== undefined) tool.withheldWhen = withheldWhen;
  if (revealOdds !== undefined) tool.revealOdds = revealOdds;
  if (defaultVisibility !== undefined) tool.defaultVisibility = defaultVisibility;
  if (parameters !== undefined) tool.parameters = parameters;
  if (roll !== undefined) tool.roll = roll;
  if (llm !== undefined) tool.llm = llm;
  tool.outcomes = outcomes;

  // ---- superRefine (v4 `:374-377`), in source order -----------------------
  validateOutcomeOrdering(tool.outcomes, issues);
  validateReferences(tool, issues);
  validateGates(tool, issues);

  if (issues.length === 0) return { success: true, data: tool };
  return { success: false, issues };
}

/**
 * The name to show a human — the author's `title`, or one derived from `name`.
 *
 * The single source of the display string: the announcement, the composer
 * popup, and the roster listing all come through here, so
 * `scan_hawking_radiation` reads as "Scan Hawking Radiation" in every one of
 * them without three implementations agreeing by luck. The model never sees
 * this — it calls tools by `name`, and a second string for one tool would only
 * invite it to pass the wrong one.
 */
export function displayTitle(definition: Pick<QtapCustomTool, 'name' | 'title'>): string {
  const authored = definition.title?.trim();
  if (authored) return authored;

  return definition.name
    .split(/[_-]+/)
    .filter((word) => word.length > 0)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

/**
 * Rule: the final outcome must be the literal `true`, and no earlier outcome
 * may be.
 *
 * The trailing catch-all makes a coverage gap structurally impossible — there is
 * always exactly one outcome to land on. An earlier catch-all would make
 * everything below it dead, which is a typo rather than an intent.
 */
function validateOutcomeOrdering(outcomes: CustomToolOutcome[], issues: Issue[]): void {
  if (outcomes.length === 0) return;

  outcomes.forEach((outcome, i) => {
    const isCatchAll = outcome.when === true;
    const isLast = i === outcomes.length - 1;

    if (isCatchAll && !isLast) {
      issues.push(
        checkIssue(
          `outcome ${i} is a catch-all (when: true), so every outcome after it is unreachable`,
          ['outcomes', String(i), 'when'],
        ),
      );
    }

    if (isLast && !isCatchAll) {
      issues.push(
        checkIssue(
          'the final outcome must be a catch-all (when: true) so every roll lands somewhere',
          ['outcomes', String(i), 'when'],
        ),
      );
    }
  });
}

/**
 * Rule: a definition gates one way or the other, never both.
 *
 * The two clauses are not complements — `withheldWhen` and a negated
 * `availableWhen` differ precisely on the character who lacks the key, which is
 * the whole reason both exist — so a file carrying both is asking two questions
 * whose interaction its author almost certainly has not thought through. It is
 * also the shape the Workbench's single "who may reach for it" control cannot
 * represent, and a form that silently drops half a file is worse than a
 * rejection that says which half.
 */
function validateGates(
  tool: { availableWhen?: ToolGate; withheldWhen?: ToolGate },
  issues: Issue[],
): void {
  if (tool.availableWhen && tool.withheldWhen) {
    issues.push(
      checkIssue(
        'declares both availableWhen and withheldWhen — a definition gates one way or the other. ' +
          'Fold the second test into the first, remembering that a key the character lacks never matches.',
        ['withheldWhen'],
      ),
    );
  }
}

/** The value types a subject or an operand can carry, with `integer` folded in. */
type ValueType = 'number' | 'string' | 'boolean';

/** Fold a declared parameter type down to the type its values actually have. */
function valueTypeOf(type: ParameterType): ValueType {
  return type === 'integer' ? 'number' : type;
}

/**
 * Rule: every `$param` reference — in a roll field or in a comparator — must
 * name a declared parameter, and the comparison it takes part in must be one
 * that can hold at run time.
 *
 * All of this is authoring error caught at load: a misspelled parameter, an
 * ordering test against a string, `{ "eq": "brass" }` posed to a number. Left to
 * run time these are silent — a test that simply never fires reads as a dead
 * branch in the outcome table rather than the typo it is.
 */
function validateReferences(tool: QtapCustomTool, issues: Issue[]): void {
  const declared = tool.parameters ?? {};

  validateRollRefs(declared, tool.roll, issues);

  tool.outcomes.forEach((outcome, i) => {
    if (outcome.when === true) return;
    const when = outcome.when;
    const path = ['outcomes', String(i), 'when'];

    // Bare comparator keys, and `roll`, both address a number.
    validateComparator(declared, when, 'number', 'the rolled value', issues, path);
    if (when.roll !== undefined) {
      validateComparator(declared, when.roll, 'number', 'the raw roll', issues, [...path, 'roll']);
    }

    for (const [name, comparator] of Object.entries(when.params ?? {})) {
      const target = declared[name];
      if (!target) {
        issues.push(checkIssue(`tests undeclared parameter "${name}"`, [...path, 'params', name]));
        continue;
      }
      validateComparator(declared, comparator, valueTypeOf(target.type), `parameter "${name}"`, issues, [
        ...path,
        'params',
        name,
      ]);
    }

    // An `llm` test on a tool with no `llm` block could never fire — there is
    // no consult whose answer it might match. That is a typo, not an intent,
    // and left alone it reads as a dead branch in the outcome table.
    if (when.llm !== undefined) {
      if (!tool.llm) {
        issues.push(
          checkIssue('tests the LLM consult, but the tool declares no `llm` block', [...path, 'llm']),
        );
      }
      // The answer's run-time type is the model's business (see the schema
      // comment), so — exactly as with `metadata` — only the `$param` operands
      // are checkable here.
      validateMetadataOperands(declared, when.llm, issues, [...path, 'llm']);
    }

    // `metadata` gets a shallower check, and there is no way around it: the keys
    // live on a character the file has never seen, so neither the key's
    // existence nor its stored type is knowable here. What IS checkable is the
    // operand — a `$param` reference must still resolve to a declared parameter,
    // exactly as anywhere else. The rest (absent key, wrong type, non-primitive
    // value) is caught fail-soft at run time by `matchesWhen`, where the
    // character is finally in the room.
    for (const [key, comparator] of Object.entries(when.metadata ?? {})) {
      validateMetadataOperands(declared, comparator, issues, [...path, 'metadata', key]);
    }
  });
}

/**
 * Check only what a metadata comparator can be checked on at load: that every
 * `$param` operand names a declared parameter. Deliberately silent about the
 * subject's type — see the caller.
 */
function validateMetadataOperands(
  declared: Record<string, CustomToolParameter>,
  comparator: MetadataComparator,
  issues: Issue[],
  path: string[],
): void {
  for (const key of COMPARATOR_KEYS) {
    const operand = (comparator as Record<string, unknown>)[key];
    if (operand === undefined) continue;
    resolveOperandType(declared, operand, issues, [...path, key]);
  }
}

/**
 * Roll fields are numeric, so every reference in one must resolve to a number:
 * a `$param` must name a numeric parameter, and a `$state` ref must carry a
 * numeric fallback (the only thing knowable about it at load time).
 */
function validateRollRefs(
  declared: Record<string, CustomToolParameter>,
  roll: Roll | undefined,
  issues: Issue[],
): void {
  if (!roll || typeof roll === 'string') return;

  for (const [field, value] of Object.entries(roll)) {
    if (isStateRef(value)) {
      if (typeof value.fallback !== 'number') {
        issues.push(
          checkIssue(
            `roll.${field} uses a $state reference whose fallback is ${typeof value.fallback} rather than a number`,
            ['roll', field],
          ),
        );
      }
      continue;
    }

    if (!isParamRef(value)) continue;

    const target = declared[value.$param];
    if (!target) {
      issues.push(
        checkIssue(`roll.${field} references undeclared parameter "${value.$param}"`, ['roll', field]),
      );
      continue;
    }

    if (target.type !== 'number' && target.type !== 'integer') {
      issues.push(
        checkIssue(
          `roll.${field} references parameter "${value.$param}", which is ${target.type} rather than numeric`,
          ['roll', field],
        ),
      );
    }
  }
}

/**
 * Check one comparator against the type of what it tests: every operand must
 * resolve, and the comparison must be one the two types can sustain.
 */
function validateComparator(
  declared: Record<string, CustomToolParameter>,
  comparator: NumericComparator | ParamComparator | WhenObject,
  subjectType: ValueType,
  subjectLabel: string,
  issues: Issue[],
  path: string[],
): void {
  for (const key of COMPARATOR_KEYS) {
    const operand = (comparator as Record<string, unknown>)[key];
    if (operand === undefined) continue;

    const operandType = resolveOperandType(declared, operand, issues, [...path, key]);
    if (operandType === null) continue;

    if (ORDERING_KEYS.includes(key)) {
      if (subjectType !== 'number' || operandType !== 'number') {
        issues.push(
          checkIssue(
            `${key} orders ${subjectLabel} against a ${operandType}, and only numbers can be ordered`,
            [...path, key],
          ),
        );
      }
      continue;
    }

    if (CONTAINMENT_KEYS.includes(key)) {
      if (subjectType !== 'string') {
        issues.push(
          checkIssue(
            `${key} searches ${subjectLabel}, which is a ${subjectType} — only a string can contain a substring`,
            [...path, key],
          ),
        );
      } else if (operandType !== 'string') {
        issues.push(
          checkIssue(
            `${key} looks for a ${operandType} inside ${subjectLabel}, and a substring must be a string`,
            [...path, key],
          ),
        );
      }
      continue;
    }

    if (subjectType !== operandType) {
      issues.push(
        checkIssue(
          `${key} compares ${subjectLabel}, which is a ${subjectType}, with a ${operandType} — this can never hold`,
          [...path, key],
        ),
      );
    }
  }
}

/**
 * The type an operand carries: a literal's own, or that of the parameter it
 * references. null means the operand is broken and has already been reported.
 */
function resolveOperandType(
  declared: Record<string, CustomToolParameter>,
  operand: unknown,
  issues: Issue[],
  path: string[],
): ValueType | null {
  if (isParamRef(operand)) {
    const target = declared[operand.$param];
    if (!target) {
      issues.push(checkIssue(`references undeclared parameter "${operand.$param}"`, path));
      return null;
    }
    return valueTypeOf(target.type);
  }

  // A `$state` operand carries the type of its (required) fallback — that is
  // what run-time resolution is guaranteed to produce when the path misses.
  if (isStateRef(operand)) {
    const fallbackType = typeof operand.fallback;
    if (fallbackType === 'number' || fallbackType === 'string' || fallbackType === 'boolean') {
      return fallbackType;
    }
    // Unreachable: the schema types the fallback before this runs.
    return null;
  }

  const literal = typeof operand;
  if (literal === 'number' || literal === 'string' || literal === 'boolean') return literal;

  // Unreachable: the schema types operands before this runs.
  return null;
}

// ------------------------------------------------------------------ rendering

/**
 * Render a rejection as the sentence an author reads on the load-error badge.
 *
 * Exists because of unions. `when` is `true | object` and `roll` is
 * `string | object`, and when both branches fail Zod reports a bare "Invalid
 * input" at the union and buries the actual complaint — the misspelled
 * comparator, the malformed dice notation — one level down in `issue.errors`. A
 * rejection nobody can read is barely better than no rejection at all, so every
 * branch's message is surfaced, joined by "or" since either would have satisfied
 * the schema.
 */
export function formatDefinitionIssues(issues: readonly Issue[]): string {
  return flattenIssues(issues).join('; ');
}

/** The per-issue lines the JSON editor lists, each `path: message`. */
export function definitionIssueLines(issues: readonly Issue[]): string[] {
  return flattenIssues(issues);
}

function flattenIssues(issues: readonly Issue[]): string[] {
  return issues.map((issue) => {
    const located = (message: string) =>
      issue.path.length ? `${issue.path.join('.')}: ${message}` : message;

    if (issue.unionErrors) {
      // Sub-issue paths are relative to the union, so they are flattened without
      // the prefix — v4 does the same.
      const branches = issue.unionErrors
        .map((branch) => flattenIssues(branch).join('; '))
        .filter((branch) => branch.length > 0);
      if (branches.length > 0) return located(branches.join(' — or — '));
    }

    return located(issue.message);
  });
}

/** Top-level keys the v1 format knows about. Anything else is reserved for v2. */
const KNOWN_TOP_LEVEL_KEYS = new Set([
  '$schema',
  'name',
  'title',
  'description',
  'disabled',
  'availableWhen',
  'withheldWhen',
  'revealOdds',
  'defaultVisibility',
  'parameters',
  'roll',
  'llm',
  'outcomes',
]);

/**
 * Report top-level keys this build doesn't understand, so discovery can log
 * them. They are tolerated, not rejected: `persist` is a planned v2 key, and an
 * older build must not choke on a newer file.
 */
export function collectUnknownKeys(raw: unknown): string[] {
  if (typeof raw !== 'object' || raw === null) return [];
  return Object.keys(raw as Record<string, unknown>).filter((k) => !KNOWN_TOP_LEVEL_KEYS.has(k));
}
