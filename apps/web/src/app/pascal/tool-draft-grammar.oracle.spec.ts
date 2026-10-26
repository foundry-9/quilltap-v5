import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/tool-draft-grammar.oracle.ndjson';
import { P4D333_SPA_LANDED } from '../../testing/p4d334-gates';
import {
  conditionSlotKey,
  definitionFromDraft,
  draftFromDefinition,
  draftTableTestsLlm,
  findParameterReferences,
  gateConditionSlotKey,
  groupsOf,
  promptReadsBrief,
  reachableOutcomesMissingBrief,
  renameParameterEverywhere,
  serializeDraft,
  subjectValueType,
  validateDraft,
  type ToolDraft,
} from './tool-draft';

/**
 * The corpus differential for the Workbench's draft model at the pin
 * `322a5488e` (P4.D334 R-13 / item 8): v4's REAL `lib/pascal/tool-draft.ts`
 * over the round's arms — the one-level any-of groups, the outcome brief, the
 * Inventory and the Log — recorded by
 * `apps/web/oracle/tool-draft-grammar.recorder.ts`.
 *
 * The draft inputs are draft JSON with their ids normalised to first-appearance
 * tokens, so v5 feeds the SAME object to `definitionFromDraft` /
 * `validateDraft` without going through the parse; only the `definition` rows
 * (and the rows whose truth reads a P4.D333 grammar twin — `needs`) wait for
 * the sibling. A row whose sibling has not landed is skipped by name.
 */

type Need = 'P4D333' | 'P4D330';
interface Row {
  kind: 'groupsOf' | 'draft' | 'rename' | 'find' | 'definition' | 'direct';
  id: string;
  needs: Need[];
  fn?: string;
  joiners?: Array<'and' | 'or' | null>;
  draft?: ToolDraft;
  doc?: unknown;
  from?: string;
  to?: string;
  name?: string;
  args?: Record<string, unknown>;
  out: unknown;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);
const ofKind = (kind: Row['kind']) => ROWS.filter((r) => r.kind === kind);

function blocked(row: Row): string | null {
  return row.needs.includes('P4D333') && !P4D333_SPA_LANDED ? 'P4D333_SPA_LANDED' : null;
}
function replay(row: Row, run: () => void): void {
  const reason = blocked(row);
  if (reason) it.skip(`${row.id} (needs ${reason})`, run);
  else it(row.id, run);
}

/** Replay the recorder's `run()`: the definition, the serialised text, the issues. */
function runDraft(draft: ToolDraft) {
  return {
    definition: JSON.stringify(definitionFromDraft(draft)),
    serialized: serializeDraft(draft),
    issues: validateDraft(draft).map((i) => [i.severity, i.where, i.message]),
  };
}

/** Map draft ids to first-appearance tokens, exactly as the recorder does. */
function normaliser() {
  const ID_PATTERN = /^(cond|outcome|effect|param|gate|gcond)-\d+$/;
  const seen = new Map<string, string>();
  const counts = new Map<string, number>();
  const map = (value: unknown): unknown => {
    if (typeof value === 'string' && ID_PATTERN.test(value)) {
      let token = seen.get(value);
      if (!token) {
        const prefix = value.slice(0, value.indexOf('-'));
        const n = (counts.get(prefix) ?? 0) + 1;
        counts.set(prefix, n);
        token = `${prefix}#${n}`;
        seen.set(value, token);
      }
      return token;
    }
    if (Array.isArray(value)) return value.map(map);
    if (value && typeof value === 'object') {
      return Object.fromEntries(Object.entries(value as Record<string, unknown>).map(([k, v]) => [k, map(v)]));
    }
    return value;
  };
  return map;
}

describe('the draft model agrees with v4 row for row', () => {
  it('carries the whole recorded corpus', () => {
    expect(ROWS).toHaveLength(244);
    expect(ofKind('groupsOf')).toHaveLength(10);
    expect(ofKind('draft')).toHaveLength(159);
    expect(ofKind('rename')).toHaveLength(4);
    expect(ofKind('find')).toHaveLength(4);
    expect(ofKind('definition')).toHaveLength(12);
    expect(ofKind('direct')).toHaveLength(55);
  });

  describe('groupsOf', () => {
    for (const row of ofKind('groupsOf')) {
      replay(row, () => {
        const chips = (row.joiners ?? []).map((joiner, index) => ({ index, ...(joiner ? { joiner } : {}) }));
        expect(JSON.stringify(groupsOf(chips).map((g) => g.map((c) => c.index)))).toBe(row.out);
      });
    }
  });

  describe('draft → definition, serialisation and issues', () => {
    for (const row of ofKind('draft')) {
      replay(row, () => {
        expect(JSON.stringify(runDraft(row.draft!))).toBe(JSON.stringify(row.out));
      });
    }
  });

  describe('renameParameterEverywhere', () => {
    for (const row of ofKind('rename')) {
      replay(row, () => {
        const renamed = renameParameterEverywhere(structuredClone(row.draft!), row.from!, row.to!);
        expect(JSON.stringify(renamed)).toBe(JSON.stringify(row.out));
      });
    }
  });

  describe('findParameterReferences', () => {
    for (const row of ofKind('find')) {
      replay(row, () => {
        expect(findParameterReferences(row.draft!, row.name!)).toEqual(row.out);
      });
    }
  });

  describe('definition → draft (the parse)', () => {
    for (const row of ofKind('definition')) {
      replay(row, () => {
        const draft = draftFromDefinition(row.doc);
        if (row.out === null) {
          expect(draft).toBeNull();
          return;
        }
        const norm = normaliser();
        expect(JSON.stringify(norm(structuredClone(draft)))).toBe(JSON.stringify(row.draft));
        expect(JSON.stringify(norm(runDraft(draft!)))).toBe(JSON.stringify(row.out));
      });
    }
  });

  describe('direct calls', () => {
    for (const row of ofKind('direct')) {
      replay(row, () => {
        const a = row.args as Record<string, never>;
        let actual: unknown;
        switch (row.fn) {
          case 'subjectValueType': {
            const params = new Map([
              ['n', { type: 'number' }],
              ['s', { type: 'string' }],
              ['b', { type: 'boolean' }],
            ]);
            actual = subjectValueType(a['subject'], params as never);
            break;
          }
          case 'conditionSlotKey':
            actual = conditionSlotKey(a as never);
            break;
          case 'gateConditionSlotKey':
            actual = gateConditionSlotKey(a as never);
            break;
          case 'draftTableTestsLlm':
            actual = draftTableTestsLlm(a as never);
            break;
          case 'promptReadsBrief':
            actual = promptReadsBrief(a as never);
            break;
          case 'reachableOutcomesMissingBrief':
            actual = reachableOutcomesMissingBrief(a['draft'], a['hits']);
            break;
          default:
            throw new Error(`unknown direct fn ${row.fn}`);
        }
        expect(JSON.stringify(actual === undefined ? null : actual)).toBe(row.out);
      });
    }
  });
});
