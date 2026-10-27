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
 * Design constraint that shapes everything below: tests are AND-composed
 * comparator objects; a one-level any-of array of them is OR; there is no
 * nesting and no expression grammar. Indirection is limited to closed forms —
 * a `{ "$param": "name" }` reference, a `{ "$state": "path", "fallback":
 * <literal> }` reference and its Inventory twin `{ "$inventory":
 * "<ref>.metadata.<key>", "fallback": <literal> }`. The **one** place a string grammar exists is an
 * effect's `value`, evaluated by the closed, eval-free parser in
 * `./expressions`: arithmetic, string concatenation, parentheses, literals, and
 * `{{ref}}` substitution. There are no identifiers, no function calls, and no
 * member access — the only names that grammar admits are the same `{{...}}`
 * reference families `renderTemplate` already substitutes, so there is still
 * nothing callable and nothing reachable beyond the run's own subjects.
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
 * is a small engine rather than a pile of `if`s. (Still true at 4.6.5 — v4
 * `6b0615807`: P4.D211 re-recorded the shared 362-row corpus from v4's real
 * schema under BOTH versions and got byte-identical output, so the rules below
 * are verified across the range. The Rust twin
 * `crates/quilltap-core/src/pascal/custom_tool_types.rs` carries the same note.)
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
 * `custom-tool-types.corpus.spec.ts`, which replays the committed 537-row
 * oracle corpus (457 definitions + 70 gate verdicts + 10 titles, regenerated at
 * v4 `322a5488e`) and compares the verdict, the parsed data, the unknown-key
 * report, and the full rejection sentence.
 *
 * # The grammars carried as data only
 *
 * v4 `e1872c94f` / `a53024d3c` / `e4a60ad9e` (the Inventory subjects and
 * gestures, the `inventoryAggregate` sheet, the `log` effect, `$inventory`
 * operands) are parsed and validated here WHOLE — the Workbench must phrase a
 * draft's rejection exactly as the server would — but nothing in the browser
 * evaluates them yet: `evaluateToolGate` reads every Inventory test as v4 reads
 * it with no bag (declined).
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

import {
  PROGRESSIONS_METADATA_KEY,
  PROGRESSION_ID_PATTERN,
  WRITABLE_PROGRESSION_FIELDS,
  isWritableProgressionField,
  parseProgressKey,
  type WritableProgressionField,
} from '../progressions/schema';
import { MAX_DIE_SIDES, MIN_DIE_SIDES, parseDiceNotation } from './dice-notation';
import {
  aborted,
  at,
  checkIssue,
  hardIssue,
  hasKey,
  invalidType,
  isPlainObject,
  parseBool,
  parseEnum,
  parseFiniteNumber,
  prefix,
  resHard,
  resOk,
  stringLengthIssues,
  union,
  unrecognizedKeys,
  type Issue,
  type Res,
} from './zod-shim';
import { MAX_EFFECT_EXPRESSION_LENGTH, parseExpression } from './expressions';
import { parseAggregateKey } from './inventory-aggregate';
import { scanPlaceholders as scanPlaceholdersInText, splitInventoryReference } from './placeholders';
import { zodLenMaxOk, zodLenMinOk } from './zod-length';
// The slot enum is IMPORTED from the wardrobe registry, never minted here: it
// holds v4's `WARDROBE_SLOT_TYPES` (`wardrobe.types.ts:33-37`) in v4's order,
// and `custom-tool-types.effects.spec.ts` trips if that list ever moves.
import { WARDROBE_SLOT_TYPES } from '../wardrobe/slot-meta';
import type { WardrobeSlotType } from '../core/core-contract';

export { MAX_EFFECT_EXPRESSION_LENGTH };

/** Well-known folder, at a store's root, holding custom-tool definitions. */
export const TOOLS_FOLDER = 'Tools';

/** Filename suffix that marks a document as a custom-tool definition. */
export const TOOL_FILE_SUFFIX = '.tool.json';

/** Cap on parameters per tool. */
export const MAX_PARAMETERS = 8;

/** Cap on outcomes per tool. */
export const MAX_OUTCOMES = 32;

/**
 * Cap on groups in one any-of array — a gate, an outcome `when`, or an effect
 * `when` written as an array of AND-groups.
 */
export const MAX_WHEN_GROUPS = 8;

/** Cap on tools in a single resolved roster. */
export const MAX_ROSTER_SIZE = 64;

/** Cap on an outcome message, in characters. */
export const MAX_MESSAGE_LENGTH = 1000;

/** Cap on a tool description, in characters. */
export const MAX_DESCRIPTION_LENGTH = 500;

/** Cap on a tool's display title, in characters. */
export const MAX_TITLE_LENGTH = 80;

/**
 * Cap on a `chipLabel` TEMPLATE's text, in characters. The rendered result is
 * uncapped here — the UI truncates it via CSS, which is where display concerns
 * belong.
 */
export const MAX_CHIP_LABEL_LENGTH = 160;

/** Cap on effects per tool. */
export const MAX_EFFECTS = 16;

/** Cap on trigger phrases per tool. */
export const MAX_TRIGGERS = 8;

/** Shortest trigger phrase, in characters — keeps `"a"` and `"ok"` out. A length, not a word count. */
export const MIN_TRIGGER_PHRASE_LENGTH = 3;

/** Longest trigger phrase, in characters. */
export const MAX_TRIGGER_PHRASE_LENGTH = 80;

/** Cap on an effect's `target` string, in characters. */
export const MAX_EFFECT_TARGET_LENGTH = 200;

/** Cap on the item references one wardrobe effect may list. */
export const MAX_WARDROBE_EFFECT_ITEMS = 8;

/** Cap on one wardrobe item reference (a title or id template), in characters. */
export const MAX_WARDROBE_ITEM_REF_LENGTH = 200;

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

/** The longest Inventory reference (an object id, title or name, or a `when` key). */
export const MAX_INVENTORY_REF_LENGTH = 200;

/**
 * A reference into one `metadata` key of one Inventory object the rolling
 * character carries or can reach — `"<ref>.metadata.<key>"`, split at the
 * FIRST `.metadata.` (the `{{inventory.*}}` placeholder's rule). Accepted
 * wherever a `$state` reference is, and typed the same way: by its required
 * fallback, which run-time resolution falls back to whenever the object, the
 * key, or a value of the fallback's type is missing. Only `metadata` is
 * addressable here — a title is never a number to roll against. A top-level
 * `roll` may be one with a dice-notation fallback ("1d4"), so a sword's
 * `metadata.damage` of "2d6" can be the roll itself (v4 `e1872c94f`).
 */
export interface InventoryRef {
  $inventory: string;
  fallback: number | string | boolean;
}

/** v4 `InventoryRefSchema`'s refine message. */
const INVENTORY_REF_MESSAGE = 'must be "<object>.metadata.<key>", e.g. "sword.metadata.damage"';

/** A roll field: a literal number, a `$param`, a `$state` or a `$inventory` reference. */
export type NumberOrParamRef = number | ParamRef | StateRef | InventoryRef;

/**
 * A comparator operand for eq/neq, which may address a parameter of any
 * declared type — `{ "eq": "brass" }` is a legitimate test of a string — or a
 * `$state` reference typed by its fallback.
 */
export type AnyOperand = number | string | boolean | ParamRef | StateRef | InventoryRef;

/**
 * A comparator operand for contains/ncontains: the substring to look for — a
 * literal string, a `$param` reference to a declared string parameter, or a
 * `$state` reference whose fallback is a string. The literal must be non-empty
 * because every string contains "", which makes an empty needle a typo wearing
 * a comparator's clothes.
 */
export type StringOperand = string | ParamRef | StateRef | InventoryRef;

/** True when a roll field is a `$param` reference rather than a literal. */
export function isParamRef(value: unknown): value is ParamRef {
  return typeof value === 'object' && value !== null && '$param' in value;
}

/** True when a value is a `$state` reference rather than a literal or `$param`. */
export function isStateRef(value: unknown): value is StateRef {
  return typeof value === 'object' && value !== null && '$state' in value;
}

/** True when a value is a `$inventory` reference. */
export function isInventoryRef(value: unknown): value is InventoryRef {
  return typeof value === 'object' && value !== null && '$inventory' in value;
}

/**
 * A declared parameter. Every parameter requires a `default` so that a
 * zero-argument run is always possible — the model may reach for a tool without
 * supplying anything, and the table must still deal.
 */
