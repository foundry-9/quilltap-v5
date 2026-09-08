/**
 * The corpus differential for the SPA's hand-ported progression schema.
 *
 * `schema.ts` reimplements the slice of Zod 4.5.4 that v4's `ProgressionSchema`
 * uses, because the SPA has no `zod` (measured 2026-09-08 — the work order's
 * "the SPA HAS zod" premise is refuted; see that module's header). That port
 * cannot be verified by inspection: `progression-editor-modal.ts` renders
 * `${issue.path.join('.') || 'This entry'}: ${issue.message}` straight to the
 * author, so a browser phrasing a rejection differently would be disagreeing
 * with the server about the same entry.
 *
 * This replays the committed 71-row corpus recorded from v4's REAL
 * `lib/progressions/schema.ts` at `25f534c0b` and byte-compares three things
 * per row: the verdict, the WHOLE joined rejection sentence (paths and their
 * order included, since the join is `'; '`), and — on an accept —
 * `JSON.stringify` of the parsed data, so the declaration key order and the
 * omitted optionals are pinned as well as the values.
 *
 * Provenance + regen recipe: `apps/web/oracle/progressions-schema.recorder.ts`
 * and `src/testing/fixtures/README.md`.
 *
 * The one arm the corpus cannot carry is noted below: `NaN` has no JSON
 * literal, so it cannot reach a validator from a real `metadata.json`.
 */

import { describe, expect, it } from 'vitest';

// `.ndjson` is registered as a `text` loader in angular.json's build options, so
// the corpus is inlined VERBATIM at build time (the `pascal-custom-tool-
// definition` precedent — `?raw` works under a bare vitest but not through the
// Angular builder's esbuild).
import corpusText from '../../testing/fixtures/progressions-schema.oracle.ndjson';
import { safeParseProgression, safeParseProgressions } from './schema';

interface Row {
  kind: 'progression' | 'progressions';
  id: string;
  inputJson: string;
  success: boolean;
  reason: string | null;
  data: string | null;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

/** The SPA's own rendering of a rejection, joined exactly as the recorder joins v4's. */
function sentence(issues: readonly { path: string[]; message: string }[]): string {
  return issues.map((i) => `${i.path.join('.') || '(root)'}: ${i.message}`).join('; ');
}

describe('the progression schema agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every `it.each` below vacuously green.
    expect(ROWS).toHaveLength(71);
    expect(ROWS.filter((r) => r.kind === 'progression')).toHaveLength(62);
    expect(ROWS.filter((r) => r.kind === 'progressions')).toHaveLength(9);
  });

  it.each(ROWS.filter((r) => r.kind === 'progression'))('$id', (row) => {
    const result = safeParseProgression(JSON.parse(row.inputJson));
    expect(result.success).toBe(row.success);
    if (result.success) {
      expect(JSON.stringify(result.data)).toBe(row.data);
    } else {
      expect(sentence(result.issues)).toBe(row.reason);
    }
  });

  it.each(ROWS.filter((r) => r.kind === 'progressions'))('$id', (row) => {
    const result = safeParseProgressions(JSON.parse(row.inputJson));
    expect(result.success).toBe(row.success);
    if (!result.success) expect(sentence(result.issues)).toBe(row.reason);
  });
});

describe('the one arm the corpus cannot express', () => {
  /**
   * JSON has no `NaN` literal, so this is unreachable from a real vault file
   * and the recorder leaves it out. The sentence below was MEASURED against
   * v4's real schema at `25f534c0b` (`quantity: { total: NaN, unit: 'MJ' }` →
   * `quantity.total: Invalid input: expected number, received NaN`); Zod v4's
   * `z.number()` refuses `NaN` at the TYPE check and names it in `received`
   * (`zod/v4/core/schemas.js:586`).
   */
  it('names NaN in a numeric type failure, exactly as Zod does', () => {
    const result = safeParseProgression({
      name: 'C',
      startTime: '2026-09-08T14:02:10Z',
      endTime: '2026-09-08T14:12:10Z',
      timeIncrement: 'minute',
      quantity: { total: NaN, unit: 'MJ' },
    });
    expect(result.success).toBe(false);
    if (result.success) return;
    expect(sentence(result.issues)).toBe(
      'quantity.total: Invalid input: expected number, received NaN',
    );
  });
});
