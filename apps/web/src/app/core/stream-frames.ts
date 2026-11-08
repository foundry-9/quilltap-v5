/**
 * Salon stream frames — the client's ONE parser for what a Salon turn's events
 * carry (v4 `lib/chat/stream-frames.ts`, `604a082c4`; contract C3).
 *
 * A Salon turn (send, continue, nudge, Storyteller run, swipe regeneration)
 * reaches the SPA as `kind`-tagged frames scoped onto the global `/api/events`
 * stream. Each one is a member of {@link ChatStreamFrame}; the client reads
 * them only through {@link parseFrame} / {@link parseFrameValue}, and
 * dispatches with an exhaustive `switch (frame.kind)` ending in
 * {@link assertNever}, so a new kind is a compile error until every consumer
 * handles or ignores it, and no ordering of tests decides what a frame means.
 *
 * Two consolidations over the old untagged bag (v4's own words):
 *
 * - `done.outcome` is one value — `finished`, `skipped` (a "nothing to add"
 *   pass), `yielded` (a bare yield to the raised hand), `stopped` (the operator
 *   hard-stopped the turn before any prose) or `empty` (the provider returned
 *   nothing) — instead of five boolean side flags.
 * - `posted` is the one frame for "here is a full message the server has
 *   already saved" (a Carina answer, a Host announcement, a Pascal outcome),
 *   which the client inserts identically; `what` says which.
 *
 * ## The twin
 *
 * v4's parser is a Zod 4.6.7 `discriminatedUnion('kind', …)` of plain
 * `z.object`s; the SPA has no `zod` (`pascal/zod-shim.ts`'s header records
 * why), so this module is a hand-written schema walk that reproduces Zod's
 * OUTPUT, not just its verdict:
 *
 * - unknown keys are STRIPPED at every schema-typed level (the envelope's
 *   `chatId`, a legacy flag riding a frame, the swipe status's nested `kind`);
 * - keys come out in SCHEMA order, not input order;
 * - an optional key absent from the input stays absent;
 * - the `z.custom` members (`round`, `bid`, a posted / swipe `message`, each
 *   `routeTrail` entry) and the `z.unknown` ones (`result`, each tool argument
 *   value) pass through UNTOUCHED, by reference.
 *
 * A frame the schema refuses is dropped (null) and WARNed ONCE per `kind`
 * string for the module's life, with Zod's first three issue texts — the
 * corpus `src/testing/fixtures/stream-frames.oracle.ndjson`, recorded from
 * v4's REAL `parseFrame` at `624d0d25c`, pins the output bytes, the warns and
 * the issue texts of every refused row (`stream-frames.oracle.spec.ts`).
 *
 * NO-PORT (server-side, P4.D366): `toLegacyBag`, `encodeFrame`, `tagFrame`,
 * `seatedFrameEncoder`, `legacyFrameEncoder`, and the dev-only
 * `[StreamFrames] Malformed frame emitted` ERROR (§R.5 / R2).
 *
 * @module core/stream-frames
 */

import type { ChatStreamFrame, DoneOutcome } from './core-contract';
import { isPlainObject } from '../pascal/zod-shim';

// ---------------------------------------------------------------------------
// A minimal Zod-shaped walk: each parser returns its output or its issues.
// ---------------------------------------------------------------------------

/** One Zod issue: a path (keys and array indices) and the en-locale message. */
interface FrameIssue {
  path: Array<string | number>;
  message: string;
}

/** The ABSENT marker an optional key yields when the input omits it. */
const ABSENT: unique symbol = Symbol('absent');

type Parsed = { ok: true; value: unknown } | { ok: false; issues: FrameIssue[] };
type Parser = (input: unknown) => Parsed;

const ok = (value: unknown): Parsed => ({ ok: true, value });
const fail = (message: string): Parsed => ({ ok: false, issues: [{ path: [], message }] });

/**
 * Zod 4's `util.parsedType` + the en locale's `getTypeName`: `null`, `array`,
 * `NaN`, and a non-finite number by its own name (`Infinity`) — the last is
 * how a JSON `1e999` reports.
 */
function receivedName(input: unknown): string {
  if (typeof input === 'number') {
    if (Number.isNaN(input)) return 'NaN';
    if (!Number.isFinite(input)) return String(input);
    return 'number';
  }
  if (input === null) return 'null';
  if (Array.isArray(input)) return 'array';
  return typeof input;
}

const invalidType = (expected: string, input: unknown): Parsed =>
  fail(`Invalid input: expected ${expected}, received ${receivedName(input)}`);