export interface CustomToolParameter {
  type: ParameterType;
  default: number | string | boolean | StateRef | InventoryRef;
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

/**
 * Either form of roll — dice notation, or a range object — or a `$inventory`
 * reference whose (dice-notation) fallback stands in when the object's
 * metadata holds no rollable notation.
 */
export type Roll = string | RollRange | InventoryRef;

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

/** The four states an `inventory` test compares against (v4 `InventoryStateValueSchema`). */
export type InventoryStateValue = 'equipped' | 'carried' | 'reachable' | 'absent';

/** v4 `InventoryStateValueSchema`'s options, in its order. */
export const INVENTORY_STATE_VALUES: readonly InventoryStateValue[] = [
  'equipped',
  'carried',
  'reachable',
  'absent',
];

/**
 * A test of one object's state: `eq` / `neq` only (an ordering key is a
 * rejection — a state has no order). The operand is one of the four states
 * or a `$param` naming a string parameter.
 */
export interface InventoryStateComparator {
  eq?: InventoryStateValue | ParamRef;
  neq?: InventoryStateValue | ParamRef;
}

/** The state test a gate may pose: literal states only. */
export interface GateInventoryStateComparator {
  eq?: InventoryStateValue;
  neq?: InventoryStateValue;
}

/** The count test a gate may pose: literal numbers only, no containment. */
export interface GateInventoryCountComparator {
  gt?: number;
  gte?: number;
  lt?: number;
  lte?: number;
  eq?: number;
  neq?: number;
}

/**
 * One group of an availability gate: tests that must ALL hold (v4
 * `ToolGateGroupSchema`, `a53024d3c` — the old one-object `ToolGateSchema`).
 *
 * Keyed by metadata key or by `"<id>.<field>"`, AND-composed across every
 * subject, and fail-soft in exactly the way an outcome's `metadata` test is —
 * a key the character lacks does not match. The subjects are `metadata` and
 * `progress` and nothing else because a gate is answered BEFORE the deal: there
 * is no roll to test, no parameters (nobody has called anything), and no
 * consult. What a character carries into the room is their `metadata.json` and,
 * derived from the `progressions` key inside it against the wall clock, their
 * timed progressions.
 *
 * Each subject lives under its own key rather than at the top of the object,
 * which is what let `progress` join at `0587d1e96` without re-shaping every file
 * already written. Both are OPTIONAL since then, and the "must test at least one
 * metadata key or progress field" refine moved from the metadata RECORD to the
 * gate OBJECT — so its issue path is the gate, not `gate.metadata`.
 */
export interface ToolGateGroup {
  metadata?: Record<string, GateComparator>;
  /**
   * Test the invoking character's timed progressions, keyed `"<id>.<field>"`.
   * Derived fresh from the wall clock at roster time; a progression the
   * character does not carry does not match (v4 `0587d1e96`).
   */
  progress?: Record<string, GateComparator>;
  /**
   * Test an object's state for the invoking character in this chat, keyed by a
   * LITERAL object reference (id, title or name). With no character or chat,
   * every test declines (v4 `e1872c94f`).
   */
  inventory?: Record<string, GateInventoryStateComparator>;
  /** Test how many kin (same name) of an object the invoking character has equipped in this chat. */
  inventoryCount?: Record<string, GateInventoryCountComparator>;
  /** Test one attribute across every object in a scope of the invoker's bag, keyed `"<scope>.<field>"`. */
  inventoryAggregate?: Record<string, GateComparator>;
}

/**
 * An availability gate: whether this invoker is offered the tool at all.
 *
 * One group (every test must hold), or an any-of array of groups — the gate
 * holds when ANY group holds. AND binds tighter than OR; there is no nesting.
 */
export type ToolGate = ToolGateGroup | ToolGateGroup[];

/** A gate's groups, in order — one for a bare object. Every reader of a gate goes through here. */
export function gateGroups(gate: ToolGate): ToolGateGroup[] {
  return Array.isArray(gate) ? gate : [gate];
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
 * Tests are AND-composed comparator objects; a one-level any-of array of them
 * is OR (see {@link When}); there is no nesting and no expression grammar,
 * which keeps the evaluator eval-free.
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
  /**
   * Test the invoking character's timed progressions, keyed `"<id>.<field>"` —
   * percent, complete, remainingMs, state and the rest, derived from the wall
   * clock at run start. A progression the character does not carry does not
   * match, exactly as an absent metadata key does not (v4 `0587d1e96`).
   */
  progress?: Record<string, MetadataComparator>;
  llm?: LlmComparator;
  /**
   * Test an object's state for the rolling character in this chat — equipped,
   * carried (own vault), reachable (a shared store) or absent — keyed by object
   * reference (a template). A run nobody made declines every test.
   */
  inventory?: Record<string, InventoryStateComparator>;
  /** Test how many kin (same name) of an object the rolling character has equipped in this chat. */
  inventoryCount?: Record<string, NumericComparator>;
  /** Test one attribute across every object in a scope of the rolling character's bag. */
  inventoryAggregate?: Record<string, MetadataComparator>;
}

/**
 * An outcome test: `true` (catch-all), one AND-group, or an any-of array of
 * AND-groups — the row wins when ANY group holds. AND binds tighter than OR.
 */
export type When = true | WhenObject | WhenObject[];

/** An outcome test's groups, in order — `[]` for the catch-all. Every reader of a `when` goes through here. */
export function whenGroups(when: When): WhenObject[] {
  if (when === true) return [];
  return Array.isArray(when) ? when : [when];
}

/**
 * An effect's condition. The outcome-row `when` comparator language plus ONE
 * new subject — `outcome`, the winning row's semantic state — because an effect
 * is evaluated after the table has dealt, when there finally IS a winning
 * outcome to test.
 *
 * A separate type rather than a widened {@link WhenObject} on purpose: an
 * `outcome` subject inside an outcome row would be a self-referential dead
 * branch (the row cannot test a verdict it has not yet won), and keeping the
 * shapes separate leaves the run-time matcher untouched.
 */
export interface EffectWhen {
  gt?: NumberOrParamRef;
  gte?: NumberOrParamRef;
  lt?: NumberOrParamRef;
  lte?: NumberOrParamRef;
  eq?: NumberOrParamRef;
  neq?: NumberOrParamRef;
  roll?: NumericComparator;
  params?: Record<string, ParamComparator>;
  metadata?: Record<string, MetadataComparator>;
  /**
   * Test the invoking character's timed progressions, keyed `"<id>.<field>"` —
   * percent, complete, remainingMs, state and the rest, derived from the wall
   * clock at run start. A progression the character does not carry does not
   * match, exactly as an absent metadata key does not (v4 `0587d1e96`).
   */
  progress?: Record<string, MetadataComparator>;
  llm?: LlmComparator;
  /**
   * Test an object's state for the rolling character in this chat — equipped,
   * carried (own vault), reachable (a shared store) or absent — keyed by object
   * reference (a template). A run nobody made declines every test.
   */
  inventory?: Record<string, InventoryStateComparator>;
  /** Test how many kin (same name) of an object the rolling character has equipped in this chat. */
  inventoryCount?: Record<string, NumericComparator>;
  /** Test one attribute across every object in a scope of the rolling character's bag. */
  inventoryAggregate?: Record<string, MetadataComparator>;
  /** Test the WINNING outcome's semantic state. */
  outcome?: { eq?: OutcomeState; neq?: OutcomeState };
}

/** An effect condition: one group, or an any-of array of groups. */
export type EffectCondition = EffectWhen | EffectWhen[];

/** An effect condition's groups, in order. Every reader of an effect `when` goes through here. */
export function effectWhenGroups(when: EffectCondition): EffectWhen[] {
  return Array.isArray(when) ? when : [when];
}

/** What a wardrobe effect does to the rolling character's outfit. */
export type WardrobeEffectAction = 'wear' | 'take_off' | 'clear_slot';

/** v4 `WardrobeEffectActionSchema`'s options, in its order. */
export const WARDROBE_EFFECT_ACTIONS: readonly WardrobeEffectAction[] = ['wear', 'take_off', 'clear_slot'];

/** The put-on modes a `wear` gesture may name — the `wardrobe_wear` tool's own. */
export type WardrobeEffectMode = 'wear' | 'replace' | 'add_to_slot';

/** v4 `WardrobeEffectModeSchema`'s options, in its order. */
export const WARDROBE_EFFECT_MODES: readonly WardrobeEffectMode[] = ['wear', 'replace', 'add_to_slot'];

/**
 * A wardrobe slot. v4 re-exports `WardrobeItemType` from its wardrobe schema
 * module (`export type { WardrobeItemType }`); the SPA's twin of that module is
 * the wardrobe registry (`../wardrobe/slot-meta`), whose type this is.
 */
export type WardrobeItemType = WardrobeSlotType;

/**
 * A wardrobe gesture: wear the listed items, take them off, or empty one slot,
 * on the ROLLING character and nobody else.
 *
 * Item references are TEMPLATES, not expressions: each is rendered through
 * `renderTemplate` (so `{{params.garment}}` lets a parameter choose) and then
 * looked up by id, then by case-insensitive title, in the character's wearable
 * pool. The per-action cross-field rules (which action needs `items`, which
 * needs `slot`) live in `validateEffects`, not here, so the issues they raise
 * carry precise paths.
 */
export interface CustomToolWardrobeGesture {
  action: WardrobeEffectAction;
  items?: string[];
  mode?: WardrobeEffectMode;
  slot?: WardrobeItemType;
}

/** The most object references one `inventory` gesture may name. */
export const MAX_INVENTORY_EFFECT_ITEMS = 8;

/** What an `inventory` effect does for the rolling character. */
export type InventoryEffectAction = 'equip' | 'unequip' | 'pick_up';

/** v4 `InventoryEffectActionSchema`'s options, in its order. */
export const INVENTORY_EFFECT_ACTIONS: readonly InventoryEffectAction[] = ['equip', 'unequip', 'pick_up'];

/**
 * An Inventory gesture: equip, unequip or pick up objects for the ROLLING
 * character in this chat, through the chokepoints the operator's dialog uses.
 * Item references are TEMPLATES, rendered and then resolved through the one
 * reference rule (id → title → name). Kits are allowed for equip and unequip;
 * `pick_up` names archetypes and instantiates into the character's vault
 * root, equipping unless `equip` is false (v4 `e1872c94f`).
 */
export interface CustomToolInventoryGesture {
  action: InventoryEffectAction;
  items: string[];
  equip?: boolean;
}

/**
 * One side effect, recorded after the table has dealt. A WRITE (`target` +
 * `value`), a WARDROBE GESTURE (`wardrobe`), an INVENTORY GESTURE
 * (`inventory`) or a LOG LINE (`log`) — exactly one (v4 `f0db8cbb8`, widened
 * by `e1872c94f` / `e4a60ad9e`). One strict object with a refine rather than a
 * union, so a mistake produces one readable issue rather than a union's tree of
 * them.
 *
 * `value` discrimination is the one ergonomic trap in the format: a JSON number
 * or boolean is a literal, stored as-is, but a JSON **string is always an
 * expression** — so literal prose must be quoted INSIDE the expression
 * (`"value": "'broken pick'"`, not `"value": "broken pick"`; the bare form is a
 * load-time parse error, and a loud one).
 */
export interface CustomToolEffect {
  when?: EffectCondition;
  target?: string;
  value?: number | boolean | string;
  wardrobe?: CustomToolWardrobeGesture;
  inventory?: CustomToolInventoryGesture;
  /**
   * A line for the log (the one `log_read` and `log_grep` search) instead of a
   * write. A template, like an outcome message.
   */
  log?: string;
}

/** An effect that writes one value. */
export type CustomToolWriteEffect = CustomToolEffect & { target: string; value: number | boolean | string };

/** An effect that changes the rolling character's outfit. */
export type CustomToolWardrobeEffect = CustomToolEffect & { wardrobe: CustomToolWardrobeGesture };

/** An effect that changes what the rolling character carries. */
export type CustomToolInventoryEffect = CustomToolEffect & { inventory: CustomToolInventoryGesture };

/** An effect that writes a line to the log. */
export type CustomToolLogEffect = CustomToolEffect & { log: string };

/** True for a write effect. Use this rather than re-testing keys. */
export function isWriteEffect(effect: CustomToolEffect): effect is CustomToolWriteEffect {
  return (
    effect.wardrobe === undefined &&
    effect.inventory === undefined &&
    effect.log === undefined &&
    effect.target !== undefined &&
    effect.value !== undefined
  );
}

/** True for a wardrobe gesture. Use this rather than re-testing keys. */
export function isWardrobeEffect(effect: CustomToolEffect): effect is CustomToolWardrobeEffect {
  return (
    effect.wardrobe !== undefined &&
    effect.inventory === undefined &&
    effect.log === undefined &&
    effect.target === undefined &&
    effect.value === undefined
  );
}

/** True for an Inventory gesture. Use this rather than re-testing keys. */
export function isInventoryEffect(effect: CustomToolEffect): effect is CustomToolInventoryEffect {
  return (
    effect.inventory !== undefined &&
    effect.wardrobe === undefined &&
    effect.log === undefined &&
    effect.target === undefined &&
    effect.value === undefined
  );
}

/** True for a log line. Use this rather than re-testing keys. */
export function isLogEffect(effect: CustomToolEffect): effect is CustomToolLogEffect {
  return (
    effect.log !== undefined &&
    effect.wardrobe === undefined &&
    effect.inventory === undefined &&
    effect.target === undefined &&
    effect.value === undefined
  );
}

/**
 * The per-action rules of an Inventory gesture — the one statement of them,
 * read by load-time validation and the Workbench's draft check.
 */
export function inventoryGestureProblems(
  gesture: Pick<CustomToolInventoryGesture, 'action' | 'equip'> & { items?: readonly string[] },
): Array<{ field: 'items' | 'equip'; message: string }> {
  const problems: Array<{ field: 'items' | 'equip'; message: string }> = [];
  if ((gesture.items?.length ?? 0) === 0) {
    problems.push({ field: 'items', message: `${gesture.action} needs at least one object` });
  }
  if (gesture.equip !== undefined && gesture.action !== 'pick_up') {
    problems.push({ field: 'equip', message: 'equip is only used with pick_up' });
  }
  return problems;
}

/**
 * The per-action rules of a wardrobe gesture, as `[field, message]` pairs. The
 * single statement of them: load-time validation and the Workbench's draft
 * check both read this, so the two say the same words.
 */
export function wardrobeGestureProblems(
  gesture: Pick<CustomToolWardrobeGesture, 'action' | 'mode' | 'slot'> & { items?: readonly string[] },
): Array<{ field: 'items' | 'mode' | 'slot'; message: string }> {
  const problems: Array<{ field: 'items' | 'mode' | 'slot'; message: string }> = [];
  const hasItems = (gesture.items?.length ?? 0) > 0;
  switch (gesture.action) {
    case 'wear': {
      if (!hasItems) problems.push({ field: 'items', message: 'wear needs at least one item' });
      const mode = gesture.mode ?? 'wear';
      if (mode === 'add_to_slot' && gesture.slot === undefined) {
        problems.push({ field: 'slot', message: 'mode add_to_slot needs a slot' });
      } else if (mode !== 'add_to_slot' && gesture.slot !== undefined) {
        problems.push({ field: 'slot', message: 'slot is only used with mode add_to_slot' });
      }
      break;
    }
    case 'take_off':
      if (!hasItems) problems.push({ field: 'items', message: 'take_off needs at least one item' });
      if (gesture.mode !== undefined) problems.push({ field: 'mode', message: 'mode is only used with wear' });
      break;
    case 'clear_slot':
      if (gesture.slot === undefined) problems.push({ field: 'slot', message: 'clear_slot needs a slot' });
      if (gesture.items !== undefined) problems.push({ field: 'items', message: 'clear_slot takes no items' });
      if (gesture.mode !== undefined) problems.push({ field: 'mode', message: 'mode is only used with wear' });
      break;
  }
  return problems;
}

/** A parsed effect target, with the raw text kept for records and messages. */
export type EffectTarget =
  | { kind: 'state'; path: Array<string | number>; raw: string }
  | { kind: 'metadata'; key: string; raw: string }
  | { kind: 'progress'; id: string; field: WritableProgressionField; raw: string };

/**
 * v4 `lib/state/state-paths.ts` `parsePath` — the one function of that module
 * this file needs, transcribed rather than imported because the SPA has no
 * state-path module (the state tool and the cascade resolver are server-side).
 * Dot notation plus array indexing: `player.inventory[0].name`.
 *
 * Carries v4's known limitation deliberately: the `\w+` segment pattern means
 * keys containing spaces or dots are unreachable via a path string.
 */
function parseStatePath(path: string | undefined): Array<string | number> {
  if (!path || path.trim() === '') return [];

  const result: Array<string | number> = [];
  const regex = /(\w+)|\[(\d+)\]/g;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(path)) !== null) {
    if (match[1] !== undefined) {
      result.push(match[1]);
    } else if (match[2] !== undefined) {
      result.push(parseInt(match[2], 10));
    }
  }

  return result;
}

