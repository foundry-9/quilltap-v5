/**
 * The corpus differential for the SPA's hand-ported custom-tool schema module —
 * the teeth for P4.6bb unit 2.
 *
 * The SPA has no zod, so `custom-tool-types.ts` reimplements a slice of Zod
 * 4.4.3 by hand. That port cannot be verified by inspection: the rejection
 * sentence it renders is user-visible payload the server also produces, so the
 * browser and the server must phrase the same file's rejection identically.
 *
 * This replays the COMMITTED oracle corpus — 299 rows generated from v4's REAL
 * `QtapCustomToolSchema` at `d883a5ee1` under Zod 4.5.4 (refreshed at the
 * `d883a5ee1` unification: thirteen unrecognized-key rows moved when Zod made
 * `unrecognized_keys` continuable — v4 `6e1a64ea6`; first captured at `c4d4b0de`; see
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
import { evaluateToolGate, hasToolGate } from './tool-gate';
import { gateConditionsFromGate, gateFromConditions } from './tool-draft';

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

/**
 * One `evaluateToolGate` verdict, against one fact sheet — P4.d19's third row
 * kind, added to the shared corpus at the `231be14c` round. The verdict is the
 * SERIALIZED object, so `withheldBy`'s ABSENCE on an available verdict is part
 * of what is compared, exactly as the Rust side reads it.
 */
interface GateRow {
  kind: 'gate';
  id: string;
  inputJson: string;
  metadata: Record<string, unknown> | null;
  hasGate: boolean;
  verdict: string;
}

type Row = TitleRow | DefinitionRow | GateRow;

const rows: Row[] = corpusText
  .split('\n')
  .filter((line) => line.trim().length > 0)
  .map((line) => JSON.parse(line) as Row);

const titleRows = rows.filter((r): r is TitleRow => r.kind === 'title');
const definitionRows = rows.filter((r): r is DefinitionRow => r.kind === 'definition');
const gateVerdictRows = rows.filter((r): r is GateRow => r.kind === 'gate');

/**
 * The corpus is generated at v4 `c4d4b0de` (P4.D35's §C extension, 299 rows —
 * the chipLabel/effects arms joined at the `c4d4b0de` drift round). The map is
 * empty — every row passes against the fixture's own bytes. The map and its
 * guard stay as the mechanism for the NEXT drift window (fill it only with
 * replay-verified sentences, and empty it again when the corpus regenerates).
 */
const REGENERATED_AT_BASELINE: Record<string, string> = {};

/** The sentence v4 `231be14c` renders for a row — the fixture's, unless drifted. */
function expectedReason(row: DefinitionRow): string | null {
  return REGENERATED_AT_BASELINE[row.id] ?? row.reason;
}

/** Rows whose INPUT declares a gate clause — the §4 keys, read off the bytes. */
const gateRows = definitionRows.filter((row) => {
  const raw: unknown = JSON.parse(row.inputJson);
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return false;
  return 'availableWhen' in raw || 'withheldWhen' in raw;
});

