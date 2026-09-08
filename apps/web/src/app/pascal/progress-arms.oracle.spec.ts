/**
 * The corpus differential for Pascal's `progress` family, on the browser side.
 *
 * `0587d1e96` gave the format a `progress` subject in a `when` and in a gate
 * (beside a now-OPTIONAL `metadata`), a third `progress.<id>.<field>` effect
 * target family, a reserved-key refusal on the `metadata.` family, `{{now}}`
 * and `{{progress.…}}` placeholders, and a third argument to
 * `evaluateToolGate`. The SPA's hand-ported twins have to phrase every one of
 * those rejections exactly as the server does — the Workbench renders
 * `formatDefinitionIssues` verbatim for a draft the author is repairing, and
 * the roster route returns it as a load error's `reason`.
 *
 * The SHARED corpus (`pascal-custom-tool-definition.oracle.ndjson`, generated
 * by the Pascal lane's `harness/oracle/cases/` case) carries the rows whose
 * sentences MOVED with this drift, and it was re-recorded at `25f534c0b`; these
 * are the rows that did not exist before, recorded by this lane's own
 * `apps/web/oracle/pascal-progress.recorder.ts` so the hunks are pinned without
 * editing another lane's generator.
 *
 * Four row kinds, all byte-compared:
 *
 *   1. `definition` — the whole `formatDefinitionIssues` sentence, or the
 *      parsed data's `JSON.stringify` on an accept (so key ORDER is pinned).
 *   2. `effect-target` — `parseEffectTarget`'s whole serialized result, so the
 *      parsed target's SHAPE is compared and not only its verdict.
 *   3. `placeholder` — `classifyPlaceholder`'s serialized ref.
 *   4. `gate-eval` — `evaluateToolGate`'s serialized verdict against a sheet
 *      derived by `flattenProgressions`, so `withheldBy`'s ABSENCE on an
 *      available verdict is part of the comparison.
 */

import { describe, expect, it } from 'vitest';

import { flattenProgressions } from '../progressions/engine';
import corpusText from '../../testing/fixtures/pascal-progress.oracle.ndjson';
import { formatDefinitionIssues, parseEffectTarget, safeParse } from './custom-tool-types';
import { gateConditionsFromGate, gateFromConditions } from './tool-draft';
import { classifyPlaceholder } from './placeholders';
import { evaluateToolGate } from './tool-gate';
import type { QtapCustomTool } from './custom-tool-types';

interface DefinitionRow {
  kind: 'definition';
  id: string;
  inputJson: string;
  success: boolean;
  reason: string | null;
  data: string | null;
}
interface TargetRow {
  kind: 'effect-target';
  id: string;
  out: string;
}
interface PlaceholderRow {
  kind: 'placeholder';
  id: string;
  out: string;
}
interface GateEvalRow {
  kind: 'gate-eval';
  id: string;
  definitionJson: string;
  withSheet: boolean;
  sheet: string;
  out: string;
}

type Row = DefinitionRow | TargetRow | PlaceholderRow | GateEvalRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const of = <T extends Row['kind']>(kind: T) =>
  ROWS.filter((r): r is Extract<Row, { kind: T }> => r.kind === kind);

describe("Pascal's progress family agrees with v4 row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every `it.each` below vacuously green.
    expect(ROWS).toHaveLength(71);
    expect(of('definition')).toHaveLength(38);
    expect(of('effect-target')).toHaveLength(16);
    expect(of('placeholder')).toHaveLength(10);
    expect(of('gate-eval')).toHaveLength(7);
  });

  it.each(of('definition'))('definition — $id', (row) => {
    const result = safeParse(JSON.parse(row.inputJson));
    expect(result.success).toBe(row.success);
    if (result.success) {
      expect(JSON.stringify(result.data)).toBe(row.data);
    } else {
      expect(formatDefinitionIssues(result.issues)).toBe(row.reason);
    }
  });

  it.each(of('effect-target'))('effect target — $id', (row) => {
    expect(JSON.stringify(parseEffectTarget(row.id))).toBe(row.out);
  });

  it.each(of('placeholder'))('placeholder — $id', (row) => {
    expect(JSON.stringify(classifyPlaceholder(row.id))).toBe(row.out);
  });

  it.each(of('gate-eval'))('gate verdict — $id', (row) => {
    // The sheet is DERIVED here rather than read from the row, so this arm is
    // also a cross-check that `flattenProgressions` produced the same bytes v4's
    // engine did before the gate ever saw them.
    const sheet = flattenProgressions(
      {
        progressions: {
          cannon: {
            name: 'Cannon recharge',
            startTime: '2026-09-08T14:00:00Z',
            endTime: '2026-09-08T14:10:00Z',
            timeIncrement: 'minute',
          },
        },
      },
      Date.parse('2026-09-08T14:05:00Z'),
    );
    expect(JSON.stringify(sheet)).toBe(row.sheet);

    const definition = JSON.parse(row.definitionJson) as Pick<
      QtapCustomTool,
      'availableWhen' | 'withheldWhen'
    >;
    const verdict = evaluateToolGate(definition, { rank: 'captain' }, row.withSheet ? sheet : null);
    expect(JSON.stringify(verdict)).toBe(row.out);
  });
});

/**
 * The Workbench round trip, over the gate rows the shared corpus cannot reach
 * yet. `gateConditionsFromGate` must emit `metadata` chips BEFORE `progress`
 * ones so reassembly is stable, and `gateFromConditions` must omit a subject
 * with nothing under it rather than writing an empty object the schema would
 * then refuse.
 */
describe('a gate round-trips through the Workbench chips', () => {
  const gates = of('definition')
    .filter((r) => r.success && r.id.startsWith('gate-'))
    .map((r) => {
      const data = JSON.parse(r.data!) as {
        availableWhen?: Record<string, unknown>;
        withheldWhen?: Record<string, unknown>;
      };
      return { id: r.id, gate: (data.availableWhen ?? data.withheldWhen)! };
    });

  it('finds the accepted gate rows', () => {
    expect(gates).toHaveLength(5);
  });

  it.each(gates.filter((g) => Object.values(g.gate).every((v) => Object.keys(v as object).length)))(
    'is byte-identical for $id',
    ({ gate }) => {
      expect(JSON.stringify(gateFromConditions(gateConditionsFromGate(gate)))).toBe(
        JSON.stringify(gate),
      );
    },
  );

  /**
   * v4's own normalization, asserted rather than tolerated: a subject that
   * parsed as `{}` contributes no chips, so reassembly drops the key. The
   * result is still a gate the schema accepts — the object-level refine counts
   * the SUM — and it is what an author who opened and re-saved the file would
   * get from v4 too.
   */
  it.each(gates.filter((g) => Object.values(g.gate).some((v) => !Object.keys(v as object).length)))(
    'drops the empty subject for $id',
    ({ gate }) => {
      const round = gateFromConditions(gateConditionsFromGate(gate)) as Record<string, unknown>;
      const nonEmpty = Object.fromEntries(
        Object.entries(gate).filter(([, v]) => Object.keys(v as object).length > 0),
      );
      expect(JSON.stringify(round)).toBe(JSON.stringify(nonEmpty));
    },
  );
});