/**
 * Parse an effect's `target`. The single parser for the syntax — validation,
 * the applier, and the Workbench all come through here, so "what counts as a
 * writable target" is decided exactly once.
 *
 * - `state.<path>` — the remainder is a state path ({@link parseStatePath}'s
 *   dot/bracket syntax). An empty path is rejected, and so is a first segment
 *   starting with `_`: those keys are the user's own (the `state` tool's
 *   underscore guard), and no AI-adjacent path may write them.
 * - `metadata.<key>` — the remainder is taken WHOLE as the key. Metadata keys
 *   are the user's vocabulary, so dots inside the key are fine precisely
 *   because it is not path-parsed.
 * - `progress.<id>.<field>` — a field of one of the rolling character's timed
 *   progressions. Unlike a metadata key, BOTH halves are the format's own
 *   vocabulary and both are checked here: the id against the progression
 *   identifier rule, the field against the closed writable set (which includes
 *   the `remove` pseudo-field — write `true` to delete the progression).
 *   Writing an id nobody authored CREATES the progression, so there is nothing
 *   to check about existence; what would be a silent no-op is a `quantity`
 *   written whole, or a `percent` written at all, and both are rejected here
 *   with the field list in the reason.
 */
export function parseEffectTarget(
  target: string,
): { ok: true; target: EffectTarget } | { ok: false; reason: string } {
  if (target.startsWith('state.')) {
    const rest = target.slice('state.'.length);
    const path = parseStatePath(rest);
    if (path.length === 0) {
      return { ok: false, reason: 'names no state path after "state."' };
    }
    const first = path[0];
    if (typeof first === 'string' && first.startsWith('_')) {
      return {
        ok: false,
        reason: `writes "${rest}" — state keys starting with an underscore are user-only, and no tool may write them`,
      };
    }
    return { ok: true, target: { kind: 'state', path, raw: target } };
  }

  // Checked BEFORE `metadata.`, which it does not prefix-collide with, but
  // ordering it first keeps the two user-facing families adjacent below.
  if (target.startsWith('progress.')) {
    const rest = target.slice('progress.'.length);
    const dot = rest.indexOf('.');
    if (dot <= 0 || dot === rest.length - 1) {
      return {
        ok: false,
        reason: 'must name "progress.<progression id>.<field>" — e.g. "progress.cannon.endTime"',
      };
    }
    const id = rest.slice(0, dot);
    const field = rest.slice(dot + 1);
    if (!PROGRESSION_ID_PATTERN.test(id)) {
      return {
        ok: false,
        reason: `writes progression "${id}", which is not a valid id — lowercase, starting with a letter, then letters, digits, _ or - (at most 64)`,
      };
    }
    if (!isWritableProgressionField(field)) {
      return {
        ok: false,
        reason: `writes "${field}", which is not a writable progression field — use one of ${WRITABLE_PROGRESSION_FIELDS.join(', ')}`,
      };
    }
    return { ok: true, target: { kind: 'progress', id, field, raw: target } };
  }

  if (target.startsWith('metadata.')) {
    const key = target.slice('metadata.'.length);
    if (key.length === 0) {
      return { ok: false, reason: 'names no metadata key after "metadata."' };
    }
    // The reserved key is not writable through this door. An effect's value is
    // always a PRIMITIVE, so `metadata.progressions` would replace the whole
    // progressions object with a string or a number — wiping every timed span
    // the character carries, past the schema validation and the rollback that
    // guard the `progress.` path, and fail-soft enough on the next read that
    // nobody would notice. `metadata.progressions.cannon` is refused for the
    // adjacent reason: a metadata key is taken WHOLE, so that writes a literal
    // key named "progressions.cannon" and touches no progression at all, which
    // is not what anyone writing it means.
    if (key === PROGRESSIONS_METADATA_KEY || key.startsWith(`${PROGRESSIONS_METADATA_KEY}.`)) {
      return {
        ok: false,
        reason:
          `writes the reserved "${PROGRESSIONS_METADATA_KEY}" key through "metadata." — ` +
          'use "progress.<id>.<field>" instead, which is validated and rolled back on a bad result',
      };
    }
    return { ok: true, target: { kind: 'metadata', key, raw: target } };
  }

  return { ok: false, reason: 'must start with "state.", "metadata." or "progress."' };
}

export interface CustomToolOutcome {
  when: When;
  message: string;
  state: OutcomeState;
  /**
   * An instruction for the LLM consult's narrator: the conclusion this outcome
   * means, in the author's words. When this row wins, the consult prompt may
   * quote it as `{{outcome.brief}}`, so the model is TOLD the verdict the table
   * reached instead of asked to work it out. Only meaningful on a tool with an
   * `llm` block, and only readable by the prompt while no row tests `llm` (v4
   * `322a5488e`, bug 257).
   */
  brief?: string;
}

/** True when any outcome row's `when` tests the consult (`llm`, or its `ok`). */
export function outcomeTableTestsLlm(outcomes: ReadonlyArray<Pick<CustomToolOutcome, 'when'>>): boolean {
  return outcomes.some((outcome) => whenGroups(outcome.when).some((group) => group.llm !== undefined));
}

/**
 * The custom-tool definition.
 *
 * Unknown TOP-LEVEL keys are tolerated, which reserves room for future keys (as
 * it once did for `effects`, which shipped under that tolerance) without
 * breaking older builds. {@link collectUnknownKeys} surfaces them for a debug
 * log.
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
  /**
   * Templated label for the outcome chip and the announcement header. Same
   * placeholders as an outcome message, rendered after the outcome is chosen.
   * Blank/absent = the title labels the chip.
   */
  chipLabel?: string;
  /**
   * Phrases that run this tool automatically, with every parameter at its
   * default, when a message from anyone in the chat contains one — exact
   * substrings, case-insensitive, no wildcards. Only honoured when the chat
   * setting "Auto-Run Custom Tools on Trigger Phrases" is on (it is off by
   * default). 1–8 phrases, each 3–80 characters, stored as written (v4
   * `362426c8f`).
   */
  triggers?: string[];
  description: string;
  disabled?: boolean;
  /**
   * Offer this tool ONLY to an invoker who satisfies every test here (or, for
   * an array, every test of any one element).
   */
  availableWhen?: ToolGate;
  /**
   * Withhold this tool from an invoker who satisfies every test here (or, for
   * an array, every test of any one element).
   */
  withheldWhen?: ToolGate;
  revealOdds?: boolean;
  defaultVisibility?: Visibility;
  /**
   * true: the rolling character announces the outcome under their own name and
   * face, in place of Pascal. A run nobody made (the bare operator) is still
   * announced by Pascal. Default false. (v4 `a05c6f755`,
   * `custom-tool.types.ts:1013-1019`.)
   */
  announceAsCharacter?: boolean;
  parameters?: Record<string, CustomToolParameter>;
  roll?: Roll;
  llm?: CustomToolLlm;
  /** Side effects applied after the run: writes into tiered persistent state or
   *  the rolling character's metadata, wardrobe gestures that change what the
   *  rolling character is wearing, inventory gestures that change what they
   *  carry, or log lines. */
  effects?: CustomToolEffect[];
  outcomes: CustomToolOutcome[];
}

