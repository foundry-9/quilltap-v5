import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/established-fact.oracle.ndjson';
import type { MessageDto } from '../core/core-contract';
import { isAnnouncementChip } from './chat-view-model';
import {
  establishedFactKindOf,
  isEstablishedFact,
  isEstablishedFactAnnouncement,
  isEstablishedFactSeatLine,
} from './established-fact';
import {
  getAnnouncementAccentClasses,
  getAnnouncementImportance,
  getSystemKindDisplayLabel,
} from './system-message-labels';

/**
 * The corpus differential for Deus Ex Machina's client rules (v4 `db6190b44`,
 * `671c59a53`): v4's four fact predicates (`lib/chat/established-fact.ts`), the
 * kind label / accent / importance rows (`system-message-labels.ts`) and the
 * never-collapsed rule (`announcement-render-items.ts`), recorded by
 * `apps/web/oracle/established-fact.recorder.ts` against v4's REAL modules at
 * the pin `624d0d25c`.
 *
 * v5 has one predicate where v4 has two: `isAnnouncementChip` is both the
 * grouping rule and the row-rendering rule, and v5 never lifts an expanded set
 * out of the group (the chip stays mounted with its body beneath). So it is
 * compared with v4's collapsed rule with nothing expanded —
 * `isCollapsedAnnouncement(m, new Set())`. The recorder's
 * `isCollapsedWhenExpanded` documents v4's expanded arm and has no v5 twin.
 */

interface Row {
  id: string;
  message: Record<string, unknown>;
  out: {
    isEstablishedFactAnnouncement: boolean;
    isEstablishedFactSeatLine: boolean;
    isEstablishedFact: boolean;
    establishedFactKindOf: 'seat' | 'announcement' | null;
    isAlwaysExpandedAnnouncement: boolean;
    isCollapsedAnnouncement: boolean;
    isCollapsedWhenExpanded: boolean;
    kindLabel: string;
    accent: string;
    importance: string;
  };
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe('established facts agree with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(207);
    expect(ROWS.filter((r) => r.id.startsWith('sender '))).toHaveLength(72);
    expect(ROWS.filter((r) => r.id.startsWith('flag '))).toHaveLength(135);
  });

  it.each(ROWS)('$id', (row) => {
    const m = row.message as unknown as MessageDto;
    const out = {
      isEstablishedFactAnnouncement: isEstablishedFactAnnouncement(m),
      isEstablishedFactSeatLine: isEstablishedFactSeatLine(m),
      isEstablishedFact: isEstablishedFact(m),
      establishedFactKindOf: establishedFactKindOf(m),
      isAnnouncementChip: isAnnouncementChip(m),
      kindLabel: getSystemKindDisplayLabel(m),
      accent: getAnnouncementAccentClasses(m),
      importance: getAnnouncementImportance(m),
    };
    expect(out).toEqual({
      isEstablishedFactAnnouncement: row.out.isEstablishedFactAnnouncement,
      isEstablishedFactSeatLine: row.out.isEstablishedFactSeatLine,
      isEstablishedFact: row.out.isEstablishedFact,
      establishedFactKindOf: row.out.establishedFactKindOf,
      isAnnouncementChip: row.out.isCollapsedAnnouncement,
      kindLabel: row.out.kindLabel,
      accent: row.out.accent,
      importance: row.out.importance,
    });
  });
});
