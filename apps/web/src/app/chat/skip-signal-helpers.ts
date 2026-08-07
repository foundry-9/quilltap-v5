/**
 * The small pure string/scan helpers the skip-signal logic imports — ported
 * near-verbatim from the v4 server modules the client-safe `skip-signal.ts`
 * pulls in:
 *
 *   - `normalizeContentBlockFormat` / `stripCharacterNamePrefix`
 *       (`lib/llm/response-normalizer.ts`)
 *   - `findMentionedCharacterIds` (`lib/chat/context/mentioned-characters.ts`)
 *   - `escapeRegex` (`lib/utils/regex.ts`)
 *   - `isParticipantPresent` (`lib/schemas/chat.types.ts`)
 *
 * Kept apart from `skip-signal.ts` so that module stays a faithful mirror of
 * v4's `lib/chat/turn-manager/skip-signal.ts`. The Rust ports
 * (`crate::message_formatter`, `crate::mentioned_characters`,
 * `crate::skip_signal`) are the second reference.
 */

/** A minimal character shape for the mention scan (v4 `Character`, loosened). */
export interface MentionCharacter {
  id: string;
  name?: string | null;
  aliases?: string[] | null;
}

/**
 * Escape special regex characters so a string can be embedded in a `RegExp`
 * as a fixed substring (v4 `lib/utils/regex.ts`).
 */