// ---------------------------------------------------------------- issue model

/**
 * One rejection, in Zod's shape. Re-exported from `./zod-shim`, where the issue
 * model and the leaf validators moved when `progressions/schema.ts` became
 * their second consumer (P4.D170) — importers of this module are unchanged.
 */
export type { Issue };

/**
 * The two pieces of a Zod issue this module's renderer reads that the shared
 * shim's model does not carry: `notArray` marks an any-of array branch's root
 * `expected array` refusal (what v4's `isNotAnArray` recognises by `code ===
 * 'invalid_type' && path.length === 0 && expected === 'array'`), and `inner` is
 * a record key's refusal reasons (Zod's `invalid_key.issues`). Both survive
 * `prefix`, which spreads.
 */
type TaggedIssue = Issue & { notArray?: true; inner?: string[] };

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

  // Zod ≥ 4.5.4 (v4 `6e1a64ea6`) measures strings in code points once the UTF-16
  // count crosses a bound — `zod-length.ts` carries each check's window, and
  // `zod-shim.ts` renders both sentences.
  const issues: Issue[] = stringLengthIssues(input, min, max);
  if (identifier && !IDENTIFIER_PATTERN.test(input)) {
    issues.push(checkIssue(IDENTIFIER_MESSAGE));
  }

  return { value: input, issues };
}

/** One `.min(N)` / `.max(N)` length check on an array, with Zod's own sentence unless authored. */
interface LengthCheck {
  n: number;
  message?: string;
}

/**
 * v4 `z.array(element).min(…).max(…)` under Zod 4.6.7 — the pin's resolved
 * Zod (`25b8fab6a`). Two engine rules decide its issues, both measured against
 * v4's real schema (the `P4.D307 rows` of the committed corpus):
 *
 * 1. **`capSize`** (new in 4.6.7): an array LONGER than its `.max(N)` fails
 *    with ONLY that check's `too_big`, ABORTING, before any element is
 *    parsed — so an oversized list of bad elements reports one sentence, not
 *    N+1, and no later refine reads it.
 * 2. **The length checks' own `when`** (`_whenHasLength`): a length check runs
 *    whenever the input HAS a `.length`, even after the type check aborted. So
 *    a STRING where the array belongs fails the type check AND is measured
 *    against the array's bounds, with the string origin (`Too big: expected
 *    string to have <=8 characters`). Only a string can reach that arm from
 *    JSON (an object carrying a `length` key would too — no corpus row poses
 *    one, so it is not modelled).
 *
 * Within bounds, the elements parse in order and the `min` check follows them.
 */
function parseSizedArray<T>(
  input: unknown,
  parseElement: (v: unknown) => Res<T>,
  min: LengthCheck | undefined,
  max: LengthCheck,
): Res<T[]> {
  if (!Array.isArray(input)) {
    const issues: Issue[] = [hardIssue(invalidType('array', input))];
    if (typeof input === 'string') {
      if (min !== undefined && !zodLenMinOk(input, min.n)) {
        issues.push(checkIssue(min.message ?? `Too small: expected string to have >=${min.n} characters`));
      }
      if (!zodLenMaxOk(input, max.n)) {
        issues.push(checkIssue(max.message ?? `Too big: expected string to have <=${max.n} characters`));
      }
    }
    return { value: undefined, issues };
  }
  if (input.length > max.n) {
    return resHard(max.message ?? `Too big: expected array to have <=${max.n} items`);
  }

  const list: T[] = [];
  const issues: Issue[] = [];
  input.forEach((item, i) => {
    const r = parseElement(item);
    issues.push(...prefix(String(i), r.issues));
    if (r.value !== undefined) list.push(r.value);
  });
  if (min !== undefined && input.length < min.n) {
    issues.push(checkIssue(min.message ?? `Too small: expected array to have >=${min.n} items`));
  }
  return { value: aborted(issues) ? undefined : list, issues };
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
 * v4 `InventoryRefSchema` (`e1872c94f`) — `z.strictObject({ $inventory:
 * z.string().min(1).max(MAX_INVENTORY_REF_LENGTH + 120).refine(…), fallback:
 * <the $state fallback union> })`. The length checks and the refine are all
 * CHECKS, so an empty reference reports Zod's `Too small` AND the refine's
 * sentence (the refine runs past a continuable length issue).
 */
function parseInventoryRef(input: unknown): Res<InventoryRef> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const ref = parseString(input['$inventory'], 1, MAX_INVENTORY_REF_LENGTH + 120, false);
  const refIssues = [...ref.issues];
  if (ref.value !== undefined && splitInventoryReference(ref.value)?.field !== 'metadata') {
    refIssues.push(checkIssue(INVENTORY_REF_MESSAGE));
  }
  issues.push(...prefix('$inventory', refIssues));

  const fallback = parseStateFallback(input['fallback']);
  issues.push(...prefix('fallback', fallback.issues));

  issues.push(...unrecognizedKeys(input, ['$inventory', 'fallback']));

  const value =
    ref.value !== undefined && fallback.value !== undefined && !aborted(issues)
      ? { $inventory: ref.value, fallback: fallback.value }
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
  const inv = parseInventoryRef(input);
  return union<NumberOrParamRef>([n, p, st, inv]);
}

/**
 * v4 `AnyOperandSchema` — number | string | boolean | ParamRef | StateRef |
 * InventoryRef, in that order.
 */
