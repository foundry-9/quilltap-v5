/**
 * "Nothing to add" turn-skipping — shared pure logic, ported near-verbatim from
 * v4 `lib/chat/turn-manager/skip-signal.ts` (HEAD b8449b3e, incl. `e22f7b36`'s
 * direct-address rewrite). The Rust port `crate::skip_signal` is the second
 * reference.
 *
 * Client-safe: only pure string helpers (imported from `./skip-signal-helpers`)
 * and type-level shapes. The Salon uses `computeSkipEligibility` for the human
 * Skip-button guard (the banner is offered only when a skip is allowed).
 *
 * A pass is recorded as a Host message (`systemSender: 'host'`,
 * `systemKind: 'turn-pass'`, `hostEvent: { participantId }`) — no new state
 * columns. Every derivation below recomputes from that history.
 *
 * The event/participant/character shapes are loosened (optional fields) so the
 * Salon's `MessageDto` / `ParticipantDetail` rows can flow through directly; a
 * caller stamps `type: 'message'` on each message row (as v4's SalonView does).
 */

import {
  escapeRegex,
  isParticipantPresent,
  normalizeContentBlockFormat,
  stripCharacterNamePrefix,
  type MentionCharacter,
} from './skip-signal-helpers';

/** The literal sentinel a character emits to pass its turn. */
export const NOTHING_TO_ADD_SENTINEL = '[NOTHING TO ADD]';

/** `systemKind` stamped on the Host message that records a pass. */
export const TURN_PASS_SYSTEM_KIND = 'turn-pass';

/**
 * A message-shaped event (v4 `ChatEvent`/`MessageEvent`, loosened for the
 * client). Callers stamp `type: 'message'` on each row so the `m.type !==
 * 'message'` guards below behave exactly as on the server.
 */
export interface SkipEvent {
  type?: string;
  id?: string;
  role?: 'USER' | 'ASSISTANT' | 'SYSTEM' | string;
  content?: string | null;
  participantId?: string | null;
  targetParticipantIds?: string[] | null;
  systemSender?: string | null;
  systemKind?: string | null;
  hostEvent?: { participantId?: string | null } | null;
  isSilentMessage?: boolean | null;
}

/** A participant (v4 `ChatParticipantBase`, loosened). */
export interface SkipParticipant {
  id: string;
  type?: string;
  characterId?: string | null;
  controlledBy?: string;
  status?: string;
}

/** The responding character for the mention scan (v4 `Character`, loosened). */
export type SkipCharacter = MentionCharacter;

/**
 * Type guard: is this event a Host turn-pass record? A turn-pass carries the
 * passing participant's id in `hostEvent.participantId`.
 */
export function isTurnPassMessage(
  m: unknown,
): m is SkipEvent & { hostEvent: { participantId: string } } {
  if (!m || typeof m !== 'object') return false;
  const msg = m as {
    type?: unknown;
    systemSender?: unknown;
    systemKind?: unknown;
    hostEvent?: unknown;
  };
  if (msg.type !== 'message') return false;
  if (msg.systemSender !== 'host') return false;
  if (msg.systemKind !== TURN_PASS_SYSTEM_KIND) return false;
  const he = msg.hostEvent;
  return (
    !!he &&
    typeof he === 'object' &&
    typeof (he as { participantId?: unknown }).participantId === 'string'
  );
}

export type DetectSkipResult = { skip: true } | { skip: false; cleaned?: string };

/**
 * Decide whether a raw model response is a turn-pass. The response is
 * normalized, stripped of any leading own-name prefix, and its FIRST non-empty
 * line is examined against the sentinel phrase.
 */
export function detectSkipSentinel(
  response: string,
  characterName?: string,
  aliases?: string[],
): DetectSkipResult {
  if (!response) return { skip: false };

  const normalizedRaw = normalizeContentBlockFormat(response);
  const normalized =
    characterName || (aliases && aliases.length > 0)
      ? stripCharacterNamePrefix(normalizedRaw, characterName, aliases)
      : normalizedRaw;

  const lines = normalized.split('\n');

  let firstIdx = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim().length > 0) {
      firstIdx = i;
      break;
    }
  }
  if (firstIdx === -1) return { skip: false };

  if (!isSentinelLine(lines[firstIdx])) return { skip: false };

  const trailing = lines.slice(firstIdx + 1).join('\n');
  if (trailing.trim().length === 0) {
    return { skip: true };
  }

  const cleaned = [...lines.slice(0, firstIdx), ...lines.slice(firstIdx + 1)].join('\n').trim();
  return { skip: false, cleaned };
}

/**
 * Does a single line, once its wrapping and trailing punctuation are shed,
 * equal the sentinel phrase? Brackets are optional; matching is case-insensitive.
 */