const str: Parser = (v) => (typeof v === 'string' ? ok(v) : invalidType('string', v));
/** `z.number()` — Zod 4 refuses `NaN` and `±Infinity` at the type check. */
const num: Parser = (v) => (typeof v === 'number' && Number.isFinite(v) ? ok(v) : invalidType('number', v));
const bool: Parser = (v) => (typeof v === 'boolean' ? ok(v) : invalidType('boolean', v));
/** `z.literal(true)`. */
const literalTrue: Parser = (v) => (v === true ? ok(v) : fail('Invalid input: expected true'));
/** `z.unknown()` — anything, untouched. */
const unknownValue: Parser = (v) => ok(v);

/** `z.enum([...])` — the en locale's `joinValues(values, '|')` over JSON-quoted strings. */
function enumOf(values: readonly string[]): Parser {
  const list = values.map((o) => JSON.stringify(o)).join('|');
  return (v) => (typeof v === 'string' && values.includes(v) ? ok(v) : fail(`Invalid option: expected one of ${list}`));
}

/** `z.custom(pred, { message })` — passes the input through by reference. */
function custom(pred: (v: unknown) => boolean, message = 'Invalid input'): Parser {
  return (v) => (pred(v) ? ok(v) : fail(message));
}

const nullable =
  (p: Parser): Parser =>
  (v) =>
    v === null ? ok(null) : p(v);

const optional =
  (p: Parser): Parser =>
  (v) =>
    v === undefined ? ok(ABSENT) : p(v);

function prefixed(seg: string | number, issues: FrameIssue[]): FrameIssue[] {
  return issues.map((i) => ({ path: [seg, ...i.path], message: i.message }));
}

/** `z.array(p)` — every element checked; issues carry the element's index. */
function arrayOf(p: Parser): Parser {
  return (v) => {
    if (!Array.isArray(v)) return invalidType('array', v);
    const issues: FrameIssue[] = [];
    const out: unknown[] = [];
    v.forEach((el, i) => {
      const r = p(el);
      if (r.ok) out.push(r.value);
      else issues.push(...prefixed(i, r.issues));
    });
    return issues.length ? { ok: false, issues } : ok(out);
  };
}

/** `z.record(z.string(), z.unknown())` — a plain object, values untouched. */
const recordOfUnknown: Parser = (v) => (isPlainObject(v) ? ok({ ...v }) : invalidType('record', v));

type Shape = ReadonlyArray<readonly [key: string, parser: Parser]>;

/**
 * `z.object(shape)` in Zod's default STRIP mode: walks the SHAPE (not the
 * input), so the output carries only schema keys, in schema order, and an
 * optional key the input omits stays absent.
 */
function objectOf(shape: Shape): Parser {
  return (v) => {
    if (!isPlainObject(v)) return invalidType('object', v);
    const issues: FrameIssue[] = [];
    const out: Record<string, unknown> = {};
    for (const [key, p] of shape) {
      const r = p(v[key]);
      if (!r.ok) issues.push(...prefixed(key, r.issues));
      else if (r.value !== ABSENT) out[key] = r.value;
    }
    return issues.length ? { ok: false, issues } : ok(out);
  };
}

// ---------------------------------------------------------------------------
// The pieces (v4 `:48-126`)
// ---------------------------------------------------------------------------

/** A non-null object; the nested payloads below are typed, not re-validated. */
const isObject = (v: unknown): boolean => typeof v === 'object' && v !== null && !Array.isArray(v);

/** A full chat message the server has already persisted. */
const postedMessage = custom(
  (v) => isObject(v) && typeof (v as { id?: unknown }).id === 'string',
  'Expected a persisted message with an id',
);

const seatTag = objectOf([
  ['participantId', str],
  ['concurrent', literalTrue],
]);

/** The status line shown above the composer while a reply is prepared. */
const responseStatus = objectOf([
  ['stage', str],
  ['message', str],
  ['toolName', optional(str)],
  ['characterName', optional(str)],
  ['characterId', optional(str)],
]);

const usage = objectOf([
  ['promptTokens', optional(num)],
  ['completionTokens', optional(num)],
  ['totalTokens', optional(num)],
]);

const cacheUsage = objectOf([
  ['cacheCreationInputTokens', optional(num)],
  ['cacheReadInputTokens', optional(num)],
]);

/** What the provider plugin managed to put on the wire (bug 94). */
const attachmentResults = objectOf([
  ['sent', arrayOf(str)],
  [
    'failed',
    arrayOf(
      objectOf([
        ['id', str],
        ['error', str],
      ]),
    ),
  ],
]);

