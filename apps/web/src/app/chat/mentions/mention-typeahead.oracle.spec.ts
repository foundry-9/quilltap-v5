import { describe, expect, it } from 'vitest';

import corpusText from '../../../testing/fixtures/mention-typeahead.ndjson';
import type { TriggerMatch } from '../../editor/char-insert/types';
import {
  BRAHMA_MENTION,
  MENTION_TRIGGER,
  canKeepLineStartAt,
  classifyLineStartMention,
  findMentionTrigger,
  mentionCandidatesFor,
  rankMentionCandidates,
  type MentionCandidate,
} from './mention-typeahead';

/**
 * The corpus differential for v4 `lib/mentions/mention-typeahead.ts`
 * (`3376b3dfa`, the composer's `@` typeahead). Recorded by
 * `apps/web/oracle/mention-typeahead.recorder.ts` against v4's REAL module at
 * `b0b6656b5` (see that file's header for the pinned-worktree invocation): v4's
 * own vectors plus a wider corpus, every exported function, compared exactly —
 * trigger matches whole, rankings as the ordered id list, candidate lists whole,
 * verdicts as strings.
 *
 * `localeCompare` / `toLocaleLowerCase` are the runtime's ICU on both sides (the
 * recorder runs under Node; this spec runs under Node's vitest), exactly as v4's
 * client uses the browser's.
 */

interface Base {
  id: string;
}
interface ConstantsRow extends Base {
  kind: 'constants';
  trigger: Record<string, unknown>;
  brahma: MentionCandidate;
}
interface TriggerRow extends Base {
  kind: 'trigger';
  text: string;
  out: TriggerMatch | null;
}
interface RankRow extends Base {
  kind: 'rank';
  candidates: MentionCandidate[];
  query: string;
  priority: string[];
  limit: number;
  out: string[];
}
interface CandidatesRow extends Base {
  kind: 'candidates';
  characters: MentionCandidate[];
  atLineStart: boolean;
  out: MentionCandidate[];
}
interface ClassifyRow extends Base {
  kind: 'classify';
  line: string;
  name: string;
  out: string;
}
interface CanKeepRow extends Base {
  kind: 'canKeep';
  name: string;
  out: boolean;
}
type Row = ConstantsRow | TriggerRow | RankRow | CandidatesRow | ClassifyRow | CanKeepRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

function ofKind<K extends Row['kind']>(kind: K): Array<Extract<Row, { kind: K }>> {
  return ROWS.filter((row): row is Extract<Row, { kind: K }> => row.kind === kind);
}

describe("mention-typeahead agrees with v4's lib/mentions/mention-typeahead.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every it.each below vacuously green.
    expect(ROWS).toHaveLength(116);
    expect(ofKind('trigger')).toHaveLength(39);
    expect(ofKind('rank')).toHaveLength(27);
    expect(ofKind('candidates')).toHaveLength(9);
    expect(ofKind('classify')).toHaveLength(34);
    expect(ofKind('canKeep')).toHaveLength(6);
  });

  it('the corpus discriminates: open and shut triggers, all four verdicts', () => {
    const triggers = ofKind('trigger');
    expect(triggers.filter((r) => r.out === null).length).toBeGreaterThanOrEqual(10);
    expect(triggers.filter((r) => r.out !== null).length).toBeGreaterThanOrEqual(10);
    for (const verdict of ['pending', 'keep', 'strip', 'abandon']) {
      expect(ofKind('classify').some((r) => r.out === verdict)).toBe(true);
    }
  });

  it('the trigger config and the Brahma entry are v4s', () => {
    const [row] = ofKind('constants');
    expect({
      opener: MENTION_TRIGGER.opener,
      queryPattern: {
        source: MENTION_TRIGGER.queryPattern.source,
        flags: MENTION_TRIGGER.queryPattern.flags,
      },
      minQueryLength: MENTION_TRIGGER.minQueryLength,
      maxQueryLength: MENTION_TRIGGER.maxQueryLength,
      closer: MENTION_TRIGGER.closer,
      lowercaseQuery: MENTION_TRIGGER.lowercaseQuery,
      queryStartPattern: MENTION_TRIGGER.queryStartPattern ?? null,
    }).toEqual(row.trigger);
    expect(BRAHMA_MENTION).toEqual(row.brahma);
  });

  it.each(ofKind('trigger'))('$id', (row) => {
    expect(findMentionTrigger(row.text)).toEqual(row.out);
  });

  it.each(ofKind('rank'))('$id', (row) => {
    const ranked = rankMentionCandidates(row.candidates, row.query, new Set(row.priority), row.limit);
    expect(ranked.map((c) => c.id)).toEqual(row.out);
  });

  it.each(ofKind('candidates'))('$id', (row) => {
    expect(mentionCandidatesFor(row.characters, row.atLineStart)).toEqual(row.out);
  });

  it.each(ofKind('classify'))('$id', (row) => {
    expect(classifyLineStartMention(row.line, row.name)).toBe(row.out);
  });

  it.each(ofKind('canKeep'))('$id', (row) => {
    expect(canKeepLineStartAt(row.name)).toBe(row.out);
  });
});
