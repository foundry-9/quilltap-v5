import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/message-navigation.oracle.ndjson';
import { revealRequestFromSalonHref, type MessageRevealRequest } from './message-navigation';

/**
 * The corpus differential for v4 `lib/chat/message-navigation.ts`'s
 * `revealRequestFromSalonHref` (P4.D384, v4 `4f5047241`, pinned at
 * `624d0d25c`). Recorded by `apps/web/oracle/message-navigation.recorder.ts`
 * against v4's REAL module (see that file's header for the pinned-worktree
 * invocation). A row carries `out` (the request or `null`) or `threw` (v4's
 * `decodeURIComponent` refusing a malformed escape in the chat id — v4 does not
 * catch it, so neither does the twin).
 */

interface Row {
  id: string;
  href: string;
  out?: MessageRevealRequest | null;
  threw?: string;
}

/** The recorder's `HREFS.length`. */
const EXPECTED_ROWS = 42;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

describe("revealRequestFromSalonHref agrees with v4's lib/chat/message-navigation.ts row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS.length).toBe(EXPECTED_ROWS);
    expect(ROWS.filter((r) => 'threw' in r).length).toBe(1);
  });

  it.each(ROWS)('$id', (row) => {
    if (row.threw !== undefined) {
      let got: string | null = null;
      try {
        revealRequestFromSalonHref(row.href);
      } catch (error) {
        const e = error as Error;
        got = `${e.name}: ${e.message}`;
      }
      expect(got).toBe(row.threw);
      return;
    }
    expect(JSON.stringify(revealRequestFromSalonHref(row.href))).toBe(JSON.stringify(row.out));
  });
});