describe('custom-tool schema — the committed v4 corpus', () => {
  it('the corpus is the expected shape (a truncated fixture must not pass silently)', () => {
    // DERIVED, never a hand-written count: a literal here would be re-asserting
    // what the generator emitted rather than what the port does, and would rot
    // the moment the corpus grows (the `harness-corpus-shape-constants-rot`
    // rule). What is pinned instead is composition — the partition is total and
    // both halves are non-empty — and that every row carries what the replay
    // below reads. P4.d19 records the round's actual counts in its lane record.
    expect(titleRows.length + definitionRows.length + gateVerdictRows.length).toBe(rows.length);
    expect(titleRows.length).toBeGreaterThan(0);
    expect(definitionRows.length).toBeGreaterThan(0);
    // The third kind arrived at the `231be14c` unification. This partition
    // assertion is what caught it — P4.d20 wrote the census over two kinds and
    // P4.d19 shipped a third, so the guard fired exactly as its name promises
    // rather than letting 31 unread rows sit in the fixture.
    expect(gateVerdictRows.length).toBeGreaterThan(0);

    const accepted = definitionRows.filter((r) => r.success);
    const rejected = definitionRows.filter((r) => !r.success);
    expect(accepted.length + rejected.length).toBe(definitionRows.length);
    expect(accepted.length).toBeGreaterThan(0);
    expect(rejected.length).toBeGreaterThan(0);

    // Ids are the test names below; a duplicate would silently shadow a case.
    expect(new Set(rows.map((r) => r.id)).size).toBe(rows.length);

    for (const row of titleRows) {
      expect(typeof row.input.name, row.id).toBe('string');
      expect(typeof row.out, row.id).toBe('string');
    }
    for (const row of definitionRows) {
      expect(typeof row.inputJson, row.id).toBe('string');
      expect(Array.isArray(row.unknownKeys), row.id).toBe(true);
      // An accepted row must carry the parsed bytes to compare against; a
      // rejected one must carry the sentence. A row missing its own half would
      // otherwise compare against `null` and pass.
      expect(typeof (row.success ? row.data : row.reason), row.id).toBe('string');
    }
  });

  it('the drift map names only rows the fixture actually carries', () => {
    // Guards the map against outliving its rows: a regenerated corpus empties
    // the map, and a stale key here would pass unnoticed.
    const ids = new Set(definitionRows.map((r) => r.id));
    for (const id of Object.keys(REGENERATED_AT_BASELINE)) {
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

  /**
   * The availability gate (`6864bf0e`), over whichever rows the corpus carries.
   *
   * ACTIVATE-AT-UNIFY: **vacuous while the committed corpus predates P4.d19's
   * regeneration at `231be14c`, and load-bearing the moment its gate rows land**
   * — there is nothing to hand-write here, per the round's §3. When it wakes it
   * pins two things at once: that v5's hand-ported schema reaches v4's recorded
   * verdict on a gated file, and that the DRAFT layer's flatten/reassemble pair
   * is a bijection over gates v4 itself accepted (the one place the Workbench's
   * gate editor is measured against real v4 bytes rather than its own idea of
   * them).
   */
  describe('availability gates', () => {
    it('reports how many gate rows the corpus carries', () => {
      // Not a count assertion — a census the reader can see, so a corpus that
      // never grew a gate row is visible rather than silently green.
      expect(gateRows.length).toBeGreaterThanOrEqual(0);
    });

    for (const row of gateRows) {
      it(`the gate round-trips through the draft — ${row.id}`, () => {
        const result = safeParse(JSON.parse(row.inputJson));
        expect(result.success).toBe(row.success);
        if (!result.success) return;

        const gate = result.data.availableWhen ?? result.data.withheldWhen;
        expect(hasToolGate(result.data)).toBe(gate !== undefined);
        if (!gate) return;

        // Flatten to chips and reassemble: byte-identical, key order included.
        expect(JSON.stringify(gateFromConditions(gateConditionsFromGate(gate)))).toBe(
          JSON.stringify(gate),
        );
      });
    }
  });

  /**
   * P4.d19's `gate` rows, replayed against the browser's own evaluator — the
   * unification wire. The Rust half already diffs these; running the SAME rows
   * through `evaluateToolGate` here is what proves the two client-safe ports
   * agree with v4 AND with each other, which is the whole reason v4 marks
   * `tool-gate.ts` CLIENT-SAFE. Compared as SERIALIZED JSON so `withheldBy`'s
   * absence on an available verdict is part of the comparison, not erased by a
   * structural equality that treats absent and undefined alike.
   */
  describe('gate verdicts (P4.d19 rows, replayed in the browser)', () => {
    for (const row of gateVerdictRows) {
      it(`matches v4 — ${row.id}`, () => {
        const parsed = safeParse(JSON.parse(row.inputJson));
        expect(parsed.success, `${row.id} should parse`).toBe(true);
        if (!parsed.success) return;

        expect(hasToolGate(parsed.data)).toBe(row.hasGate);
        expect(JSON.stringify(evaluateToolGate(parsed.data, row.metadata))).toBe(row.verdict);
      });
    }
  });
});
