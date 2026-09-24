/**
 * Mention typeahead — pure logic (v4 `lib/mentions/mention-typeahead.ts`,
 * `3376b3dfa`).
 *
 * The decisions behind the composer's `@` character typeahead: where an `@name`
 * trigger starts and ends, which characters match a query and in what order,
 * and what becomes of the `@` once a name has been completed at the start of a
 * line.
 *
 * ### The `@` after completion
 *
 * A completed mention inserts the character's plain name — no chip, no node, no
 * markup — so the `@` is normally dropped (`see @aris` → `see Aristarchus`).
 * The exception is the start of a line, where `@Name: question` and
 * `@Name? question` are Carina queries (`chat/carina-parser.ts`). There the `@`
 * is kept provisionally and judged by what the writer types next:
 * {@link classifyLineStartMention}.
 *
 * Framework-free: the trigger rule comes from `editor/char-insert/trigger.ts`
 * (v4's `lib/char-insert/trigger.ts`, which differs from it only in comments)
 * and nothing here touches Angular or ProseMirror. A character-for-character
 * transcription, pinned row-for-row by `mention-typeahead.oracle.spec.ts`
 * against a corpus recorded from v4's REAL module. v4 homes it under `lib/`
 * though it is a client module; it lives beside the Salon here.
 *
 * @module chat/mentions/mention-typeahead
 */

import { findTrigger } from '../../editor/char-insert/trigger';
import type { TriggerConfig, TriggerMatch } from '../../editor/char-insert/types';
import { isCarinaInvocableName } from '../carina-parser';

/**
 * `@` plus letters, digits, `_` or `-`. A space ends the query — and, with at
 * least one query character typed, commits the highlighted name.
 *
 * `minQueryLength: 0` is deliberate: a bare `@` opens the whole list. The
 * opener-context rule in `findTrigger` is what keeps `name@example.com` from
 * ever opening it.
 */
export const MENTION_TRIGGER: TriggerConfig = {
  opener: '@',
  queryPattern: /[\p{L}\p{M}\p{N}_-]/u,
  minQueryLength: 0,
  maxQueryLength: 48,
  closer: null,
  lowercaseQuery: false,
};

/** Find the active `@query` in the text before the cursor, or null. */
export function findMentionTrigger(textBefore: string): TriggerMatch | null {
  return findTrigger(textBefore, MENTION_TRIGGER);
}

/** The minimum a character needs in order to be offered. */
export interface MentionCandidate {
  id: string;
  name: string;
  title?: string | null;
}

/**
 * Match quality: whole-name prefix beats word prefix (`vi` → `Lady Vivienne`).
 * Mid-word substrings are deliberately not matched — `ar` offering `Barnaby`
 * is noise in a list meant to narrow as you type.
 */
const MatchTier = { NamePrefix: 0, WordPrefix: 1 } as const;
type MatchTier = (typeof MatchTier)[keyof typeof MatchTier];

function matchTier(name: string, query: string): MatchTier | null {
  if (query.length === 0) return MatchTier.NamePrefix;
  const folded = name.toLocaleLowerCase();
  const needle = query.toLocaleLowerCase();
  if (folded.startsWith(needle)) return MatchTier.NamePrefix;
  if (folded.split(/[\s\-_'’.]+/u).some((word) => word.startsWith(needle))) {
    return MatchTier.WordPrefix;
  }
  return null;
}

/**
 * The Brahma Console, offered as a name only at the start of a line — the one
 * place `@Brahma:` / `@Brahma?` means anything. It is not a character, so the
 * character list never carries it. The operator (the composer's only user) may
 * always reach it; the gating in the Carina service concerns characters.
 */
export const BRAHMA_MENTION: MentionCandidate = {
  id: 'brahma-console',
  name: 'Brahma',
  title: 'the Brahma Console',
};

/**
 * The candidates for a trigger: the characters, plus Brahma when the `@` opens
 * a line — unless a character already answers to that name, in which case the
 * name is on offer already and the Carina service decides who hears it.
 */
export function mentionCandidatesFor<T extends MentionCandidate>(
  characters: readonly T[],
  atLineStart: boolean,
): Array<T | MentionCandidate> {
  if (!atLineStart) return [...characters];
  const brahmaName = BRAHMA_MENTION.name.toLocaleLowerCase();
  if (characters.some((c) => c.name?.trim().toLocaleLowerCase() === brahmaName)) {
    return [...characters];
  }
  return [...characters, BRAHMA_MENTION];
}

/**
 * Filter and order candidates for a query.
 *
 * Characters in `priorityIds` (the current chat's cast) come first, then by
 * match tier, then alphabetically. Case-insensitive throughout. Blank names are
 * never offered.
 */
export function rankMentionCandidates<T extends MentionCandidate>(
  candidates: readonly T[],
  query: string,
  priorityIds: ReadonlySet<string>,
  limit: number,
): T[] {
  const scored: Array<{ candidate: T; priority: number; tier: MatchTier }> = [];
  const seen = new Set<string>();

  for (const candidate of candidates) {
    if (!candidate.name || !candidate.name.trim() || seen.has(candidate.id)) continue;
    const tier = matchTier(candidate.name, query);
    if (tier === null) continue;
    seen.add(candidate.id);
    scored.push({ candidate, priority: priorityIds.has(candidate.id) ? 0 : 1, tier });
  }

  scored.sort(
    (a, b) =>
      a.priority - b.priority ||
      a.tier - b.tier ||
      a.candidate.name.localeCompare(b.candidate.name, undefined, { sensitivity: 'base' }),
  );

  return scored.slice(0, limit).map((entry) => entry.candidate);
}

/**
 * Whether a line-start completion of `name` should keep its `@` pending a
 * verdict — only when `@name:` could actually be parsed as a Carina query
 * (`isCarinaInvocableName`, the parser's own name grammar). `Jean-Luc`, `Zoë`
 * or a one-letter name drop the `@` at once, as they would mid-line.
 */
export function canKeepLineStartAt(name: string): boolean {
  return isCarinaInvocableName(name);
}

/**
 * What to do with a line that began as a completed `@Name`.
 *
 * - `pending` — undecided: nothing typed yet, or only the `:` / `?` separator.
 * - `keep`    — `@Name:` or `@Name?` followed by whitespace: a Carina query.
 * - `strip`   — anything else followed the name, or the name is one the
 *               Carina parser cannot address: drop the `@`.
 * - `abandon` — the line no longer starts with `@Name` (edited, deleted,
 *               undone): leave it alone and stop watching.
 *
 * (v4 prints this block above `canKeepLineStartAt`, where it documents the
 * wrong symbol; it belongs to the verdict type and is placed here.)
 */
export type LineStartMentionVerdict = 'pending' | 'keep' | 'strip' | 'abandon';

export function classifyLineStartMention(line: string, name: string): LineStartMentionVerdict {
  const head = `@${name}`;
  if (!line.startsWith(head)) return 'abandon';
  // A name the Carina parser cannot address never earns a kept `@`.
  if (!canKeepLineStartAt(name)) return 'strip';

  const rest = line.slice(head.length);
  if (rest.length === 0) return 'pending';

  const separator = rest[0];
  if (separator !== ':' && separator !== '?') return 'strip';
  if (rest.length === 1) return 'pending';

  return /\s/.test(rest[1]) ? 'keep' : 'strip';
}