export function escapeRegex(input: string): string {
  return input.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Present = active or silent (v4 `isParticipantPresent`). Both states take
 * turns and perceive the scene.
 */
export function isParticipantPresent(status: string | null | undefined): boolean {
  return status === 'active' || status === 'silent';
}

/**
 * A seat is "user-driven" when its durable `controlledBy` is `'user'` OR the
 * human is impersonating it this session. v4 Bug 44's overlay
 * (`62c63dc3`) leaves `controlledBy` at `'llm'` and records the id in
 * `impersonatingParticipantIds` instead, so turn-taking — the "type as them"
 * prompt and the Skip button — must consult the overlay, not the bare column,
 * or an impersonated seat's paused turn surfaces neither affordance. Client
 * mirror of v4's `isUserDrivenSeat` (`lib/chat/turn-manager/utils.ts`) and the
 * core `is_user_driven_seat` (`participant_filters.rs`).
 */
export function isUserDrivenSeat(
  participantId: string,
  controlledBy: 'llm' | 'user' | null | undefined,
  impersonatingParticipantIds: readonly string[] | null | undefined,
): boolean {
  if (controlledBy === 'user') return true;
  return Array.isArray(impersonatingParticipantIds) && impersonatingParticipantIds.includes(participantId);
}

/**
 * Normalize LLM response content that may be wrapped in Anthropic content-block
 * array format (`[{'type': 'text', 'text': "..."}]`). Pure string transform.
 */
export function normalizeContentBlockFormat(content: string): string {
  if (!content) return content;

  const trimmed = content.trim();

  // Quick checks to avoid expensive regex on normal content.
  if (!trimmed.startsWith('[')) return content;
  if (!trimmed.includes("'type'") && !trimmed.includes('"type"')) return content;

  // Python-style single quotes: [{'type': 'text', 'text': "..."}]
  const pythonPattern = /^\[\s*\{\s*'type'\s*:\s*'text'\s*,\s*'text'\s*:\s*"([\s\S]*)"\s*\}\s*\]$/;
  const pythonMatch = trimmed.match(pythonPattern);
  if (pythonMatch) {
    return pythonMatch[1];
  }

  // Double-quoted JSON: [{"type": "text", "text": "..."}]
  try {
    const parsed = JSON.parse(trimmed);
    if (
      Array.isArray(parsed) &&
      parsed.length > 0 &&
      parsed[0]?.type === 'text' &&
      typeof parsed[0]?.text === 'string'
    ) {
      return parsed[0].text;
    }
  } catch {
    // Not valid JSON — fine, return original content.
  }

  return content;
}

/**
 * Strip a leading `[Name]` / `Name:` prefix (the responding character's name or
 * aliases) from a response, repeatedly (v4 `stripCharacterNamePrefix`). With no
 * name/aliases it falls back to a conservative short-name-in-brackets pattern.
 */
export function stripCharacterNamePrefix(
  content: string,
  characterName?: string,
  aliases?: string[],
): string {
  if (!content) return content;

  let result = content;

  const namePatterns: RegExp[] = [];

  if (characterName) {
    const escapedName = characterName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    namePatterns.push(new RegExp(`^\\s*\\[${escapedName}\\]\\s*`, 'i'));
    namePatterns.push(new RegExp(`^\\s*${escapedName}\\s*:\\s*`, 'i'));
  }

  if (aliases && aliases.length > 0) {
    for (const alias of aliases) {
      const escapedAlias = alias.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      namePatterns.push(new RegExp(`^\\s*\\[${escapedAlias}\\]\\s*`, 'i'));
      namePatterns.push(new RegExp(`^\\s*${escapedAlias}\\s*:\\s*`, 'i'));
    }
  }

  if (namePatterns.length === 0) {
    namePatterns.push(/^\s*\[[\p{L}\p{M}'\- ]{1,40}\]\s*/u);
  }

  let previousLength = -1;
  let iterations = 0;
  const MAX_ITERATIONS = 10;

  while (result.length !== previousLength && iterations < MAX_ITERATIONS) {
    previousLength = result.length;
    iterations++;

    let matched = false;
    for (const pattern of namePatterns) {
      if (pattern.test(result)) {
        result = result.replace(pattern, '');
        matched = true;
        break;
      }
    }

    if (!matched) break;
  }

  return result;
}

/**
 * Build a single case-insensitive, word-boundary regex over every candidate's
 * names/aliases (longer tokens first), or null when there is nothing to match.
 */
function buildCandidateNameRegex(candidates: MentionCharacter[]): RegExp | null {
  const tokens = new Set<string>();
  for (const character of candidates) {
    if (character.name && character.name.trim().length > 0) {
      tokens.add(character.name.trim());
    }
    for (const alias of character.aliases ?? []) {
      if (alias && alias.trim().length > 0) {
        tokens.add(alias.trim());
      }
    }
  }

  if (tokens.size === 0) return null;

  const sorted = Array.from(tokens).sort((a, b) => b.length - a.length);
  const alternation = sorted.map(escapeRegex).join('|');
  return new RegExp(`\\b(?:${alternation})\\b`, 'gi');
}

/**
 * Scan a corpus for whole-word, case-insensitive mentions of any candidate's
 * name or alias, returning the set of matched character IDs (v4
 * `findMentionedCharacterIds`).
 */
export function findMentionedCharacterIds(
  scanCorpus: string,
  candidates: MentionCharacter[],
): Set<string> {
  const matched = new Set<string>();
  if (!scanCorpus || candidates.length === 0) return matched;

  const regex = buildCandidateNameRegex(candidates);
  if (!regex) return matched;

  const tokenToCharacterIds = new Map<string, Set<string>>();
  const addToken = (token: string, characterId: string) => {
    const key = token.trim().toLowerCase();
    if (!key) return;
    let set = tokenToCharacterIds.get(key);
    if (!set) {
      set = new Set<string>();
      tokenToCharacterIds.set(key, set);
    }
    set.add(characterId);
  };
  for (const character of candidates) {
    if (character.name) addToken(character.name, character.id);
    for (const alias of character.aliases ?? []) {
      addToken(alias, character.id);
    }
  }

  let match: RegExpExecArray | null;
  while ((match = regex.exec(scanCorpus)) !== null) {
    const hit = match[0].toLowerCase();
    const ids = tokenToCharacterIds.get(hit);
    if (ids) {
      for (const id of ids) matched.add(id);
    }
  }

  return matched;
}
