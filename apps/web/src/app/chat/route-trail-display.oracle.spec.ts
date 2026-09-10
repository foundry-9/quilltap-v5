/**
 * The corpus differential for `route-trail-display.ts` against v4's REAL
 * `lib/chat/route-trail-display.ts`, recorded at the `78b381a96` pin by
 * `apps/web/oracle/route-trail-display.ts` (P4.D177 §C.1 — the feature was
 * shipped whole in `5841a8c62`, so there is no prior baseline to diff from).
 *
 * Also carries the ground truth for the hand-rolled `RouteAttemptVia` /
 * `RouteAttemptOutcome` / `RouteAttemptTrigger` unions in `core-contract.ts`
 * — v5's SPA has no zod (`spa-has-no-zod-schema-twins-are-hand-rolled`), so
 * the three enum rows are the only proof those seven-literal unions agree
 * with v4's `lib/schemas/chat.types.ts`.
 */

import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/route-trail-display.oracle.ndjson';
import type { RouteAttempt, RouteAttemptOutcome, RouteAttemptTrigger, RouteAttemptVia } from '../core/core-contract';
import {
  ROUTE_OUTCOME_GLYPH,
  collapseRouteTrail,
  describeRouteAttempt,
  routeOutcomeLabel,
  type RouteTrailRow,
} from './route-trail-display';

interface CollapseRow {
  kind: 'collapse';
  id: string;
  trailJson: string;
  out: string;
}
interface DescribeRow {
  kind: 'describe';
  id: string;
  rowJson: string;
  out: string;
}
interface GlyphRow {
  kind: 'glyph';
  id: string;
  out: string;
}
interface LabelRow {
  kind: 'label';
  id: RouteAttemptOutcome;
  out: string;
}
interface EnumRow {
  kind: 'enum';
  id: 'via' | 'outcome' | 'trigger';
  out: string;
}

type Row = CollapseRow | DescribeRow | GlyphRow | LabelRow | EnumRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

const of = <T extends Row['kind']>(kind: T) => ROWS.filter((r): r is Extract<Row, { kind: T }> => r.kind === kind);

describe('route-trail-display agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every it.each below vacuously green.
    expect(ROWS).toHaveLength(25);
    expect(of('collapse')).toHaveLength(4);
    expect(of('describe')).toHaveLength(12);
    expect(of('glyph')).toHaveLength(3);
    expect(of('label')).toHaveLength(3);
    expect(of('enum')).toHaveLength(3);
  });

  it.each(of('collapse'))('collapse — $id', (row) => {
    const trail = JSON.parse(row.trailJson) as RouteAttempt[];
    expect(JSON.stringify(collapseRouteTrail(trail))).toBe(row.out);
  });

  it.each(of('describe'))('describe — $id', (row) => {
    const parsedRow = JSON.parse(row.rowJson) as RouteTrailRow;
    expect(describeRouteAttempt(parsedRow)).toBe(row.out);
  });

  it.each(of('glyph'))('glyph — $id', (row) => {
    if (row.id === 'failed') expect(ROUTE_OUTCOME_GLYPH.failed).toBe(row.out);
    else if (row.id === 'refused') expect(ROUTE_OUTCOME_GLYPH.refused).toBe(row.out);
    else expect(JSON.stringify('answered' in ROUTE_OUTCOME_GLYPH)).toBe(row.out);
  });

  it.each(of('label'))('label — $id', (row) => {
    expect(routeOutcomeLabel(row.id)).toBe(row.out);
  });
});

/**
 * The client-safe trigger/via/outcome unions are hand-rolled TypeScript
 * literal unions (v5 has no zod), so nothing else in the build checks them
 * against v4's actual `RouteAttemptViaEnum` / `RouteAttemptOutcomeEnum` /
 * the `trigger` enum. This sweep is that check, both ways: every v4 literal
 * must be assignable to the v5 type, and the v5 type must declare no literal
 * v4 doesn't have.
 */
describe("the trigger/via/outcome unions agree with v4's zod enums", () => {
  // Exhaustive maps, not arrays: `Record<Union, true>` fails to COMPILE when a
  // v5 literal is missing from the map, and the runtime sort-compare below
  // fails when the map names one v4 does not have — identity both ways, as
  // v4's own parity assertion is (the §3 unification review of the `78b381a96`
  // round: a bare array only ever proved array ⊆ union).
  const VIA_ALL: Record<RouteAttemptVia, true> = {
    primary: true,
    retry: true,
    concierge: true,
    understudy: true,
    'tier-pick': true,
  };
  const OUTCOME_ALL: Record<RouteAttemptOutcome, true> = { answered: true, failed: true, refused: true };
  const TRIGGER_ALL: Record<RouteAttemptTrigger, true> = {
    auth: true,
    'rate-limit': true,
    network: true,
    'model-missing': true,
    'provider-error': true,
    'empty-response': true,
    'moderation-refusal': true,
  };
  const VIA = Object.keys(VIA_ALL) as RouteAttemptVia[];
  const OUTCOME = Object.keys(OUTCOME_ALL) as RouteAttemptOutcome[];
  const TRIGGER = Object.keys(TRIGGER_ALL) as RouteAttemptTrigger[];

  it('via — five literals, both directions', () => {
    const recorded = JSON.parse(of('enum').find((r) => r.id === 'via')!.out) as string[];
    expect(VIA.slice().sort()).toEqual(recorded.slice().sort());
  });

  it('outcome — three literals, both directions', () => {
    const recorded = JSON.parse(of('enum').find((r) => r.id === 'outcome')!.out) as string[];
    expect(OUTCOME.slice().sort()).toEqual(recorded.slice().sort());
  });

  it('trigger — seven literals, both directions', () => {
    const recorded = JSON.parse(of('enum').find((r) => r.id === 'trigger')!.out) as string[];
    expect(TRIGGER.slice().sort()).toEqual(recorded.slice().sort());
  });
});
