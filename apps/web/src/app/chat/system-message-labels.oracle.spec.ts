import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/announcement-sender-name.oracle.ndjson';
import type { MessageDto } from '../core/core-contract';
import { getAnnouncementSenderName } from './system-message-labels';

/**
 * The corpus differential for v4's `getAnnouncementSenderName`
 * (`app/salon/[id]/components/system-message-labels.ts`, `a05c6f755` —
 * P4.D320): the name on a Staff message's bar when the row may also carry a
 * `customAnnouncer`. Recorded by
 * `apps/web/oracle/announcement-sender-name.recorder.ts` against v4's REAL
 * function, including the Staff fallback over every `systemSender` this app
 * knows (so a staff-name drift trips here too).
 */

interface Row {
  id: string;
  message: Pick<MessageDto, 'systemSender' | 'customAnnouncer'>;
  /** False when the recorder passed no second argument at all. */
  withNames: boolean;
  names: Record<string, string> | null;
  out: string;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe('getAnnouncementSenderName agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(32);
    expect(ROWS.filter((r) => r.id.startsWith('staff-'))).toHaveLength(12);
  });

  it.each(ROWS)('$id', (row) => {
    const out = row.withNames
      ? getAnnouncementSenderName(row.message, row.names ?? undefined)
      : getAnnouncementSenderName(row.message);
    expect(out).toBe(row.out);
  });
});