function parseAnyOperand(input: unknown): Res<AnyOperand> {
  const n = parseFiniteNumber(input);
  const s = parseString(input, undefined, undefined, false);
  const b = parseBool(input);
  const p = parseParamRef(input);
  const st = parseStateRef(input);
  const inv = parseInventoryRef(input);
  return union<AnyOperand>([n, s, b, p, st, inv]);
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
  const inv = parseInventoryRef(input);
  return union<StringOperand>([s, p, st, inv]);
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
  // `.refine(hasComparator, AT_LEAST_ONE)` — runs only when nothing aborted, over
  // the PARSED value: a stray containment key was stripped as unrecognized and
  // cannot satisfy it (under Zod 4.5.4 it no longer aborts the object either).
  if (!NUMERIC_COMPARATOR_KEYS.some((k) => hasKey(input, k))) issues.push(checkIssue(AT_LEAST_ONE));
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
 * The key schemas of the format's `z.record`s, as REASONS: `null` when the key
 * passes, else the key schema's own issue messages — what v4 `a53024d3c`'s
 * `invalid_key` arm of {@link flattenIssues} surfaces in place of Zod's bare
 * "Invalid key in record".
 */
type KeyReason = (key: string) => string[] | null;

/** v4 `IdentifierSchema` as a record KEY — `params` and `parameters`. */
const identifierKey: KeyReason = (key) => (IDENTIFIER_PATTERN.test(key) ? null : [IDENTIFIER_MESSAGE]);

/**
 * v4 `MetadataKeySchema` — `z.string().min(1)`, Zod's own sentence. Metadata
 * keys are the USER's vocabulary (`HOUSE` and `Clearance Level` are ordinary
 * keys), so only emptiness is refused.
 */
const metadataKey: KeyReason = (key) =>
  zodLenMinOk(key, 1) ? null : ['Too small: expected string to have >=1 characters'];

/**
 * v4 `ProgressKeySchema` — delegated to `parseProgressKey`, the one parser
 * that owns the `"<id>.<field>"` shape, so this schema and the Workbench's
 * condition validator cannot drift apart; its per-mistake sentence is the
 * reason.
 */
const progressKey: KeyReason = (key) => {
  const parsed = parseProgressKey(key);
  return parsed.ok ? null : [parsed.reason];
};

/** v4 `InventoryKeySchema` — `z.string().min(1).max(MAX_INVENTORY_REF_LENGTH)`. */
const inventoryKey: KeyReason = (key) => {
  const messages = stringLengthIssues(key, 1, MAX_INVENTORY_REF_LENGTH).map((i) => i.message);
  return messages.length > 0 ? messages : null;
};

/** v4 `AggregateKeySchema` — `z.string().superRefine(…parseAggregateKey…)`. */
const aggregateKey: KeyReason = (key) => {
  // The ONE grammar (R-2): `inventory-aggregate.ts` owns `<scope>.<field>` and
  // its three refusal sentences, for the schema, the validator and the evaluator.
  const parsed = parseAggregateKey(key);
  return parsed.ok ? null : [parsed.reason];
};

/**
 * v4 `z.record(keySchema, …)` over a value already known to be a plain object:
 * a key the key schema refuses is an `invalid_key` issue — aborting, unlike the
 * same rule used as a field check. Zod's outer message is always "Invalid key
 * in record"; the key schema's own messages ride as the issue's `inner`, which
 * {@link flattenIssues} surfaces instead (v4 `a53024d3c`).
 */
function parseRecord<T>(
  obj: Record<string, unknown>,
  keyReason: KeyReason,
  parseValue: (v: unknown) => Res<T>,
): Res<Record<string, T>> {
  const issues: Issue[] = [];
  const out: Record<string, T> = {};
  for (const key of Object.keys(obj)) {
    const reason = keyReason(key);
    if (reason !== null) {
      const issue: TaggedIssue = { ...at(hardIssue('Invalid key in record'), key), inner: reason };
      issues.push(issue);
      continue;
    }
    const r = parseValue(obj[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  return { value: aborted(issues) ? undefined : out, issues };
}

/**
 * One optional `z.record(…)` subject of a strict object: parse it into `out`
 * when present, reporting a non-object as Zod names a `z.record` — "record",
 * not "object" (the captured rows pin it at every site).
 */
function parseRecordSubject<T>(
  input: Record<string, unknown>,
  subject: string,
  keyReason: KeyReason,
  parseValue: (v: unknown) => Res<T>,
  out: Record<string, unknown>,
  issues: Issue[],
): void {
  if (!hasKey(input, subject)) return;
  const raw = input[subject];
  if (isPlainObject(raw)) {
    const r = parseRecord(raw, keyReason, parseValue);
    issues.push(...prefix(subject, r.issues));
    if (r.value !== undefined) out[subject] = r.value;
  } else {
    issues.push(...prefix(subject, [hardIssue(invalidType('record', raw))]));
  }
}

/** True when a record subject was parsed and holds at least one key. */
function nonEmpty(record: Record<string, unknown> | undefined): boolean {
  return record !== undefined && Object.keys(record).length > 0;
}

/** v4's inventory-state refine message, shared by the `when` and gate forms. */
const INVENTORY_STATE_AT_LEAST_ONE =
  'must test something: `eq` or `neq` against equipped, carried, reachable or absent';

/**
 * v4 `InventoryStateComparatorSchema` (`gate: false` — each operand a state or
 * a `$param`) and `GateInventoryStateComparatorSchema` (`gate: true` — literal
 * states only). A strict object with an object-level refine.
 */
function parseInventoryStateComparator(
  input: unknown,
  gate: boolean,
): Res<InventoryStateComparator> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: InventoryStateComparator = {};
  for (const key of ['eq', 'neq'] as const) {
    if (!hasKey(input, key)) continue;
    const raw = input[key];
    const state = parseEnum<InventoryStateValue>(raw, INVENTORY_STATE_VALUES);
    const r: Res<InventoryStateValue | ParamRef> = gate
      ? state
      : union<InventoryStateValue | ParamRef>([state, parseParamRef(raw)]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  issues.push(...unrecognizedKeys(input, ['eq', 'neq']));

  if (aborted(issues)) return { value: undefined, issues };
  if (out.eq === undefined && out.neq === undefined) issues.push(checkIssue(INVENTORY_STATE_AT_LEAST_ONE));
  return { value: out, issues };
}

/**
 * v4 `GateInventoryCountComparatorSchema` — the six numeric keys, each a bare
 * finite number (a gate has no parameters), with `AT_LEAST_ONE`.
 */
function parseGateInventoryCountComparator(input: unknown): Res<GateInventoryCountComparator> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: GateInventoryCountComparator = {};
  for (const key of NUMERIC_COMPARATOR_KEYS) {
    if (!hasKey(input, key)) continue;
    const r = parseFiniteNumber(input[key]);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  issues.push(...unrecognizedKeys(input, NUMERIC_COMPARATOR_KEYS));

  if (aborted(issues)) return { value: undefined, issues };
  if (!NUMERIC_COMPARATOR_KEYS.some((k) => out[k] !== undefined)) issues.push(checkIssue(AT_LEAST_ONE));
  return { value: out, issues };
}

/**
 * v4 `ToolGateGroupSchema` (`a53024d3c` — the old one-object `ToolGateSchema`
 * renamed) — `z.strictObject({ metadata, progress, inventory, inventoryCount,
 * inventoryAggregate }).refine(…)`, every subject optional.
 *
 * `0587d1e96` moved the "must test at least one …" refine from the metadata
 * RECORD to the gate OBJECT, so the issue PATH is the gate rather than
 * `gate.metadata`, and a gate with NO subject at all reads as "tests nothing".
 * Being an object-level refine it is skipped once the object has aborted.
 */
function parseToolGateGroup(input: unknown): Res<ToolGateGroup> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: ToolGateGroup = {};
  const bag = out as Record<string, unknown>;

  parseRecordSubject(input, 'metadata', metadataKey, parseGateComparator, bag, issues);
  parseRecordSubject(input, 'progress', progressKey, parseGateComparator, bag, issues);
  parseRecordSubject(
    input,
    'inventory',
    inventoryKey,
    (v) => parseInventoryStateComparator(v, true),
    bag,
    issues,
  );
  parseRecordSubject(input, 'inventoryCount', inventoryKey, parseGateInventoryCountComparator, bag, issues);
  parseRecordSubject(input, 'inventoryAggregate', aggregateKey, parseGateComparator, bag, issues);

  issues.push(...unrecognizedKeys(input, GATE_GROUP_SUBJECTS));

  if (aborted(issues)) return { value: undefined, issues };

  const tested = GATE_GROUP_SUBJECTS.reduce((n, subject) => n + Object.keys(bag[subject] ?? {}).length, 0);
  if (tested === 0) {
    issues.push(
      checkIssue('must test at least one metadata key, progress field, inventory object, or inventory aggregate'),
    );
  }

  return { value: out, issues };
}

/** A gate group's subjects, in v4's shape order. */
const GATE_GROUP_SUBJECTS = [
  'metadata',
  'progress',
  'inventory',
  'inventoryCount',
  'inventoryAggregate',
] as const;

/** v4 `ToolGateSchema` — `z.union([ToolGateGroupSchema, <the any-of array>])`. */
function parseToolGate(input: unknown): Res<ToolGate> {
  return union<ToolGate>([parseToolGateGroup(input), parseGroupArray(input, parseToolGateGroup)]);
}

/**
 * v4's any-of array branch — `z.array(group).min(1).max(MAX_WHEN_GROUPS,
 * \`at most ${MAX_WHEN_GROUPS} groups\`)` — shared by the gate, the outcome
 * `when` and the effect `when` (v4 `a53024d3c`).
 *
 * Zod 4.6.7's array semantics, measured at the pin: an oversized array fails
 * with ONLY its `too_big` — the AUTHOR's message, `at most 8 groups` — before
 * an element is parsed, and that issue ABORTS; `.min(1)` is a continuable
 * check with Zod's own sentence; elements are prefixed by index. A non-array
 * refusal is tagged for v4's `isNotAnArray` filter, and an input with a
 * `.length` also gets the two length checks past it (Zod's own `.min`
 * sentence first, then the authored `.max`). The server twin is
 * `custom_tool_types.rs::parse_group_array`.
 */
function parseGroupArray<T>(input: unknown, parseGroup: (v: unknown) => Res<T>): Res<T[]> {
  const maxMessage = `at most ${MAX_WHEN_GROUPS} groups`;
  if (Array.isArray(input)) {
    if (input.length > MAX_WHEN_GROUPS) return resHard(maxMessage);
    const groups: T[] = [];
    const issues: Issue[] = [];
    input.forEach((item, g) => {
      const r = parseGroup(item);
      issues.push(...prefix(String(g), r.issues));
      if (r.value !== undefined) groups.push(r.value);
    });
    if (input.length === 0) issues.push(checkIssue('Too small: expected array to have >=1 items'));
    return { value: aborted(issues) ? undefined : groups, issues };
  }

  const notArray: TaggedIssue = { ...hardIssue(invalidType('array', input)), notArray: true };
  const issues: Issue[] = [notArray];
  if (typeof input === 'string') {
    if (!zodLenMinOk(input, 1)) issues.push(checkIssue('Too small: expected string to have >=1 characters'));
    if (!zodLenMaxOk(input, MAX_WHEN_GROUPS)) issues.push(checkIssue(maxMessage));
  } else if (isPlainObject(input) && typeof input['length'] === 'number') {
    // Zod's length checks read ANY `.length` (origin `unknown`); only the
    // `.min` keeps Zod's sentence, the `.max` is the author's.
    const n = input['length'];
    if (n < 1) issues.push(checkIssue('Too small: expected unknown to be >=1'));
    if (n > MAX_WHEN_GROUPS) issues.push(checkIssue(maxMessage));
  }
  return { value: undefined, issues };
}

function parseParameter(input: unknown): Res<CustomToolParameter> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];

  const ty = parseEnum<ParameterType>(input['type'], ['number', 'integer', 'string', 'boolean']);
  issues.push(...prefix('type', ty.issues));

  // `default: z.union([number, string, boolean, StateRefSchema,
  // InventoryRefSchema])` — no `.finite()` on the number arm here (unlike a
  // `$state` fallback).
  let defaultValue: number | string | boolean | StateRef | InventoryRef | undefined;
  const rawDefault = input['default'];
  if (
    typeof rawDefault === 'number' ||
    typeof rawDefault === 'string' ||
    typeof rawDefault === 'boolean'
  ) {
    defaultValue = rawDefault;
  } else {
    const branches: Res<number | string | boolean | StateRef | InventoryRef>[] = [
      resHard(invalidType('number', rawDefault)),
      resHard(invalidType('string', rawDefault)),
      resHard(invalidType('boolean', rawDefault)),
      parseStateRef(rawDefault),
      parseInventoryRef(rawDefault),
    ];
    const r = union<number | string | boolean | StateRef | InventoryRef>(branches);
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
  const effectiveDefault =
    isStateRef(defaultValue) || isInventoryRef(defaultValue) ? defaultValue.fallback : defaultValue;
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
  const inv = parseInventoryRef(input);
  return union<Roll>([d, r, inv]);
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

/**
 * v4 `WHEN_SUBJECTS_SHAPE`, parsed in its declaration order — the bare six
 * comparators on the value, then `roll`, `params`, `metadata`, `progress`,
 * `llm`, `inventory`, `inventoryCount`, `inventoryAggregate` — which is the
 * order the parsed object serializes in. One shape, read by the outcome row's
 * `when` and the effect's, so the two can never disagree about a subject.
 */
function parseWhenSubjects(input: Record<string, unknown>, out: WhenObject, issues: Issue[]): void {
  const bag = out as Record<string, unknown>;
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
  parseRecordSubject(input, 'params', identifierKey, parseParamComparator, bag, issues);
  // Metadata keys are `z.string().min(1)` — any non-empty string, unlike
  // `params`' identifier grammar. Progress keys are the FORMAT's vocabulary,
  // so their shape IS checkable at load time.
  parseRecordSubject(input, 'metadata', metadataKey, parseParamComparator, bag, issues);
  parseRecordSubject(input, 'progress', progressKey, parseParamComparator, bag, issues);
  if (hasKey(input, 'llm')) {
    const r = parseLlmComparator(input['llm']);
    issues.push(...prefix('llm', r.issues));
    if (r.value !== undefined) out.llm = r.value;
  }
  parseRecordSubject(
    input,
    'inventory',
    inventoryKey,
    (v) => parseInventoryStateComparator(v, false),
    bag,
    issues,
  );
  parseRecordSubject(input, 'inventoryCount', inventoryKey, parseNumericComparator, bag, issues);
  parseRecordSubject(input, 'inventoryAggregate', aggregateKey, parseParamComparator, bag, issues);
}

/** The keys {@link parseWhenSubjects} reads, in v4's shape order. */
const WHEN_SUBJECT_KEYS: readonly string[] = [
  ...NUMERIC_COMPARATOR_KEYS,
  'roll',
  'params',
  'metadata',
  'progress',
  'llm',
  'inventory',
  'inventoryCount',
  'inventoryAggregate',
];

/**
 * v4 `testsSomething` — true when a `when` object tests at least one of the
 * shared subjects. `hasComparator` reads the PARSED value, where a strict
 * object has stripped every unrecognized key — only the six comparators this
 * level declares can satisfy it (a stray `contains` is an unrecognized key).
 */
function testsSomething(when: WhenObject): boolean {
  return (
    nonEmpty(when.inventory) ||
    nonEmpty(when.inventoryAggregate) ||
    nonEmpty(when.inventoryCount) ||
    NUMERIC_COMPARATOR_KEYS.some((k) => when[k] !== undefined) ||
    when.roll !== undefined ||
    when.llm !== undefined ||
    nonEmpty(when.params) ||
    nonEmpty(when.metadata) ||
    nonEmpty(when.progress)
  );
}

/** v4 `WhenObjectSchema` — `z.strictObject(WHEN_SUBJECTS_SHAPE).refine(testsSomething, …)`. */
function parseWhenObject(input: unknown): Res<WhenObject> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: WhenObject = {};
  parseWhenSubjects(input, out, issues);
  issues.push(...unrecognizedKeys(input, WHEN_SUBJECT_KEYS));

  if (aborted(issues)) return { value: undefined, issues };
  if (!testsSomething(out)) {
    issues.push(
      checkIssue(
        'must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, `metadata`, `progress`, `inventory`, `inventoryCount`, or `inventoryAggregate`',
      ),
    );
  }
  return { value: out, issues };
}

/**
 * v4 `EffectWhenSchema` — the outcome-row comparator language plus the
 * `outcome` subject, appended LAST. A separate strict object, not a widened
 * {@link parseWhenObject}, exactly as v4 keeps them apart.
 *
 * The nested `outcome` object carries its own refine ("must test something:
 * `eq` or `neq` …"), and the outer refine's sentence names `outcome` where the
 * outcome-row one does not.
 */
function parseEffectWhen(input: unknown): Res<EffectWhen> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: EffectWhen = {};
  parseWhenSubjects(input, out, issues);
  if (hasKey(input, 'outcome')) {
    const r = parseOutcomeSubject(input['outcome']);
    issues.push(...prefix('outcome', r.issues));
    if (r.value !== undefined) out.outcome = r.value;
  }
  issues.push(...unrecognizedKeys(input, [...WHEN_SUBJECT_KEYS, 'outcome']));

  if (aborted(issues)) return { value: undefined, issues };
  if (!testsSomething(out) && out.outcome === undefined) {
    issues.push(
      checkIssue(
        'must test something: a comparator on the value, `roll`, `llm`, `outcome`, a non-empty `params`, `metadata`, `progress`, `inventory`, `inventoryCount`, or `inventoryAggregate`',
      ),
    );
  }
  return { value: out, issues };
}

/** v4 `EffectConditionSchema` — one group, or an any-of array of groups. */
function parseEffectCondition(input: unknown): Res<EffectCondition> {
  return union<EffectCondition>([parseEffectWhen(input), parseGroupArray(input, parseEffectWhen)]);
}

/** The `outcome` subject: `{ eq?, neq? }` over outcome states, one required. */
function parseOutcomeSubject(input: unknown): Res<{ eq?: OutcomeState; neq?: OutcomeState }> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];
  const out: { eq?: OutcomeState; neq?: OutcomeState } = {};
  for (const key of ['eq', 'neq'] as const) {
    if (!hasKey(input, key)) continue;
    const r = parseEnum<OutcomeState>(input[key], ['success', 'partial', 'failure', 'info']);
    issues.push(...prefix(key, r.issues));
    if (r.value !== undefined) out[key] = r.value;
  }
  issues.push(...unrecognizedKeys(input, ['eq', 'neq']));

  if (aborted(issues)) return { value: undefined, issues };
  if (out.eq === undefined && out.neq === undefined) {
    issues.push(checkIssue('must test something: `eq` or `neq` against an outcome state'));
  }
  return { value: out, issues };
}