/** A positioned reasoning block — DISPLAY ONLY. */
const reasoningSegment = objectOf([
  ['anchorOffset', num],
  ['content', str],
  ['seq', num],
]);

const turnInfo = objectOf([
  ['nextSpeakerId', nullable(str)],
  ['reason', str],
  ['cycleComplete', bool],
  ['isUsersTurn', bool],
]);

/** How a `done` frame closed its turn. */
export const DONE_OUTCOMES: readonly DoneOutcome[] = ['finished', 'skipped', 'yielded', 'stopped', 'empty'];
const doneOutcome = enumOf(DONE_OUTCOMES);

/** Why a chain stopped. */
const chainCompleteReason = enumOf([
  'user_turn',
  'paused',
  'max_depth',
  'max_time',
  'error',
  'no_next_speaker',
  'cycle_complete',
  'yielded',
  'stopped',
  'round_open',
]);

/** One attachment the file-fallback pipeline processed before the turn. */
const fileProcessingInfo = objectOf([
  ['filename', str],
  ['type', str],
  ['usedImageDescriptionLLM', bool],
  ['error', optional(str)],
]);

/** Every variant may carry a seat tag (concurrent Storyteller turns) — always last. */
const SEAT = ['seat', optional(seatTag)] as const;

// ---------------------------------------------------------------------------
// The union (v4 `:132-298`) — the discriminator's own `kind` key first
// ---------------------------------------------------------------------------

const literalKind = (k: string): Parser => (v) => (v === k ? ok(v) : fail(`Invalid input: expected "${k}"`));

const VARIANTS: ReadonlyArray<readonly [kind: string, shape: Shape]> = [
  ['content', [['text', str], SEAT]],
  ['reasoning', [['text', str], SEAT]],
  ['status', [['status', responseStatus], SEAT]],
  ['fileProcessing', [['info', arrayOf(fileProcessingInfo)], SEAT]],
  [
    'toolsDetected',
    [
      ['names', arrayOf(str)],
      ['arguments', arrayOf(recordOfUnknown)],
      SEAT,
    ],
  ],
  [
    'toolResult',
    [
      ['index', optional(num)],
      ['name', str],
      ['success', bool],
      ['result', optional(unknownValue)],
      ['error', optional(str)],
      SEAT,
    ],
  ],
  [
    'turnStart',
    [
      ['participantId', str],
      ['characterName', str],
      ['chainDepth', num],
      SEAT,
    ],
  ],
  [
    'done',
    [
      ['outcome', doneOutcome],
      ['messageId', nullable(str)],
      ['participantId', optional(nullable(str))],
      ['usage', nullable(usage)],
      ['cacheUsage', nullable(cacheUsage)],
      ['attachmentResults', nullable(attachmentResults)],
      ['toolsExecuted', bool],
      ['turn', optional(turnInfo)],
      ['emptyResponseReason', optional(str)],
      ['skippedParticipantId', optional(nullable(str))],
      ['provider', optional(str)],
      ['modelName', optional(str)],
      ['routeTrail', optional(nullable(arrayOf(custom(isObject))))],
      ['isSilentMessage', optional(bool)],
      ['pendingExternalTurn', optional(bool)],
      ['reasoningContent', optional(nullable(str))],
      ['reasoningSegments', optional(nullable(arrayOf(reasoningSegment)))],
      SEAT,
    ],
  ],
  [
    'turnComplete',
    [
      ['participantId', str],
      ['messageId', str],
      ['chainDepth', num],
      ['skipped', optional(bool)],
      SEAT,
    ],
  ],
  [
    'chainComplete',
    [
      ['reason', chainCompleteReason],
      ['nextSpeakerId', nullable(str)],
      ['chainDepth', num],
      ['paused', optional(bool)],
      ['heldUserTurn', optional(bool)],
      SEAT,
    ],
  ],
  [
    'error',
    [
      ['message', str],
      ['errorType', str],
      ['details', str],
      ['userMessagePersisted', optional(bool)],
      ['pendingToolResultsPersisted', optional(bool)],
      SEAT,
    ],
  ],
  [
    'pendingExternalTurn',
    [
      ['messageId', str],
      ['participantId', str],
      ['characterName', str],
      SEAT,
    ],
  ],
  [
    'posted',
    [
      ['what', enumOf(['carinaAnswer', 'hostAnnouncement', 'pascalResult'])],
      ['message', postedMessage],
      SEAT,
    ],
  ],
  ['storytellerRound', [['round', custom(isObject)], SEAT]],
  [
    'storytellerBid',
    [
      ['roundId', str],
      ['bid', custom(isObject)],
      SEAT,
    ],
  ],
  ['roundOpen', [['roundId', str], SEAT]],
  [
    'handAsked',
    [
      ['participantId', str],
      ['characterName', str],
      SEAT,
    ],
  ],
  [
    'handAnswered',
    [
      ['participantId', str],
      ['characterName', str],
      ['yielded', bool],
      SEAT,
    ],
  ],
  [
    'confirmationResult',
    [
      ['messageId', str],
      ['confirmed', nullable(bool)],
      ['revised', bool],
      ['notes', nullable(str)],
      ['content', optional(str)],
      SEAT,
    ],
  ],
  ['swipeDone', [['message', postedMessage], SEAT]],
];

