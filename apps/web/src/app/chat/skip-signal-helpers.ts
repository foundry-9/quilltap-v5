/**
 * The small pure string/scan helpers the skip-signal logic imports — ported
 * near-verbatim from the v4 server modules the client-safe `skip-signal.ts`
 * pulls in:
 *
 *   - `normalizeContentBlockFormat` / `stripCharacterNamePrefix`
 *       (`lib/llm/response-normalizer.ts`)
 *   - `escapeRegex` (`lib/utils/regex.ts`) — the direct-address regex builder's
 *       token escape since v4 `e22f7b36`
 *   - `isParticipantPresent` (`lib/schemas/chat.types.ts`)
 *
 * Kept apart from `skip-signal.ts` so that module stays a faithful mirror of
 * v4's `lib/chat/turn-manager/skip-signal.ts`. The Rust ports
 * (`crate::message_formatter`, `crate::skip_signal`) are the second reference.
 *
 * `findMentionedCharacterIds` is gone: v4 `9a88f862f` deleted the module it
 * mirrored, and it had no client consumer (P4.D347's R-8 HANDOFF).
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