/**
 * v4 `CustomToolWardrobeGestureSchema` — a strict object; `items` is a sized
 * array of item-reference TEMPLATES ({@link parseSizedArray}), `mode` and
 * `slot` the two enums. The per-action rules are the tool-level refine's
 * ({@link validateWardrobeGesture}), not this parse's.
 */
function parseWardrobeGesture(input: unknown): Res<CustomToolWardrobeGesture> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];

  const action = parseEnum<WardrobeEffectAction>(input['action'], WARDROBE_EFFECT_ACTIONS);
  issues.push(...prefix('action', action.issues));

  let items: string[] | undefined;
  if (hasKey(input, 'items')) {
    const r = parseSizedArray<string>(
      input['items'],
      (item) => parseString(item, 1, MAX_WARDROBE_ITEM_REF_LENGTH, false),
      { n: 1 },
      { n: MAX_WARDROBE_EFFECT_ITEMS },
    );
    issues.push(...prefix('items', r.issues));
    items = r.value;
  }

  let mode: WardrobeEffectMode | undefined;
  if (hasKey(input, 'mode')) {
    const r = parseEnum<WardrobeEffectMode>(input['mode'], WARDROBE_EFFECT_MODES);
    issues.push(...prefix('mode', r.issues));
    mode = r.value;
  }

  let slot: WardrobeItemType | undefined;
  if (hasKey(input, 'slot')) {
    const r = parseEnum<WardrobeItemType>(input['slot'], WARDROBE_SLOT_TYPES);
    issues.push(...prefix('slot', r.issues));
    slot = r.value;
  }

  issues.push(...unrecognizedKeys(input, ['action', 'items', 'mode', 'slot']));

  if (action.value === undefined || aborted(issues)) return { value: undefined, issues };

  // Schema order, absent optionals omitted — Zod emits schema order, never the
  // author's (the `effects-wardrobe-key-order` row).
  const gesture = { action: action.value } as CustomToolWardrobeGesture;
  if (items !== undefined) gesture.items = items;
  if (mode !== undefined) gesture.mode = mode;
  if (slot !== undefined) gesture.slot = slot;
  return { value: gesture, issues };
}

/**
 * v4 `CustomToolInventoryGestureSchema` (`e1872c94f`) — a strict object;
 * `items` is a REQUIRED sized array of object-reference TEMPLATES, `equip` an
 * optional boolean. The per-action rules are the tool-level refine's
 * ({@link inventoryGestureProblems}), not this parse's.
 */
function parseInventoryGesture(input: unknown): Res<CustomToolInventoryGesture> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];

  const action = parseEnum<InventoryEffectAction>(input['action'], INVENTORY_EFFECT_ACTIONS);
  issues.push(...prefix('action', action.issues));

  const items = parseSizedArray<string>(
    input['items'],
    (item) => parseString(item, 1, MAX_INVENTORY_REF_LENGTH, false),
    { n: 1 },
    { n: MAX_INVENTORY_EFFECT_ITEMS },
  );
  issues.push(...prefix('items', items.issues));

  let equip: boolean | undefined;
  if (hasKey(input, 'equip')) {
    const r = parseBool(input['equip']);
    issues.push(...prefix('equip', r.issues));
    equip = r.value;
  }

  issues.push(...unrecognizedKeys(input, ['action', 'items', 'equip']));

  if (action.value === undefined || items.value === undefined || aborted(issues)) {
    return { value: undefined, issues };
  }

  const gesture: CustomToolInventoryGesture = { action: action.value, items: items.value };
  if (equip !== undefined) gesture.equip = equip;
  return { value: gesture, issues };
}

/**
 * v4 `CustomToolEffectSchema` (`f0db8cbb8`, widened by `e1872c94f` /
 * `e4a60ad9e`): a strict object whose `target`, `value`, `wardrobe`,
 * `inventory` and `log` are all OPTIONAL, then a refine that says which of the
 * four kinds it is. The `value` union is number → boolean → string, and the
 * string arm's length rules are CHECKS: an empty string leaves the string
 * branch as the only non-aborted one, so its own "Too small" sentence hoists
 * out of the union rather than a bare "Invalid input".
 *
 * The refine runs only on a non-aborted object (continuable issues — a length
 * check, a stray key — do not stop it), and its three sentences are the
 * author's: both halves → not both; neither → needs one; one write half → the
 * missing half's path.
 */
function parseCustomToolEffect(input: unknown): Res<CustomToolEffect> {
  if (!isPlainObject(input)) return resHard(invalidType('object', input));

  const issues: Issue[] = [];

  let when: EffectCondition | undefined;
  if (hasKey(input, 'when')) {
    const r = parseEffectCondition(input['when']);
    issues.push(...prefix('when', r.issues));
    when = r.value;
  }

  let target: string | undefined;
  if (hasKey(input, 'target')) {
    const r = parseString(input['target'], 1, MAX_EFFECT_TARGET_LENGTH, false);
    issues.push(...prefix('target', r.issues));
    target = r.value;
  }

  let value: number | boolean | string | undefined;
  if (hasKey(input, 'value')) {
    const rawValue = input['value'];
    const r = union<number | boolean | string>([
      parseFiniteNumber(rawValue),
      parseBool(rawValue),
      parseString(rawValue, 1, MAX_EFFECT_EXPRESSION_LENGTH, false),
    ]);
    issues.push(...prefix('value', r.issues));
    value = r.value;
  }

  let wardrobe: CustomToolWardrobeGesture | undefined;
  if (hasKey(input, 'wardrobe')) {
    const r = parseWardrobeGesture(input['wardrobe']);
    issues.push(...prefix('wardrobe', r.issues));
    wardrobe = r.value;
  }

  let inventory: CustomToolInventoryGesture | undefined;
  if (hasKey(input, 'inventory')) {
    const r = parseInventoryGesture(input['inventory']);
    issues.push(...prefix('inventory', r.issues));
    inventory = r.value;
  }

  let log: string | undefined;
  if (hasKey(input, 'log')) {
    const r = parseString(input['log'], 1, MAX_MESSAGE_LENGTH, false);
    issues.push(...prefix('log', r.issues));
    log = r.value;
  }

  issues.push(...unrecognizedKeys(input, ['when', 'target', 'value', 'wardrobe', 'inventory', 'log']));

  if (aborted(issues)) return { value: undefined, issues };

  // ---- the effect's superRefine (v4 `custom-tool.types.ts:984-1010`) ------
  const isWrite = target !== undefined || value !== undefined;
  const kinds = [isWrite, wardrobe !== undefined, inventory !== undefined, log !== undefined].filter(
    Boolean,
  ).length;
  if (kinds > 1) {
    issues.push(
      checkIssue(
        'an effect is exactly one of a write (target + value), a wardrobe gesture, an inventory gesture, or a log line',
      ),
    );
  } else if (kinds === 0) {
    issues.push(checkIssue('an effect needs target + value, wardrobe, inventory, or log'));
  } else if (isWrite && (target === undefined || value === undefined)) {
    issues.push(
      checkIssue('a write needs both target and value', [target === undefined ? 'target' : 'value']),
    );
  }

  // Declaration order, absent optionals omitted — the corpus byte-compares
  // `JSON.stringify` of the parsed document against Zod's own output.
  const effect = {} as CustomToolEffect;
  if (when !== undefined) effect.when = when;
  if (target !== undefined) effect.target = target;
  if (value !== undefined) effect.value = value;
  if (wardrobe !== undefined) effect.wardrobe = wardrobe;
  if (inventory !== undefined) effect.inventory = inventory;
  if (log !== undefined) effect.log = log;
  return { value: effect, issues };
}

