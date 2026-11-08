import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/conversation-search.oracle.ndjson';
import {
  CONVERSATION_SEARCH_MIN_QUERY,
  conversationQueryPattern,
  escapeRegExp,
  findConversationMatches,
  toSearchableText,
  type SearchableMessage,
} from './conversation-search';

/**
 * The corpus differential for v4 `lib/chat/conversation-search.ts` (P4.D384,
 * v4 `4f5047241`, pin `624d0d25c`). v5's `chat/conversation-search.ts` is a
 * transcription; this replays every row recorded by
 * `apps/web/oracle/conversation-search.recorder.ts` against v4's REAL module and
 * compares the `JSON.stringify` bytes (so key order, lone surrogates in a split
 * snippet, and `null` vs absent all count).
 */

type SpeakerMode = 'absent' | 'null' | 'byId' | 'announcer';

interface ConstantsRow {
  id: string;
  kind: 'constants';
  out: { min: number };
}
interface TextRow {
  id: string;
  kind: 'text';
  input: string;
  out: string;
}
interface EscapeRow {
  id: string;
  kind: 'escape';
  input: string;
  out: string;
}
interface PatternRow {
  id: string;
  kind: 'pattern';
  query: string;
  probe: string;
  out: { source: string; flags: string; probeHit: boolean } | null;
}
interface FindRow {
  id: string;
  kind: 'find';
  messages: SearchableMessage[];
  query: string;
  limit: number;
  speaker: SpeakerMode;
  out: unknown;
}
type Row = ConstantsRow | TextRow | EscapeRow | PatternRow | FindRow;

const ALL: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const of = <K extends Row['kind']>(kind: K) =>
  ALL.filter((r): r is Extract<Row, { kind: K }> => r.kind === kind);

/** The recorder's speaker modes, rebuilt identically (see its header). */
function speakerFn(mode: SpeakerMode): ((m: SearchableMessage) => string | null) | undefined {
  switch (mode) {
    case 'absent':
      return undefined;
    case 'null':
      return () => null;
    case 'byId':
      return (m) => `speaker-${m.id}`;
    case 'announcer':
      return (m) => (m.systemSender ? `Staff ${m.systemSender}` : null);
  }
}

describe("conversation search agrees with v4's lib/chat/conversation-search.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each blocks below vacuously green.
    expect(ALL).toHaveLength(94);
    expect(of('constants')).toHaveLength(1);
    expect(of('text')).toHaveLength(24);
    expect(of('escape')).toHaveLength(6);
    expect(of('pattern')).toHaveLength(19);
    expect(of('find')).toHaveLength(44);
  });

  it('the find corpus discriminates in both directions', () => {
    const finds = of('find');
    const totals = finds.map((r) => (r.out as { total: number }).total);
    expect(totals.filter((t) => t === 0).length).toBeGreaterThanOrEqual(5);
    expect(totals.filter((t) => t > 0).length).toBeGreaterThanOrEqual(25);
  });

  it('the constant', () => {
    expect(JSON.stringify({ min: CONVERSATION_SEARCH_MIN_QUERY })).toBe(
      JSON.stringify(of('constants')[0].out),
    );
  });

  it.each(of('text'))('$id', (row) => {
    expect(JSON.stringify(toSearchableText(row.input))).toBe(JSON.stringify(row.out));
  });

  it.each(of('escape'))('$id', (row) => {
    expect(JSON.stringify(escapeRegExp(row.input))).toBe(JSON.stringify(row.out));
  });

  it.each(of('pattern'))('$id', (row) => {
    const pattern = conversationQueryPattern(row.query);
    let probeHit: boolean | null = null;
    if (pattern) {
      pattern.lastIndex = 0;
      probeHit = pattern.test(row.probe);
    }
    const out = pattern ? { source: pattern.source, flags: pattern.flags, probeHit } : null;
    expect(JSON.stringify(out)).toBe(JSON.stringify(row.out));
  });

  it.each(of('find'))('$id', (row) => {
    const speakerOf = speakerFn(row.speaker);
    const out = findConversationMatches(
      row.messages,
      row.query,
      speakerOf ? { limit: row.limit, speakerOf } : { limit: row.limit },
    );
    expect(JSON.stringify(out)).toBe(JSON.stringify(row.out));
  });
});