function isSentinelLine(line: string): boolean {
  let s = line.trim();
  const wrappers = new Set(['*', '_', '~', '"', "'", '`']);
  let changed = true;
  while (changed && s.length > 1) {
    changed = false;
    const first = s[0];
    const last = s[s.length - 1];
    if (wrappers.has(first) && (first === last || wrappers.has(last))) {
      s = s.slice(1, -1).trim();
      changed = true;
    }
  }
  s = s.replace(/^\[/, '').replace(/\]$/, '').trim();
  s = s.replace(/[.!?,;:]+$/, '').trim();
  return s.toLowerCase() === 'nothing to add';
}

/**
 * Backward-walk the event history collecting the participant ids of every
 * turn-pass record posted since the most recent substantive message.
 */
export function findSkippedSinceLastSubstantive(events: ReadonlyArray<SkipEvent>): Set<string> {
  const skipped = new Set<string>();
  for (let i = events.length - 1; i >= 0; i--) {
    const m = events[i];
    if (m.type !== 'message') continue;
    if (isTurnPassMessage(m)) {
      skipped.add(m.hostEvent.participantId);
      continue;
    }
    if ((m.role === 'USER' || m.role === 'ASSISTANT') && m.participantId) {
      const isWhisper =
        Array.isArray(m.targetParticipantIds) && (m.targetParticipantIds?.length ?? 0) > 0;
      if (!isWhisper) break;
    }
  }
  return skipped;
}

/**
 * Whether a chat is large/busy enough for turn-skipping to apply at all: more
 * than two active character participants, OR at least two LLM-driven ones.
 */
export function qualifiesForTurnSkipping(participants: ReadonlyArray<SkipParticipant>): boolean {
  const activeChars = participants.filter(
    (p) => p.type === 'CHARACTER' && isParticipantPresent(p.status) && !!p.characterId,
  );
  if (activeChars.length > 2) return true;
  const llmChars = activeChars.filter((p) => p.controlledBy !== 'user');
  return llmChars.length >= 2;
}

/**
 * True when no character has yet taken an LLM turn (no ASSISTANT message with a
 * non-null participantId). Greetings count; Staff messages do not.
 */
export function isFirstCharacterTurn(events: ReadonlyArray<SkipEvent>): boolean {
  for (const m of events) {
    if (m.type !== 'message') continue;
    if (m.role === 'ASSISTANT' && m.participantId) return false;
  }
  return true;
}

/**
 * A conversational turn visible to the responding character.
 */
function isVisibleConversationalTurn(m: SkipEvent, respondingParticipantId: string): boolean {
  if (m.systemSender) return false;
  if (m.isSilentMessage === true) return false;
  if (typeof m.content !== 'string' || m.content.trim() === '') return false;
  const targets = m.targetParticipantIds;
  if (targets && targets.length > 0 && !targets.includes(respondingParticipantId)) {
    return false;
  }
  return true;
}

/** How many recent visible turns to scan for a "recently addressed" signal. */
const RECENTLY_ADDRESSED_LOOKBACK = 10;

/**
 * Short interjections that commonly lead into a vocative ("Hey Marion, ...").
 * Deliberately small — the goal is catching real address openers, not every
 * word that could precede a name.
 */
const VOCATIVE_LEAD_INS =
  '(?:hey|hi|hello|oh|no|yes|well|so|and|but|now|listen|please|right|ok|okay|thanks|sorry|merci)';

/**
 * Characters that can end the clause *before* a vocative: sentence punctuation,
 * quotes, brackets, markdown emphasis, newlines, dashes. `-` sits last so the
 * class needs no escaping.
 */
const VOCATIVE_PRE_BOUNDARY = '[.!?;:…"“”\'()\\[\\]*_~\\n—–-]';

/**
 * Build a regex matching the character's name or an alias in a direct-address
 * (vocative) position: preceded by the start of the text, a clause boundary, a
 * comma, an `@`, or a lead-in interjection — and followed by address
 * punctuation (`Marion,` / `Greg?` / `Amy —` / `Al.`) or the end of a line. A
 * name flowing mid-sentence ("if Greg is ready", "Friday's block", "I glance at
 * Amy over the bench") deliberately does NOT match: narrating or citing someone
 * is not speaking *to* them.
 *
 * Returns null when the character has no usable name tokens.
 */
function buildDirectAddressRegex(character: SkipCharacter): RegExp | null {
  const tokens = [character.name, ...(character.aliases ?? [])]
    .map((t) => t?.trim())
    .filter((t): t is string => !!t && t.length > 0);
  if (tokens.length === 0) return null;

  // Longer tokens first so "John Smith" wins over "John".
  const names = tokens
    .sort((a, b) => b.length - a.length)
    .map(escapeRegex)
    .join('|');

  return new RegExp(
    `(?:^|${VOCATIVE_PRE_BOUNDARY}\\s*|,\\s+|@|\\b${VOCATIVE_LEAD_INS},?\\s+)` +
      `(?:${names})\\s*(?:[,.!?…—–:;]|$)`,
    'im',
  );
}