const VARIANT_PARSERS = new Map<string, Parser>(
  VARIANTS.map(([kind, shape]) => [kind, objectOf([['kind', literalKind(kind)], ...shape])]),
);

/** The en locale's discriminator sentence, the options in union order. */
const DISCRIMINATOR_MESSAGE = `Invalid discriminator value. Expected ${VARIANTS.map(([k]) => `'${k}'`).join(' | ')}`;

/** `StreamFrameSchema.safeParse` — the output, or Zod's issues. */
function safeParseFrame(json: unknown): Parsed {
  if (!isPlainObject(json)) return invalidType('object', json);
  const kind = json['kind'];
  const parser = typeof kind === 'string' ? VARIANT_PARSERS.get(kind) : undefined;
  if (!parser) return { ok: false, issues: [{ path: ['kind'], message: DISCRIMINATOR_MESSAGE }] };
  return parser(json);
}

/**
 * The first three issue texts v4 logs (`${path.join('.')}: ${message}`) —
 * exported for the oracle spec, which pins them on every refused corpus row.
 */
export function frameIssueTexts(json: unknown): string[] | null {
  const r = safeParseFrame(json);
  if (r.ok) return null;
  return r.issues.slice(0, 3).map((i) => `${i.path.join('.')}: ${i.message}`);
}

// ---------------------------------------------------------------------------
// The helpers (v4 `:300-334`)
// ---------------------------------------------------------------------------

/** Exhaustiveness guard for `switch (frame.kind)` / `switch (outcome)`. */
export function assertNever(value: never): never {
  throw new Error(`Unhandled stream frame variant: ${JSON.stringify(value)}`);
}

/**
 * A `done` that leaves no bubble behind: a pass, a bare yield or a hard stop.
 * The client resets its buffer without appending a reply or toasting (the Host
 * note, where there is one, is the visible record).
 */
export function doneLeavesNoReply(outcome: DoneOutcome): boolean {
  switch (outcome) {
    case 'skipped':
    case 'yielded':
    case 'stopped':
      return true;
    case 'finished':
    case 'empty':
      return false;
    default:
      return assertNever(outcome);
  }
}

// ---------------------------------------------------------------------------
// Parsing (v4 `:411-439`)
// ---------------------------------------------------------------------------

/**
 * The kinds already reported. MODULE-global for the page's life, as v4's is —
 * so a recurring malformed kind costs one WARN, not one per frame.
 */
const reportedKinds = new Set<string>();

/**
 * Parse an already-`JSON.parse`d payload — the half every v5 reader calls,
 * since the transport parsed the event envelope before any reader sees it.
 * Returns null for anything that is not a known frame; a refused frame is
 * WARNed once per `kind` and ignored rather than crashing a stream mid-turn.
 */
export function parseFrameValue(json: unknown): ChatStreamFrame | null {
  const parsed = safeParseFrame(json);
  if (parsed.ok) return parsed.value as ChatStreamFrame;
  const kind = isObject(json) ? String((json as { kind?: unknown }).kind) : typeof json;
  if (!reportedKinds.has(kind)) {
    reportedKinds.add(kind);
    console.warn('[StreamFrames] Ignoring an unrecognised stream frame', {
      kind,
      issues: parsed.issues.slice(0, 3).map((i) => `${i.path.join('.')}: ${i.message}`),
    });
  }
  return null;
}

/**
 * The client's only parser for a Salon stream's raw `data:` payload — v4's
 * exact twin. Returns null for blank keep-alive payloads, chunking artefacts
 * and anything that is not a known frame.
 */
export function parseFrame(raw: string): ChatStreamFrame | null {
  const trimmed = raw.trim();
  if (!trimmed || trimmed === '[DONE]' || trimmed === '{}') return null;
  let json: unknown;
  try {
    json = JSON.parse(trimmed);
  } catch {
    return null;
  }
  return parseFrameValue(json);
}