/** v4 `WhenSchema` — `z.union([z.literal(true), WhenObjectSchema, <the any-of array>])`. */
function parseWhen(input: unknown): Res<When> {
  // `z.literal(true)`.
  const lit: Res<When> =
    input === true
      ? resOk<When>(true)
      : { value: undefined, issues: [hardIssue('Invalid input: expected true')] };
  const o = parseWhenObject(input);
  const anyOf = parseGroupArray(input, parseWhenObject);
  return union<When>([lit, o, anyOf]);
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

  // Bug 257 (`322a5488e`): `z.string().min(1).max(MAX_MESSAGE_LENGTH)
  // .optional()`, LAST in the shape — so its issues follow `state`'s, and the
  // key serializes after it.
  let brief: string | undefined;
  if (hasKey(input, 'brief')) {
    const r = parseString(input['brief'], 1, MAX_MESSAGE_LENGTH, false);
    issues.push(...prefix('brief', r.issues));
    brief = r.value;
  }

  issues.push(...unrecognizedKeys(input, ['when', 'message', 'state', 'brief']));

  if (
    when.value !== undefined &&
    message.value !== undefined &&
    state.value !== undefined &&
    !aborted(issues)
  ) {
    const outcome: CustomToolOutcome = { when: when.value, message: message.value, state: state.value };
    if (brief !== undefined) outcome.brief = brief;
    return { value: outcome, issues };
  }
  return { value: undefined, issues };
}

// -------------------------------------------------------------- the top level

