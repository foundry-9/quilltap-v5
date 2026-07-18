/**
 * The corpus differential for the SPA's hand-ported custom-tool schema module —
 * the teeth for P4.6bb unit 2.
 *
 * The SPA has no zod, so `custom-tool-types.ts` reimplements a slice of Zod
 * 4.4.3 by hand. That port cannot be verified by inspection: the rejection
 * sentence it renders is user-visible payload the server also produces, so the
 * browser and the server must phrase the same file's rejection identically.
 *
 * This replays the COMMITTED oracle corpus — 115 rows generated from v4's REAL
 * `QtapCustomToolSchema` at `d68638b4` (see
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
 * ⚠ The committed corpus was generated at v4 `d68638b4`; this lane ports v4
 * `616930db`, and FOUR of its rows changed message there — verified by
 * replaying the committed corpus against v4's real Zod at `616930db`, where
 * exactly these four (and no others) drifted:
 *
 *  - three `when` refine rows gained `` `llm` `` in the at-least-one list;
 *  - one metadata comparator row widened to the eight-key list.
 *
 * This is v4 drift, not a v5 porting bug, and the port must follow v4 (D23).
 * Lane D8 owns the regenerated NDJSON (§C), so the correction lives here until
 * unification rather than in the fixture.
 *
 * §C: the unifier REPLACES this map with `{}` (and the counts below with D8's
 * recorded counts) once the regenerated corpus lands — every row must then pass
 * against the fixture's own bytes.
 */
const REGENERATED_AT_616930DB: Record<string, string> = {
  'when-tests-nothing':
    'outcomes.0.when: must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, or a non-empty `metadata`',
  'when-empty-params':
    'outcomes.0.when: must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, or a non-empty `metadata`',
  'metadata-empty-object':
    'outcomes.0.when: must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, or a non-empty `metadata`',
  'metadata-empty-comparator':
    'outcomes.0.when.metadata.faction: must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains)',
};

/** The sentence v4 `616930db` renders for a row — the fixture's, unless drifted. */
function expectedReason(row: DefinitionRow): string | null {
  return REGENERATED_AT_616930DB[row.id] ?? row.reason;
}

describe('custom-tool schema — the committed v4 corpus', () => {
  it('the corpus is the expected shape (a truncated fixture must not pass silently)', () => {
    // §C: unifier updates to D8's recorded counts
    expect(rows.length).toBe(115);
    expect(titleRows.length).toBe(10);
    expect(definitionRows.length).toBe(105);
  });

  it('the drift map names only rows the fixture actually carries', () => {
    // Guards the map against outliving its rows: once D8's regenerated corpus
    // lands the map goes to `{}`, and a stale key here would pass unnoticed.
    const ids = new Set(definitionRows.map((r) => r.id));
    for (const id of Object.keys(REGENERATED_AT_616930DB)) {
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