/**
 * Has the responding character been DIRECTLY addressed since they last spoke?
 * A hit is the responder's name/alias in a vocative position
 * ({@link buildDirectAddressRegex}), or a whisper targeted at the responder.
 *
 * Direct address, not mere mention, on purpose (v4 `e22f7b36`): in a
 * chorus-prone group scene every turn's roll-call recap names most of the cast,
 * so a mention-based signal marked everyone as addressed forever and the
 * "answer rather than pass" caution fired for every character on every turn —
 * nobody ever passed.
 */
export function isRecentlyAddressed(
  events: ReadonlyArray<SkipEvent>,
  respondingParticipantId: string,
  respondingCharacter: SkipCharacter,
): boolean {
  let lastOwnIdx = -1;
  for (let i = events.length - 1; i >= 0; i--) {
    const m = events[i];
    if (m.type !== 'message') continue;
    if (m.role !== 'ASSISTANT') continue;
    if (m.participantId !== respondingParticipantId) continue;
    if (m.systemSender) continue;
    const isWhisper =
      Array.isArray(m.targetParticipantIds) && (m.targetParticipantIds?.length ?? 0) > 0;
    if (isWhisper) continue;
    lastOwnIdx = i;
    break;
  }

  const visible: SkipEvent[] = [];
  for (let i = lastOwnIdx + 1; i < events.length; i++) {
    const m = events[i];
    if (m.type !== 'message') continue;
    if (isVisibleConversationalTurn(m, respondingParticipantId)) {
      visible.push(m);
    }
  }
  const window = visible.slice(-RECENTLY_ADDRESSED_LOOKBACK);
  if (window.length === 0) return false;

  for (const m of window) {
    const targets = m.targetParticipantIds;
    if (targets && targets.length > 0 && targets.includes(respondingParticipantId)) {
      return true;
    }
  }

  const regex = buildDirectAddressRegex(respondingCharacter);
  if (!regex) return false;
  const corpus = window.map((m) => m.content ?? '').join('\n');
  return regex.test(corpus);
}

export type MustSpeakReason =
  | 'not-multi-character'
  | 'feature-disabled'
  | 'first-character-turn'
  | 'summoned'
  | 'already-skipped'
  | 'all-others-skipped'
  | null;

export interface ComputeSkipEligibilityOptions {
  events: ReadonlyArray<SkipEvent>;
  participants: ReadonlyArray<SkipParticipant>;
  respondingParticipantId: string;
  respondingCharacter: SkipCharacter;
  /** Nudge / queue-popped turn — the operator explicitly summoned this voice. */
  summoned?: boolean;
  /** Per-chat toggle; NULL/true = enabled. Pass `chat.turnSkippingEnabled !== false`. */
  turnSkippingEnabled: boolean;
}

export interface SkipEligibility {
  offerSkip: boolean;
  mustSpeakReason: MustSpeakReason;
  recentlyAddressed: boolean;
}

/**
 * Decide whether the responding character may be offered the skip option this
 * turn, and (for the human Skip-button guard) why not.
 *
 * Precedence of withhold reasons: not-multi-character → feature-disabled →
 * first-character-turn → summoned → already-skipped → all-others-skipped.
 */
export function computeSkipEligibility(options: ComputeSkipEligibilityOptions): SkipEligibility {
  const {
    events,
    participants,
    respondingParticipantId,
    respondingCharacter,
    summoned = false,
    turnSkippingEnabled,
  } = options;

  const recentlyAddressed = isRecentlyAddressed(
    events,
    respondingParticipantId,
    respondingCharacter,
  );

  let mustSpeakReason: MustSpeakReason = null;

  if (!qualifiesForTurnSkipping(participants)) {
    mustSpeakReason = 'not-multi-character';
  } else if (!turnSkippingEnabled) {
    mustSpeakReason = 'feature-disabled';
  } else if (isFirstCharacterTurn(events)) {
    mustSpeakReason = 'first-character-turn';
  } else if (summoned) {
    mustSpeakReason = 'summoned';
  } else {
    const skipped = findSkippedSinceLastSubstantive(events);
    if (skipped.has(respondingParticipantId)) {
      mustSpeakReason = 'already-skipped';
    } else {
      const otherActiveCharacters = participants.filter(
        (p) =>
          p.type === 'CHARACTER' &&
          isParticipantPresent(p.status) &&
          !!p.characterId &&
          p.id !== respondingParticipantId,
      );
      const allOthersSkipped = otherActiveCharacters.every((p) => skipped.has(p.id));
      if (allOthersSkipped) {
        mustSpeakReason = 'all-others-skipped';
      }
    }
  }

  return {
    offerSkip: mustSpeakReason === null,
    mustSpeakReason,
    recentlyAddressed,
  };
}