/** A successful parse, or the rejection list in Zod's order. */
export type SafeParseResult =
  { success: true; data: QtapCustomTool } | { success: false; issues: Issue[] };

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

  let chipLabel: string | undefined;
  if (hasKey(obj, 'chipLabel')) {
    const r = parseString(obj['chipLabel'], 1, MAX_CHIP_LABEL_LENGTH, false);
    issues.push(...prefix('chipLabel', r.issues));
    chipLabel = r.value;
  }

  // v4 `362426c8f`: after `chipLabel`, before `description` — an issue-order
  // contract (the `shape-order-chip-triggers-description` row), not just a key
  // order.
  let triggers: string[] | undefined;
  if (hasKey(obj, 'triggers')) {
    const r = parseSizedArray<string>(
      obj['triggers'],
      (phrase) => parseString(phrase, MIN_TRIGGER_PHRASE_LENGTH, MAX_TRIGGER_PHRASE_LENGTH, false),
      { n: 1 },
      { n: MAX_TRIGGERS },
    );
    issues.push(...prefix('triggers', r.issues));
    triggers = r.value;
  }

  const description = parseString(obj['description'], 1, MAX_DESCRIPTION_LENGTH, false);
  issues.push(...prefix('description', description.issues));

  const optBool = (key: 'disabled' | 'revealOdds' | 'announceAsCharacter'): boolean | undefined => {
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

  // Between `defaultVisibility` and `parameters` in the schema's declaration
  // order (v4 `a05c6f755`), so its issue is reported — and the key serialized —
  // there. `optional()`, not `nullable()`: a `null` is refused.
  const announceAsCharacter = optBool('announceAsCharacter');

  let parameters: Record<string, CustomToolParameter> | undefined;
  if (hasKey(obj, 'parameters')) {
    const raw2 = obj['parameters'];
    if (isPlainObject(raw2)) {
      const r = parseRecord(raw2, identifierKey, parseParameter);
      const paramIssues = [...r.issues];
      // `.refine(… <= MAX_PARAMETERS)` — a check on the record itself, so it is
      // skipped when an entry already aborted.
      if (!aborted(paramIssues) && Object.keys(raw2).length > MAX_PARAMETERS) {
        paramIssues.push(checkIssue(`at most ${MAX_PARAMETERS} parameters`));
      }
      issues.push(...prefix('parameters', paramIssues));
      parameters = r.value;
    } else {
      issues.push(...prefix('parameters', [hardIssue(invalidType('record', raw2))]));
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

  // `.max(MAX_EFFECTS, …)` and the outcomes' `.min(1, …)` / `.max(MAX_OUTCOMES,
  // …)` carry authored messages; {@link parseSizedArray} applies Zod 4.6.7's
  // two length rules to them (an oversized list reports ONLY its cap, before
  // any element; a string is measured against the bounds too).
  let effects: CustomToolEffect[] | undefined;
  if (hasKey(obj, 'effects')) {
    const r = parseSizedArray(obj['effects'], parseCustomToolEffect, undefined, {
      n: MAX_EFFECTS,
      message: `at most ${MAX_EFFECTS} effects`,
    });
    issues.push(...prefix('effects', r.issues));
    effects = r.value;
  }

  const outcomesRes = parseSizedArray(
    obj['outcomes'],
    parseOutcome,
    { n: 1, message: 'at least one outcome is required' },
    { n: MAX_OUTCOMES, message: `at most ${MAX_OUTCOMES} outcomes` },
  );
  issues.push(...prefix('outcomes', outcomesRes.issues));
  const outcomes = outcomesRes.value;

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
  if (chipLabel !== undefined) tool.chipLabel = chipLabel;
  if (triggers !== undefined) tool.triggers = triggers;
  tool.description = description.value;
  if (disabled !== undefined) tool.disabled = disabled;
  if (availableWhen !== undefined) tool.availableWhen = availableWhen;
  if (withheldWhen !== undefined) tool.withheldWhen = withheldWhen;
  if (revealOdds !== undefined) tool.revealOdds = revealOdds;
  if (defaultVisibility !== undefined) tool.defaultVisibility = defaultVisibility;
  if (announceAsCharacter !== undefined) tool.announceAsCharacter = announceAsCharacter;
  if (parameters !== undefined) tool.parameters = parameters;
  if (roll !== undefined) tool.roll = roll;
  if (llm !== undefined) tool.llm = llm;
  if (effects !== undefined) tool.effects = effects;
  tool.outcomes = outcomes;

  // ---- superRefine (v4 `:374-377`), in source order -----------------------
  validateOutcomeOrdering(tool.outcomes, issues);
  validateReferences(tool, issues);
  validateOutcomeBriefs(tool, issues);
  validateGates(tool, issues);
  validateEffects(tool, issues);
  validateTriggers(tool, issues);

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

  // A gate is answered at roster time, before any parameter exists, so a
  // templated key would be its literal text for ever — and resolve to
  // `absent` for ever. Outcome and effect `when` keys stay templates.
  for (const clause of ['availableWhen', 'withheldWhen'] as const) {
    const gate = tool[clause];
    if (!gate) continue;
    gateGroups(gate).forEach((group, g) => {
      const base: string[] = Array.isArray(gate) ? [clause, String(g)] : [clause];
      for (const subject of ['inventory', 'inventoryCount'] as const) {
        for (const key of Object.keys(group[subject] ?? {})) {
          if (key.includes('{{')) {
            issues.push(
              checkIssue(
                `a gate's ${subject} key must be a literal object reference; "${key}" carries a placeholder, and a gate has no parameters to fill it`,
                [...base, subject, key],
              ),
            );
          }
        }
      }
    });
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
    const when = outcome.when;
    whenGroups(when).forEach((group, g) => {
      // The group index rides ONLY when the clause IS an array.
      const path = Array.isArray(when) ? ['outcomes', String(i), 'when', String(g)] : ['outcomes', String(i), 'when'];
      validateWhenSubjects(declared, group, Boolean(tool.llm), issues, path);
    });
  });
}

/**
 * Rule: `{{outcome.*}}` and `brief` keep to the run order (v4
 * `validateOutcomeBriefs`, `322a5488e` — bug 257).
 *
 * When no row tests the consult, the table is dealt first and the prompt may
 * quote the winning row. When some row does, the table needs the answer, so the
 * prompt cannot read the verdict — a reference there would be a hole for ever,
 * and it is rejected at load. A `brief` is rendered before the consult too, so
 * it may quote neither `{{llm}}` nor `{{outcome.*}}`.
 */
function validateOutcomeBriefs(
  tool: { llm?: CustomToolLlm; outcomes: CustomToolOutcome[] },
  issues: Issue[],
): void {
  if (tool.llm && outcomeTableTestsLlm(tool.outcomes)) {
    for (const { key, ref } of scanPlaceholdersInText(tool.llm.prompt)) {
      if (ref.kind === 'outcome') {
        issues.push(
          checkIssue(
            `the consult prompt references {{${key}}}, but an outcome tests the consult's answer, so the table ` +
              'cannot be dealt before the consult. Drop the `llm` test, or take the verdict out of the prompt',
            ['llm', 'prompt'],
          ),
        );
      }
    }
  }

  tool.outcomes.forEach((outcome, i) => {
    if (outcome.brief === undefined) return;
    const path = ['outcomes', String(i), 'brief'];
    if (!tool.llm) {
      issues.push(checkIssue('has a `brief`, but the tool declares no `llm` block to read it', path));
    }
    for (const { key, ref } of scanPlaceholdersInText(outcome.brief)) {
      if (ref.kind === 'llm' || ref.kind === 'outcome') {
        issues.push(checkIssue(`a brief is written before the consult runs and cannot quote {{${key}}}`, path));
      }
    }
  });
}

/**
 * The shared walk over one `when` object's subjects — outcome rows and effect
 * conditions carry the same comparator language, so they are checked by the
 * same code rather than two copies that drift.
 */
function validateWhenSubjects(
  declared: Record<string, CustomToolParameter>,
  when: WhenObject | EffectWhen,
  hasLlmBlock: boolean,
  issues: Issue[],
  path: string[],
): void {
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
    validateComparator(
      declared,
      comparator,
      valueTypeOf(target.type),
      `parameter "${name}"`,
      issues,
      [...path, 'params', name],
    );
  }

  // An `llm` test on a tool with no `llm` block could never fire — there is
  // no consult whose answer it might match. That is a typo, not an intent,
  // and left alone it reads as a dead branch in the outcome table.
  if (when.llm !== undefined) {
    if (!hasLlmBlock) {
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

  // An aggregate's key is the format's own (checked by its schema); whether a
  // scope holds anything, and what types its values carry, are facts about a
  // character — so, as with `metadata`, only the operands are checkable here.
  for (const [key, comparator] of Object.entries(when.inventoryAggregate ?? {})) {
    validateMetadataOperands(declared, comparator, issues, [...path, 'inventoryAggregate', key]);
  }

  // `inventory` and `inventoryCount` keys are TEMPLATES: their placeholders
  // must resolve the way an item reference's do. The state operand is a
  // literal state or a `$param` to a string parameter; the count's operands
  // are numbers.
  for (const [key, comparator] of Object.entries(when.inventory ?? {})) {
    validateTemplatePlaceholders(declared, hasLlmBlock, key, issues, [...path, 'inventory', key]);
    for (const op of ['eq', 'neq'] as const) {
      const operand = comparator[op];
      if (operand === undefined || !isParamRef(operand)) continue;
      const target = declared[operand.$param];
      if (!target) {
        issues.push(
          checkIssue(`references undeclared parameter "${operand.$param}"`, [...path, 'inventory', key, op]),
        );
      } else if (valueTypeOf(target.type) !== 'string') {
        issues.push(
          checkIssue(
            `${op} compares an inventory state with parameter "${operand.$param}", which is ${target.type} rather than a string`,
            [...path, 'inventory', key, op],
          ),
        );
      }
    }
  }
  for (const [key, comparator] of Object.entries(when.inventoryCount ?? {})) {
    validateTemplatePlaceholders(declared, hasLlmBlock, key, issues, [...path, 'inventoryCount', key]);
    validateComparator(declared, comparator, 'number', `the equipped count of "${key}"`, issues, [
      ...path,
      'inventoryCount',
      key,
    ]);
  }
}

/**
 * A template's placeholders resolve as an outcome message's must: a
 * `{{params.x}}` names a declared parameter, a `{{llm}}` needs an `llm` block.
 * Used for inventory keys and references and for log lines.
 */
function validateTemplatePlaceholders(
  declared: Record<string, CustomToolParameter>,
  hasLlmBlock: boolean,
  key: string,
  issues: Issue[],
  path: string[],
): void {
  for (const { ref } of scanPlaceholdersInText(key)) {
    if (ref.kind === 'params' && !declared[ref.name]) {
      issues.push(checkIssue(`references undeclared parameter "${ref.name}"`, path));
    } else if (ref.kind === 'llm' && !hasLlmBlock) {
      issues.push(checkIssue('references {{llm}}, but the tool declares no `llm` block', path));
    }
  }
}

/**
 * Rule: every effect must be applicable. The target parses (including the
 * underscore guard), a string `value` parses as an expression, every
 * `params.x` reference names a declared parameter, and an `{{llm}}` reference
 * or an `llm` `when`-subject requires an `llm` block — the same rule outcome
 * rows follow.
 *
 * Parse failure here is the dice-notation doctrine: syntax errors are typos,
 * caught in the Workbench and at discovery, never at the table.
 */
function validateEffects(
  tool: Pick<QtapCustomTool, 'parameters' | 'llm' | 'effects'>,
  issues: Issue[],
): void {
  const declared = tool.parameters ?? {};

  (tool.effects ?? []).forEach((effect, i) => {
    const base = ['effects', String(i)];

    // v4 `f0db8cbb8` moved the `when` subjects FIRST (they were last), so a
    // write with a bad subject and a bad target reports the subject first —
    // the `effects-write-when-order` row.
    if (effect.when !== undefined) {
      const when = effect.when;
      effectWhenGroups(when).forEach((group, g) => {
        const path = Array.isArray(when) ? [...base, 'when', String(g)] : [...base, 'when'];
        validateWhenSubjects(declared, group, Boolean(tool.llm), issues, path);
      });
    }

    if (effect.wardrobe !== undefined) {
      validateWardrobeGesture(declared, Boolean(tool.llm), effect.wardrobe, issues, [
        ...base,
        'wardrobe',
      ]);
      return;
    }

    if (effect.log !== undefined) {
      validateTemplatePlaceholders(declared, Boolean(tool.llm), effect.log, issues, [...base, 'log']);
      return;
    }

    if (effect.inventory !== undefined) {
      for (const problem of inventoryGestureProblems(effect.inventory)) {
        issues.push(checkIssue(problem.message, [...base, 'inventory', problem.field]));
      }
      effect.inventory.items.forEach((item, m) => {
        validateTemplatePlaceholders(declared, Boolean(tool.llm), item, issues, [
          ...base,
          'inventory',
          'items',
          String(m),
        ]);
      });
      return;
    }

    // Shape errors (a write missing its target or value) are the effect
    // schema's own refine; nothing further to check without both halves.
    if (effect.target === undefined || effect.value === undefined) return;

    const target = parseEffectTarget(effect.target);
    if (!target.ok) {
      issues.push(checkIssue(`target ${target.reason}`, [...base, 'target']));
    }

    // A JSON string value is ALWAYS an expression — the quoting trap. The bare
    // prose an author meant as a literal fails to parse right here, loudly.
    if (typeof effect.value === 'string') {
      const parsed = parseExpression(effect.value);
      if (!parsed.ok) {
        issues.push(
          checkIssue(`value is not a valid expression: ${parsed.reason}`, [...base, 'value']),
        );
      } else {
        for (const ref of parsed.expr.refs) {
          if (ref.startsWith('params.')) {
            const name = ref.slice('params.'.length);
            if (!declared[name]) {
              issues.push(
                checkIssue(`value references undeclared parameter "${name}"`, [...base, 'value']),
              );
            }
          } else if (ref === 'llm' && !tool.llm) {
            issues.push(
              checkIssue('value references {{llm}}, but the tool declares no `llm` block', [
                ...base,
                'value',
              ]),
            );
          }
        }
      }
    }
  });
}

/**
 * Rule: a wardrobe gesture carries exactly the fields its action uses
 * ({@link wardrobeGestureProblems}), and every item reference's placeholders
 * resolve the way an outcome message's must — a `{{params.x}}` names a
 * declared parameter, a `{{llm}}` needs an `llm` block. Unknown placeholder
 * families are NOT rejected: the `renderTemplate` doctrine leaves them as
 * written, and the item simply fails to resolve at the table.
 */
function validateWardrobeGesture(
  declared: Record<string, CustomToolParameter>,
  hasLlmBlock: boolean,
  gesture: CustomToolWardrobeGesture,
  issues: Issue[],
  path: string[],
): void {
  for (const problem of wardrobeGestureProblems(gesture)) {
    issues.push(checkIssue(problem.message, [...path, problem.field]));
  }

  (gesture.items ?? []).forEach((item, m) => {
    for (const { ref } of scanPlaceholdersInText(item)) {
      if (ref.kind === 'params' && !declared[ref.name]) {
        issues.push(
          checkIssue(`item references undeclared parameter "${ref.name}"`, [...path, 'items', String(m)]),
        );
      } else if (ref.kind === 'llm' && !hasLlmBlock) {
        issues.push(
          checkIssue('item references {{llm}}, but the tool declares no `llm` block', [
            ...path,
            'items',
            String(m),
          ]),
        );
      }
    }
  });
}

/**
 * Rule (v4 `362426c8f`, FOURTH in the superRefine): a trigger phrase carries no
 * leading or trailing whitespace, and no two phrases are the same once folded
 * (`toLocaleLowerCase` — lower-casing, not case-folding: `Straße` and
 * `STRASSE` are two phrases). The FIRST occurrence wins the fold map, so a
 * third copy names trigger 1.
 */
function validateTriggers(tool: Pick<QtapCustomTool, 'triggers'>, issues: Issue[]): void {
  if (!tool.triggers) return;
  const seen = new Map<string, number>();
  tool.triggers.forEach((phrase, index) => {
    if (phrase !== phrase.trim()) {
      issues.push(
        checkIssue(`trigger ${index + 1} has leading or trailing whitespace`, ['triggers', String(index)]),
      );
    }
    const folded = phrase.toLocaleLowerCase();
    const earlier = seen.get(folded);
    if (earlier !== undefined) {
      issues.push(
        checkIssue(
          `trigger ${index + 1} repeats trigger ${earlier + 1} (phrases match regardless of case)`,
          ['triggers', String(index)],
        ),
      );
    } else {
      seen.set(folded, index);
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

  // A whole-roll `$inventory`: the object's metadata names the dice, and the
  // fallback must be dice notation this build can roll.
  if (isInventoryRef(roll)) {
    if (typeof roll.fallback !== 'string' || parseDiceNotation(roll.fallback) === null) {
      issues.push(
        checkIssue('a roll drawn from $inventory needs a dice-notation fallback, like "1d4"', ['roll', 'fallback']),
      );
    }
    return;
  }

  for (const [field, value] of Object.entries(roll)) {
    if (isStateRef(value) || isInventoryRef(value)) {
      const kind = isStateRef(value) ? '$state' : '$inventory';
      if (typeof value.fallback !== 'number') {
        issues.push(
          checkIssue(
            `roll.${field} uses a ${kind} reference whose fallback is ${typeof value.fallback} rather than a number`,
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
        checkIssue(`roll.${field} references undeclared parameter "${value.$param}"`, [
          'roll',
          field,
        ]),
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
  if (isStateRef(operand) || isInventoryRef(operand)) {
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

/** v4 `isNotAnArray`: a union branch whose only complaint is "this is not an array". */
function isNotAnArray(branch: readonly Issue[]): boolean {
  return branch.length === 1 && (branch[0] as TaggedIssue).notArray === true && branch[0].path.length === 0;
}

function flattenIssues(issues: readonly Issue[]): string[] {
  return issues.map((issue: TaggedIssue) => {
    const located = (message: string) =>
      issue.path.length ? `${issue.path.join('.')}: ${message}` : message;

    if (issue.unionErrors) {
      // An any-of array is an alternative SPELLING, not an alternative
      // meaning: when the value simply was not an array, that branch has
      // nothing to say and only buries the real complaint.
      const relevant = issue.unionErrors.filter((branch) => !isNotAnArray(branch));
      // Sub-issue paths are relative to the union, so they are flattened without
      // the prefix — v4 does the same.
      const branches = (relevant.length > 0 ? relevant : issue.unionErrors)
        .map((branch) => flattenIssues(branch).join('; '))
        .filter((branch) => branch.length > 0);
      if (branches.length > 0) return located(branches.join(' — or — '));
    }

    if (issue.inner !== undefined && issue.inner.length > 0) {
      // A record key's own refinement (an aggregate or progress key) says WHY;
      // "Invalid key in record" alone does not.
      return located(issue.inner.join('; '));
    }

    return located(issue.message);
  });
}

/**
 * Top-level keys the v1 format knows about, in schema order. Anything else is
 * reserved for v2. v4 derives it from `QtapCustomToolSchema.shape`; the SPA has
 * no schema object, so it is written out (17 since `a05c6f755`'s `announceAsCharacter`).
 */
export const KNOWN_TOP_LEVEL_KEYS: readonly string[] = [
  '$schema',
  'name',
  'title',
  'chipLabel',
  'triggers',
  'description',
  'disabled',
  'availableWhen',
  'withheldWhen',
  'revealOdds',
  'defaultVisibility',
  'announceAsCharacter',
  'parameters',
  'roll',
  'llm',
  'effects',
  'outcomes',
];

const KNOWN_TOP_LEVEL_KEY_SET: ReadonlySet<string> = new Set(KNOWN_TOP_LEVEL_KEYS);

/**
 * Report top-level keys this build doesn't understand, so discovery can log
 * them. They are tolerated, not rejected: `persist` is a planned v2 key, and an
 * older build must not choke on a newer file.
 */
export function collectUnknownKeys(raw: unknown): string[] {
  if (typeof raw !== 'object' || raw === null) return [];
  return Object.keys(raw as Record<string, unknown>).filter((k) => !KNOWN_TOP_LEVEL_KEY_SET.has(k));
}
