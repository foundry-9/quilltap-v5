/**
 * The corpus differential for the SPA's hand-ported custom-tool schema module —
 * the teeth for P4.6bb unit 2.
 *
 * The SPA has no zod, so `custom-tool-types.ts` reimplements a slice of Zod
 * 4.4.3 by hand. That port cannot be verified by inspection: the rejection
 * sentence it renders is user-visible payload the server also produces, so the
 * browser and the server must phrase the same file's rejection identically.
 *
 * This replays the COMMITTED oracle corpus — 175 rows generated from v4's REAL
 * `QtapCustomToolSchema` at `7e6d13e5` (see
 * `src/testing/fixtures/README.md` for provenance and the regen recipe) — and
 * byte-compares four things per row:
 *
 *   1. the accept/reject verdict,
 *   2. the parsed data, via `JSON.stringify` (so key ORDER and omitted
 *      optionals are pinned, not just field values),
 *   3. the `collectUnknownKeys` report,
 *   4. the FULL `formatDefinitionIssues` sentence — Zod's own built-in messages
 *      and their ORDER included, since the join is `'; '`.
 *
 * It is the same bar the Rust `pascal_custom_tool_definition_equivalence`
 * differential holds, over the same bytes.
 */

import { describe, expect, it } from 'vitest';

// `.ndjson` is registered as a `text` loader in angular.json's build options, so
// the corpus is inlined VERBATIM at build time — the fixture stays a byte-exact
// copy of the oracle output rather than something a loader reshaped on the way
// in. (Vite's `?raw` works under a bare `vitest run` but not through the Angular
// builder's esbuild, which wants the extension declared.)
import corpusText from '../../testing/fixtures/pascal-custom-tool-definition.oracle.ndjson';
import {
  collectUnknownKeys,
  displayTitle,
  formatDefinitionIssues,
  safeParse,
} from './custom-tool-types';

interface TitleRow {
  kind: 'title';
  id: string;
  input: { name: string; title?: string };
  out: string;
}

interface DefinitionRow {
  kind: 'definition';
  id: string;
  /**
   * The definition's BYTES — never a re-stringified structure. Both sides parse
   * this with their own JSON parser, exactly as `readToolFile` does. (A
   * structured field would be a lie for the cases worth testing: an `Infinity`
   * operand stringifies to `null`.)
   */
  inputJson: string;
  success: boolean;
  reason: string | null;
  data: string | null;
  unknownKeys: string[];
}

type Row = TitleRow | DefinitionRow;

const rows: Row[] = corpusText
  .split('\n')
  .filter((line) => line.trim().length > 0)
  .map((line) => JSON.parse(line) as Row);

const titleRows = rows.filter((r): r is TitleRow => r.kind === 'title');
const definitionRows = rows.filter((r): r is DefinitionRow => r.kind === 'definition');

/**
 * The corpus is generated at v4 `7e6d13e5` (lane D10's §C regen, 175 rows —
 * the `$state` schema families joined at the state-cascade round). The map is
 * empty — every row passes against the fixture's own bytes. The map and its
 * guard stay as the mechanism for the NEXT drift window (fill it only with
 * replay-verified sentences, and empty it again when the corpus regenerates).
 */
const REGENERATED_AT_7E6D13E5: Record<string, string> = {};

/** The sentence v4 `7e6d13e5` renders for a row — the fixture's, unless drifted. */
function expectedReason(row: DefinitionRow): string | null {
  return REGENERATED_AT_7E6D13E5[row.id] ?? row.reason;
}

describe('custom-tool schema — the committed v4 corpus', () => {
  it('the corpus is the expected shape (a truncated fixture must not pass silently)', () => {
    // D10's recorded §C counts at `7e6d13e5`: 175 = 10 title + 165 definition
    // (58 accept / 107 reject).
    expect(rows.length).toBe(175);
    expect(titleRows.length).toBe(10);
    expect(definitionRows.length).toBe(165);
  });

  it('the drift map names only rows the fixture actually carries', () => {
    // Guards the map against outliving its rows: a regenerated corpus empties
    // the map, and a stale key here would pass unnoticed.
    const ids = new Set(definitionRows.map((r) => r.id));
    for (const id of Object.keys(REGENERATED_AT_7E6D13E5)) {
      expect(ids.has(id), `drift map names "${id}", which the corpus does not`).toBe(true);
    }
  });

  describe('displayTitle', () => {
    for (const row of titleRows) {
      it(`matches v4 — ${row.id}`, () => {
        expect(displayTitle({ name: row.input.name, title: row.input.title })).toBe(row.out);
      });
    }
  });

  describe('safeParse', () => {
    for (const row of definitionRows) {
      it(`matches v4 — ${row.id}`, () => {
        const raw: unknown = JSON.parse(row.inputJson);

        expect(collectUnknownKeys(raw)).toEqual(row.unknownKeys);

        const result = safeParse(raw);

        if (result.success) {
          // v4 accepted too, and the parsed shape agrees byte for byte —
          // omitted optionals and key order included.
          expect(row.success, `v5 accepted; v4 rejected with: ${row.reason}`).toBe(true);
          expect(JSON.stringify(result.data)).toBe(row.data);
        } else {
          const sentence = formatDefinitionIssues(result.issues);
          expect(row.success, `v5 rejected with: ${sentence}; v4 accepted`).toBe(false);
          expect(sentence).toBe(expectedReason(row));
        }
      });
    }
  });
});
