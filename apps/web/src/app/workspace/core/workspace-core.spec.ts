/**
 * The tier-1 corpus differential for the workspace pure core (P4.9J1).
 *
 * v4 is the oracle. `harness/oracle/cases/workspace-core.ts` imports v4's REAL
 * `lib/workspace/{workspace-reducer,workspace-persistence,tab-meta}` and
 * `lib/navigation/route-to-intent` and emits the committed
 * `__fixtures__/workspace-core-fixtures.json`. This spec replays EVERY row
 * through the v5 hand-port and asserts deep equality (object states — deep
 * equal; strings — exact). The port cannot be verified by inspection: the
 * persistence validation was rewritten from Zod by hand, so the corpus is the
 * proof it strips/drops identically.
 *
 * Regen (from the v4 checkout at baseline `b8b12695`):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/workspace-core.ts \
 *     > <v5>/apps/web/src/app/workspace/core/__fixtures__/workspace-core-fixtures.json
 *
 * The v5-only route additions (v5's `/characters*` names) are unit-tested at the
 * bottom — they diverge from v4 by design, so they are NOT in the replayed corpus.
 */

import { describe, expect, it } from 'vitest';

import corpus from './__fixtures__/workspace-core-fixtures.json';
import {
  createInitialState,
  getPaneState,
  isActiveInItsPane,
  isSplit,
  paneOfTab,
  tabIdentity,
  workspaceReducer,
  type WorkspaceAction,
} from './reducer';
import {
  deserializeWorkspaceState,
  hydrateWorkspaceState,
  pruneWorkspaceState,
  serializeWorkspaceState,
  workspaceStorageKey,
  type PruneOptions,
} from './persistence';
import { DEFAULT_TAB_META } from './tab-meta';
import { parseHrefToIntent } from './route-to-intent';
import type { PaneId, WorkspaceState } from '../workspace-contract';

function makeIsChatValid(spec: string[] | 'all'): PruneOptions['isChatValid'] {
  if (spec === 'all') return () => true;
  const set = new Set(spec);
  return (id: string) => set.has(id);
}

describe('workspace core — reducer corpus', () => {
  for (const sc of corpus.reducer) {
    it(sc.label, () => {
      let state = createInitialState(sc.homeId);
      for (const step of sc.steps) {
        state = workspaceReducer(state, step.action as WorkspaceAction);
        expect(state).toEqual(step.state);
      }
    });
  }
});

describe('workspace core — tabIdentity corpus', () => {
  for (const row of corpus.tabIdentity) {
    it(`identity(${row.tab.kind}) = ${row.identity}`, () => {
      expect(tabIdentity(row.tab as { kind: WorkspaceState['tabs'][string]['kind'] })).toBe(
        row.identity,
      );
    });
  }
});

describe('workspace core — selector probes', () => {
  const probe = corpus.selectors;
  const state = probe.state as unknown as WorkspaceState;
  it('isSplit', () => {
    expect(isSplit(state)).toBe(probe.isSplit);
  });
  it('paneOfTab', () => {
    for (const [id, expected] of Object.entries(probe.paneOfTab)) {
      expect(paneOfTab(state, id)).toBe(expected as PaneId | null);
    }
  });
  it('isActiveInItsPane', () => {
    for (const [id, expected] of Object.entries(probe.isActiveInItsPane)) {
      expect(isActiveInItsPane(state, id)).toBe(expected);
    }
  });
  it('getPaneState', () => {
    expect(getPaneState(state, 'left')).toEqual(probe.getPaneState.left);
    expect(getPaneState(state, 'right')).toEqual(probe.getPaneState.right);
  });
});

describe('workspace core — persistence: storageKey', () => {
  for (const row of corpus.persistence.storageKey) {
    it(`key(${String(row.instanceId)})`, () => {
      expect(workspaceStorageKey(row.instanceId)).toBe(row.key);
    });
  }
});

describe('workspace core — persistence: deserialize', () => {
  for (const row of corpus.persistence.deserialize) {
    it(row.label, () => {
      expect(deserializeWorkspaceState(row.input)).toEqual(row.output);
    });
  }
});

describe('workspace core — persistence: prune', () => {
  for (const row of corpus.persistence.prune) {
    it(row.label, () => {
      const out = pruneWorkspaceState(
        row.input as unknown as WorkspaceState,
        { isChatValid: makeIsChatValid(row.validChatIds as string[] | 'all') },
        row.homeFallbackId,
      );
      expect(out).toEqual(row.output);
    });
  }
});

describe('workspace core — persistence: hydrate', () => {
  for (const row of corpus.persistence.hydrate) {
    it(row.label, () => {
      const out = hydrateWorkspaceState(
        row.input,
        { isChatValid: makeIsChatValid(row.validChatIds as string[] | 'all') },
        row.homeFallbackId,
      );
      expect(out).toEqual(row.output);
    });
  }
});

describe('workspace core — persistence: serialize round-trip', () => {
  for (const row of corpus.persistence.roundTrip) {
    it(row.label, () => {
      const serialized = serializeWorkspaceState(row.state as unknown as WorkspaceState);
      expect(serialized).toBe(row.serialized);
      expect(deserializeWorkspaceState(serialized)).toEqual(row.deserialized);
    });
  }
});

describe('workspace core — route-to-intent corpus', () => {
  for (const row of corpus.routeIntent) {
    it(`href ${JSON.stringify(row.href)}`, () => {
      expect(parseHrefToIntent(row.href)).toEqual(row.intent);
    });
  }
});

describe('workspace core — tab-meta table', () => {
  it('matches v4 DEFAULT_TAB_META byte-for-byte', () => {
    expect(DEFAULT_TAB_META).toEqual(corpus.tabMeta);
  });
});

// --- v5-only route additions (diverge from v4 by design; NOT in the corpus) --
describe('route-to-intent — v5 /characters route adaptations', () => {
  it('maps the bare roster', () => {
    expect(parseHrefToIntent('/characters')).toEqual({ kind: 'aurora' });
  });
  it('maps the create form', () => {
    expect(parseHrefToIntent('/characters/new')).toEqual({ kind: 'character-new' });
  });
  it('maps a bare /characters/<id> to the detail view (no /view suffix in v5)', () => {
    expect(parseHrefToIntent('/characters/abc')).toEqual({
      kind: 'character-view',
      payload: { characterId: 'abc' },
    });
  });
  it('still maps the legacy /characters/<id>/edit and /view', () => {
    expect(parseHrefToIntent('/characters/abc/edit')).toEqual({
      kind: 'character-edit',
      payload: { characterId: 'abc', tab: undefined },
    });
    expect(parseHrefToIntent('/characters/abc/view')).toEqual({
      kind: 'character-view',
      payload: { characterId: 'abc', tab: undefined },
    });
  });
  it('leaves v5-only bare details (group/project/store/terminal) unmapped', () => {
    expect(parseHrefToIntent('/characters/groups/g1')).toBeNull();
    expect(parseHrefToIntent('/prospero/p1')).toBeNull();
    expect(parseHrefToIntent('/scriptorium/s1')).toBeNull();
    expect(parseHrefToIntent('/salon/c1/terminal/sess1')).toBeNull();
  });
});
