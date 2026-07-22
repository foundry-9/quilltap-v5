/**
 * Oracle case (P4.9J1): the tabbed-workspace pure core.
 *
 * Drives v4's REAL pure modules (never reimplements them):
 *   lib/workspace/workspace-reducer.ts   — reducer + selectors
 *   lib/workspace/workspace-persistence.ts — serialize/deserialize/prune/hydrate
 *   lib/workspace/tab-meta.ts            — the DEFAULT_TAB_META table
 *   lib/navigation/route-to-intent.ts    — parseHrefToIntent
 *
 * Emits ONE deterministic JSON corpus (pretty-printed) that the v5 pure-core
 * port replays row-for-row. Every id the reducer needs is supplied in the
 * scripted action (the reducer is pure — no crypto/localStorage), so the corpus
 * is fully deterministic and regenerates byte-stably. The persistence layer is
 * likewise pure. `standaloneDocKey`'s uuid branch is never exercised here (all
 * standalone-document payloads carry an explicit `filePath`/`docKey`).
 *
 * Run from inside the v4 server checkout (imports resolve `@/` there):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/workspace-core.ts \
 *     > ~/source/quilltap-v5/apps/web/src/app/workspace/core/__fixtures__/workspace-core-fixtures.json
 *
 * Never hand-edit the emitted fixture; regenerate and grep a known row.
 */

import {
  workspaceReducer,
  createInitialState,
  tabIdentity,
  paneOfTab,
  getPaneState,
  isActiveInItsPane,
  isSplit,
  type WorkspaceAction,
} from '@/lib/workspace/workspace-reducer';
import {
  serializeWorkspaceState,
  deserializeWorkspaceState,
  pruneWorkspaceState,
  hydrateWorkspaceState,
  workspaceStorageKey,
} from '@/lib/workspace/workspace-persistence';
import { DEFAULT_TAB_META } from '@/lib/workspace/tab-meta';
import { parseHrefToIntent } from '@/lib/navigation/route-to-intent';
import type { PaneId, TabKind, WorkspaceState } from '@/lib/workspace/types';

// ---------------------------------------------------------------------------
// Reducer corpus — scripted action sequences, recording the COMPLETE state
// after every action (the replay applies the same actions and deep-equals).
// ---------------------------------------------------------------------------

interface ReducerStep {
  action: WorkspaceAction;
  state: WorkspaceState;
}
interface ReducerScenario {
  label: string;
  /** Start from `createInitialState(homeId)`. */
  homeId: string;
  steps: ReducerStep[];
}

function scenario(label: string, homeId: string, actions: WorkspaceAction[]): ReducerScenario {
  let state = createInitialState(homeId);
  const steps: ReducerStep[] = [];
  for (const action of actions) {
    state = workspaceReducer(state, action);
    steps.push({ action, state });
  }
  return { label, homeId, steps };
}

const open = (
  id: string,
  kind: TabKind,
  extra: Partial<Extract<WorkspaceAction, { type: 'OPEN_TAB' }>> = {},
): WorkspaceAction => ({ type: 'OPEN_TAB', id, kind, ...extra });

const reducerScenarios: ReducerScenario[] = [
  scenario('open-into-focused-pane-activates', 'home', [open('a', 'aurora')]),

  scenario('de-dupe-singleton-focuses-existing', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero'),
    open('zzz', 'aurora'), // duplicate aurora, different id — ignored
  ]),

  scenario('salon-tabs-distinct-by-chatId', 'home', [
    open('a', 'salon', { payload: { chatId: 'c1' } }),
    open('b', 'salon', { payload: { chatId: 'c2' } }),
  ]),

  scenario('de-dupe-salon-by-chatId', 'home', [
    open('a', 'salon', { payload: { chatId: 'c1' } }),
    open('dup', 'salon', { payload: { chatId: 'c1' } }),
  ]),

  scenario('document-tabs-distinct-by-chat-and-doc', 'home', [
    open('d1', 'document', { payload: { chatId: 'c1', chatDocumentId: 'doc1' } }),
    open('d2', 'document', { payload: { chatId: 'c1', chatDocumentId: 'doc2' } }),
    open('d1dup', 'document', { payload: { chatId: 'c1', chatDocumentId: 'doc1' } }),
  ]),

  scenario('character-edit-and-view-distinct-per-character', 'home', [
    open('ce1', 'character-edit', { payload: { characterId: 'ch1' } }),
    open('ce2', 'character-edit', { payload: { characterId: 'ch2' } }),
    open('cv1', 'character-view', { payload: { characterId: 'ch1' } }),
  ]),

  // v4 `8d86847a`: the drill-in payloads. prospero/scriptorium/aurora stay
  // SINGLETONS keyed by kind alone, so a `/prospero/<id>` deep link re-uses the
  // open tab and re-targets its payload (the OPEN_TAB payload-refresh arm).
  scenario('drill-into-open-singleton-refreshes-payload', 'home', [
    open('p', 'prospero'),
    open('ignored', 'prospero', { payload: { projectId: 'p1' } }),
    open('ignored2', 'prospero', { payload: { projectId: 'p2' } }),
  ]),

  // A drill-first open, then a payload-less re-open: `payload === undefined`
  // does NOT clear the existing payload (the `action.payload !== undefined`
  // guard) — the tab stays drilled. Pins the asymmetry.
  scenario('drill-first-then-plain-reopen-keeps-payload', 'home', [
    open('s', 'scriptorium', { payload: { storeId: 's1' } }),
    open('ignored', 'scriptorium'),
  ]),

  scenario('salon-list-is-a-singleton-beside-salon-tabs', 'home', [
    open('sl', 'salon-list'),
    open('s1', 'salon', { payload: { chatId: 'c1' } }),
    open('sldup', 'salon-list'),
  ]),

  scenario('aurora-group-drill-refreshes-payload', 'home', [
    open('a', 'aurora', { payload: { groupId: 'g1' } }),
    open('ignored', 'aurora', { payload: { groupId: 'g2' } }),
  ]),

  scenario('custom-tools-library-vs-definition-distinct', 'home', [
    open('lib', 'custom-tools'),
    open('def', 'custom-tools', { payload: { mountPointId: 'm1', path: 'p/a.json' } }),
    open('libdup', 'custom-tools'),
  ]),

  scenario('update-payload-and-title-on-reopen-singleton', 'home', [
    open('a', 'settings', { payload: { tab: 'system' }, title: 'System' }),
    open('ignored', 'settings', { payload: { tab: 'memory' }, title: 'Memory' }),
  ]),

  // Same keys, DIFFERENT insertion order — payloadEqual uses JSON.stringify so
  // this is NOT equal → the payload refreshes. Pins the stringify semantics.
  scenario('reopen-same-keys-different-order-refreshes-payload', 'home', [
    open('a', 'settings', { payload: { tab: 'system', section: 'memory' } }),
    open('ignored', 'settings', { payload: { section: 'memory', tab: 'system' } }),
  ]),

  scenario('reopen-identical-payload-no-change', 'home', [
    open('a', 'settings', { payload: { tab: 'system' } }),
    open('again', 'settings', { payload: { tab: 'system' } }),
  ]),

  scenario('open-right-pane-creates-split', 'home', [open('a', 'aurora', { pane: 'right' })]),

  scenario('focus-false-inserts-without-activation', 'home', [open('a', 'aurora', { focus: false })]),

  scenario('focus-false-into-empty-right-becomes-active-there', 'home', [
    open('a', 'aurora', { pane: 'right', focus: false }),
  ]),

  scenario('respects-explicit-parentTabId', 'home', [
    open('s', 'salon', { payload: { chatId: 'c1' } }),
    open('t', 'terminal', { payload: { chatId: 'c1' }, parentTabId: 's' }),
  ]),

  // De-dupe with focus:false only refreshes payload; never changes active pane.
  scenario('de-dupe-focus-false-refresh-only', 'home', [
    open('a', 'settings', { payload: { tab: 'system' } }),
    open('b', 'aurora'),
    open('ignored', 'settings', { payload: { tab: 'memory' }, focus: false }),
  ]),

  scenario('set-active-and-focused-pane', 'home', [
    open('a', 'aurora'),
    { type: 'SET_ACTIVE', pane: 'left', id: 'home' },
  ]),

  scenario('set-active-ignores-foreign-id', 'home', [{ type: 'SET_ACTIVE', pane: 'left', id: 'nope' }]),

  scenario('set-focused-pane-ignores-absent-right', 'home', [{ type: 'SET_FOCUSED_PANE', pane: 'right' }]),

  scenario('set-focused-pane-switches-when-right-exists', 'home', [
    open('a', 'aurora', { pane: 'right' }),
    { type: 'SET_FOCUSED_PANE', pane: 'left' },
  ]),

  scenario('move-reorder-same-pane-activates-moved', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero'),
    { type: 'MOVE_TAB', id: 'b', toPane: 'left', toIndex: 0 },
  ]),

  scenario('move-to-right-creates-split', 'home', [
    open('a', 'aurora'),
    { type: 'MOVE_TAB', id: 'a', toPane: 'right' },
  ]),

  scenario('move-collapses-emptied-source', 'home', [
    open('a', 'aurora', { pane: 'right' }),
    { type: 'MOVE_TAB', id: 'a', toPane: 'left' },
  ]),

  scenario('move-picks-new-source-active', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero'), // active b
    { type: 'MOVE_TAB', id: 'b', toPane: 'right' },
  ]),

  // Cross-pane move that empties the LEFT pane promotes right→left.
  scenario('move-empties-left-promotes-right', 'home', [
    open('a', 'aurora', { pane: 'right' }), // home in left, a in right, focus right
    { type: 'MOVE_TAB', id: 'home', toPane: 'right', toIndex: 0 },
  ]),

  scenario('move-out-of-range-index-clamps', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero'),
    { type: 'MOVE_TAB', id: 'home', toPane: 'left', toIndex: 99 },
  ]),

  scenario('move-unknown-id-noop', 'home', [{ type: 'MOVE_TAB', id: 'ghost', toPane: 'right' }]),

  scenario('unsplit-merges-right-into-left', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero', { pane: 'right' }),
    { type: 'UNSPLIT' },
  ]),

  scenario('unsplit-noop-when-unsplit', 'home', [{ type: 'UNSPLIT' }]),

  scenario('set-split-ratio-mid', 'home', [{ type: 'SET_SPLIT_RATIO', ratio: 0.42 }]),
  scenario('set-split-ratio-below-min-clamps', 'home', [{ type: 'SET_SPLIT_RATIO', ratio: 0.01 }]),
  scenario('set-split-ratio-above-max-clamps', 'home', [{ type: 'SET_SPLIT_RATIO', ratio: 0.99 }]),
  scenario('set-split-ratio-nan-defaults', 'home', [{ type: 'SET_SPLIT_RATIO', ratio: NaN }]),
  scenario('set-split-ratio-infinity-defaults', 'home', [
    { type: 'SET_SPLIT_RATIO', ratio: Infinity },
  ]),
  scenario('set-split-ratio-negative-clamps', 'home', [{ type: 'SET_SPLIT_RATIO', ratio: -3 }]),

  scenario('close-activates-neighbour', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero'), // active b
    { type: 'CLOSE_TAB', id: 'b', homeFallbackId: 'h2' },
  ]),

  scenario('close-last-tab-resets-to-home', 'home', [
    { type: 'CLOSE_TAB', id: 'home', homeFallbackId: 'h2' },
  ]),

  scenario('close-cascades-to-children', 'home', [
    open('s', 'salon', { payload: { chatId: 'c1' } }),
    open('t', 'terminal', { payload: { chatId: 'c1' }, parentTabId: 's' }),
    open('d', 'document', { payload: { chatId: 'c1', chatDocumentId: 'doc1' }, parentTabId: 's' }),
    { type: 'CLOSE_TAB', id: 's', homeFallbackId: 'h2' },
  ]),

  scenario('close-collapses-right-pane', 'home', [
    open('a', 'aurora', { pane: 'right' }),
    { type: 'CLOSE_TAB', id: 'a', homeFallbackId: 'h2' },
  ]),

  scenario('close-empties-left-promotes-right', 'home', [
    open('a', 'aurora', { pane: 'right' }),
    { type: 'CLOSE_TAB', id: 'home', homeFallbackId: 'h2' },
  ]),

  scenario('close-unknown-id-noop', 'home', [{ type: 'CLOSE_TAB', id: 'nope', homeFallbackId: 'h2' }]),

  // Neighbour-pick BACKWARD arm: close the last tab in the pane while it is
  // active — pickActiveAfterRemoval walks forward (nothing), then backward.
  scenario('close-active-last-picks-backward', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero'), // order home,a,b ; active b (last)
    { type: 'CLOSE_TAB', id: 'b', homeFallbackId: 'h2' },
  ]),

  // Neighbour-pick FORWARD arm: close an active middle tab.
  scenario('close-active-middle-picks-forward', 'home', [
    open('a', 'aurora'),
    open('b', 'prospero'),
    open('c', 'scriptorium'),
    { type: 'SET_ACTIVE', pane: 'left', id: 'a' }, // active middle 'a'
    { type: 'CLOSE_TAB', id: 'a', homeFallbackId: 'h2' },
  ]),

  scenario('replace-state-wholesale', 'home', [
    open('a', 'aurora'),
    {
      type: 'REPLACE_STATE',
      state: {
        tabs: { z: { id: 'z', kind: 'home', title: 'Home', icon: 'sparkles' } },
        panes: { left: { order: ['z'], activeTabId: 'z' }, right: null },
        focusedPane: 'left',
        splitRatio: 0.3,
      },
    },
  ]),

  // A longer combined sequence exercising split, cross-pane move, active repair,
  // unsplit-after, and a close cascade in one run.
  scenario('combined-split-move-close', 'home', [
    open('s', 'salon', { payload: { chatId: 'c9' } }),
    open('t', 'terminal', { payload: { chatId: 'c9' }, parentTabId: 's' }),
    { type: 'MOVE_TAB', id: 't', toPane: 'right' },
    { type: 'SET_SPLIT_RATIO', ratio: 0.65 },
    { type: 'SET_ACTIVE', pane: 'left', id: 's' },
    { type: 'CLOSE_TAB', id: 's', homeFallbackId: 'h2' }, // cascades t; right empties→collapse
  ]),
];

// ---------------------------------------------------------------------------
// tabIdentity corpus — every arm.
// ---------------------------------------------------------------------------

const identityCases: Array<{ tab: { kind: TabKind; payload?: unknown }; identity: string }> = [
  { tab: { kind: 'salon', payload: { chatId: 'c1' } }, identity: '' },
  { tab: { kind: 'terminal', payload: { chatId: 'c1' } }, identity: '' },
  { tab: { kind: 'document', payload: { chatId: 'c1', chatDocumentId: 'd1' } }, identity: '' },
  { tab: { kind: 'document', payload: { chatId: 'c1', chatDocumentId: 'd2' } }, identity: '' },
  { tab: { kind: 'document', payload: { chatId: 'c1' } }, identity: '' }, // missing doc id
  { tab: { kind: 'document-standalone', payload: { docKey: 'general::notes.md' } }, identity: '' },
  { tab: { kind: 'document-standalone', payload: {} }, identity: '' }, // missing docKey
  { tab: { kind: 'salon-list' }, identity: '' },
  // The drill payload does NOT change identity — these stay singletons.
  { tab: { kind: 'prospero', payload: { projectId: 'p1' } }, identity: '' },
  { tab: { kind: 'scriptorium', payload: { storeId: 's1' } }, identity: '' },
  { tab: { kind: 'aurora', payload: { groupId: 'g1' } }, identity: '' },
  { tab: { kind: 'character-edit', payload: { characterId: 'x' } }, identity: '' },
  { tab: { kind: 'character-view', payload: { characterId: 'x' } }, identity: '' },
  { tab: { kind: 'custom-tools' }, identity: '' },
  { tab: { kind: 'custom-tools', payload: { mountPointId: 'm', path: 'p' } }, identity: '' },
  { tab: { kind: 'custom-tools', payload: { mountPointId: 'm' } }, identity: '' },
  { tab: { kind: 'salon', payload: undefined }, identity: '' }, // missing chatId
  { tab: { kind: 'aurora' }, identity: '' },
  { tab: { kind: 'settings', payload: { tab: 'system' } }, identity: '' },
  { tab: { kind: 'home' }, identity: '' },
  { tab: { kind: 'wardrobe', payload: { characterId: 'x' } }, identity: '' },
];
for (const c of identityCases) c.identity = tabIdentity(c.tab);

// ---------------------------------------------------------------------------
// Selector probes — paneOfTab / getPaneState / isActiveInItsPane / isSplit over
// a representative split state.
// ---------------------------------------------------------------------------

function selectorProbeState(): WorkspaceState {
  let s = createInitialState('home');
  s = workspaceReducer(s, open('a', 'aurora')); // left: home,a
  s = workspaceReducer(s, open('b', 'prospero', { pane: 'right' })); // right: b
  return s;
}
const probeState = selectorProbeState();
const selectorProbes = {
  state: probeState,
  isSplit: isSplit(probeState),
  paneOfTab: {
    home: paneOfTab(probeState, 'home'),
    a: paneOfTab(probeState, 'a'),
    b: paneOfTab(probeState, 'b'),
    ghost: paneOfTab(probeState, 'ghost'),
  } as Record<string, PaneId | null>,
  isActiveInItsPane: {
    home: isActiveInItsPane(probeState, 'home'),
    a: isActiveInItsPane(probeState, 'a'),
    b: isActiveInItsPane(probeState, 'b'),
    ghost: isActiveInItsPane(probeState, 'ghost'),
  } as Record<string, boolean>,
  getPaneState: {
    left: getPaneState(probeState, 'left'),
    right: getPaneState(probeState, 'right'),
  },
};

// ---------------------------------------------------------------------------
// Persistence corpus.
// ---------------------------------------------------------------------------

function buildLayout(): WorkspaceState {
  // home + salon(c1) in left; salon(c2) + terminal(c2) in right.
  let s = createInitialState('home');
  s = workspaceReducer(s, open('s1', 'salon', { payload: { chatId: 'c1' } }));
  s = workspaceReducer(s, open('s2', 'salon', { payload: { chatId: 'c2' }, pane: 'right' }));
  s = workspaceReducer(s, open('t2', 'terminal', { payload: { chatId: 'c2' }, parentTabId: 's2', pane: 'right' }));
  return s;
}

// -- storageKey --
const storageKeyCases = [
  { instanceId: undefined as string | undefined | null, key: workspaceStorageKey() },
  { instanceId: null, key: workspaceStorageKey(null) },
  { instanceId: 'inst-1', key: workspaceStorageKey('inst-1') },
];

// -- deserialize --
const layout = buildLayout();
const everyKindState: WorkspaceState = (() => {
  const kinds: TabKind[] = [
    'home', 'salon', 'salon-list', 'terminal', 'document', 'aurora', 'prospero', 'scriptorium',
    'settings', 'files', 'photos', 'scenarios', 'brahma', 'wardrobe', 'profile',
    'about', 'generate-image', 'document-standalone', 'character-new',
    'character-edit', 'character-view', 'settings-wizard', 'custom-tools',
  ];
  const tabs: WorkspaceState['tabs'] = {};
  for (const kind of kinds) {
    const id = `t_${kind}`;
    tabs[id] = { id, kind, title: kind };
  }
  return {
    tabs,
    panes: { left: { order: Object.keys(tabs), activeTabId: Object.keys(tabs)[0] }, right: null },
    focusedPane: 'left',
    splitRatio: 0.5,
  };
})();

const bogusTabState = {
  ...layout,
  tabs: {
    ...layout.tabs,
    bogus: { id: 'bogus', kind: 'from-the-future', title: 'Mystery' },
  },
  panes: {
    ...layout.panes,
    left: { ...layout.panes.left, order: [...layout.panes.left.order, 'bogus'] },
  },
};

// A tab carrying extra unknown keys — Zod's default .strip() drops them; the
// v5 hand-port must strip identically.
const extraKeyTabState = {
  ...layout,
  tabs: {
    ...layout.tabs,
    s1: { ...layout.tabs.s1, junk: 'DROP ME', another: 42 },
  },
};

// Extra top-level + pane-level keys — also stripped by the outer object schema.
const extraTopState = { ...layout, extraTop: 'DROP', panes: { left: { ...layout.panes.left, junk: 1 }, right: layout.panes.right } };

const deserializeCases: Array<{ label: string; input: string | null; output: WorkspaceState | null }> = [
  { label: 'null', input: null, output: null },
  { label: 'empty', input: '', output: null },
  { label: 'not-json', input: 'not json', output: null },
  { label: 'missing-panes', input: '{"tabs":{}}', output: null },
  { label: 'bad-focused-pane', input: JSON.stringify({ ...layout, focusedPane: 'middle' }), output: null },
  { label: 'valid-round-trip', input: serializeWorkspaceState(layout), output: null },
  { label: 'every-kind-round-trip', input: serializeWorkspaceState(everyKindState), output: null },
  { label: 'unknown-kind-tab-dropped', input: JSON.stringify(bogusTabState), output: null },
  { label: 'extra-tab-keys-stripped', input: JSON.stringify(extraKeyTabState), output: null },
  { label: 'extra-top-and-pane-keys-stripped', input: JSON.stringify(extraTopState), output: null },
  { label: 'tabs-not-object', input: JSON.stringify({ ...layout, tabs: [] }), output: null },
  { label: 'ratio-non-number', input: JSON.stringify({ ...layout, splitRatio: 'x' }), output: null },
];
for (const c of deserializeCases) c.output = deserializeWorkspaceState(c.input);

// -- prune -- (validChatIds: 'all' → everything valid; else whitelist) --
interface PruneCase {
  label: string;
  input: WorkspaceState;
  validChatIds: string[] | 'all';
  homeFallbackId: string;
  output: WorkspaceState;
}
function makeIsChatValid(spec: string[] | 'all'): (id: string) => boolean {
  if (spec === 'all') return () => true;
  const set = new Set(spec);
  return (id) => set.has(id);
}

function standaloneLayout(): WorkspaceState {
  let s = createInitialState('home');
  s = workspaceReducer(s, open('ds1', 'document-standalone', {
    payload: { docKey: 'general::notes.md', scope: 'general', filePath: 'notes.md' },
  }));
  s = workspaceReducer(s, open('ds2', 'document-standalone', {
    payload: { docKey: 'uuid-blank', scope: 'general' }, // no filePath → dropped
  }));
  return s;
}

function orphanChainLayout(): WorkspaceState {
  // salon s(c1) → terminal t(parent s) → document gd(parent t): a 3-link chain.
  let s = createInitialState('home');
  s = workspaceReducer(s, open('s', 'salon', { payload: { chatId: 'c1' } }));
  s = workspaceReducer(s, open('t', 'terminal', { payload: { chatId: 'c1' }, parentTabId: 's' }));
  s = workspaceReducer(s, open('gd', 'document', { payload: { chatId: 'c1', chatDocumentId: 'd1' }, parentTabId: 't' }));
  return s;
}

function missingDocIdLayout(): WorkspaceState {
  let s = createInitialState('home');
  // A pre-multi-doc document tab lacking chatDocumentId → dropped even with a valid chat.
  s = workspaceReducer(s, open('d', 'document', { payload: { chatId: 'c1' } }));
  return s;
}

function danglingActiveLayout(): WorkspaceState {
  const s = buildLayout();
  return {
    ...s,
    panes: {
      ...s.panes,
      left: { order: [...s.panes.left.order, 'ghost'], activeTabId: 'ghost' },
    },
  };
}

function onlyInvalidSalonLayout(): WorkspaceState {
  let s = createInitialState('home');
  s = workspaceReducer(s, open('s1', 'salon', { payload: { chatId: 'c1' } }));
  s = workspaceReducer(s, { type: 'CLOSE_TAB', id: 'home', homeFallbackId: 'x' });
  return s; // only s1 remains
}

const pruneCases: PruneCase[] = [
  { label: 'all-valid-unchanged', input: buildLayout(), validChatIds: 'all', homeFallbackId: 'h2', output: null as never },
  { label: 'drop-invalid-salon', input: buildLayout(), validChatIds: ['c2'], homeFallbackId: 'h2', output: null as never },
  { label: 'drop-parent-orphans-child', input: buildLayout(), validChatIds: ['c1'], homeFallbackId: 'h2', output: null as never },
  { label: 'orphan-chain-fixpoint', input: orphanChainLayout(), validChatIds: [], homeFallbackId: 'h2', output: null as never },
  { label: 'missing-doc-id-dropped', input: missingDocIdLayout(), validChatIds: 'all', homeFallbackId: 'h2', output: null as never },
  { label: 'standalone-with-and-without-file', input: standaloneLayout(), validChatIds: 'all', homeFallbackId: 'h2', output: null as never },
  { label: 'dangling-active-and-stray-order', input: danglingActiveLayout(), validChatIds: 'all', homeFallbackId: 'h2', output: null as never },
  { label: 'total-loss-home-fallback', input: onlyInvalidSalonLayout(), validChatIds: [], homeFallbackId: 'h2', output: null as never },
];
for (const c of pruneCases) c.output = pruneWorkspaceState(c.input, { isChatValid: makeIsChatValid(c.validChatIds) }, c.homeFallbackId);

// -- hydrate -- (raw → prune) --
interface HydrateCase {
  label: string;
  input: string | null;
  validChatIds: string[] | 'all';
  homeFallbackId: string;
  output: WorkspaceState;
}
const hydrateCases: HydrateCase[] = [
  { label: 'round-trip-prune-dead', input: serializeWorkspaceState(buildLayout()), validChatIds: ['c2'], homeFallbackId: 'h2', output: null as never },
  { label: 'missing-storage-fresh-home', input: null, validChatIds: 'all', homeFallbackId: 'h2', output: null as never },
  { label: 'garbage-fresh-home', input: 'garbage', validChatIds: 'all', homeFallbackId: 'h2', output: null as never },
];
for (const c of hydrateCases) c.output = hydrateWorkspaceState(c.input, { isChatValid: makeIsChatValid(c.validChatIds) }, c.homeFallbackId);

// -- serialize→deserialize round-trips --
const roundTripStates: Array<{ label: string; state: WorkspaceState }> = [
  { label: 'layout', state: buildLayout() },
  { label: 'every-kind', state: everyKindState },
  { label: 'initial', state: createInitialState('home') },
];
const roundTripCases = roundTripStates.map(({ label, state }) => {
  const serialized = serializeWorkspaceState(state);
  return { label, state, serialized, deserialized: deserializeWorkspaceState(serialized) };
});

// ---------------------------------------------------------------------------
// Route-intent corpus — the v4 route table + edges. (v5 additions are unit-
// tested v5-side; these v4 rows must replay byte-identically.)
// ---------------------------------------------------------------------------

const routeHrefs = [
  '/salon/abc123',
  '/salon/new', // still null — v4 handles new-chat in the interceptor
  '/salon',
  '/chats', // legacy alias for the salon list
  '/salon/', // trailing slash on the list
  '/',
  '/aurora',
  '/aurora/new',
  '/prospero',
  '/scriptorium',
  '/files',
  '/photos',
  '/scenarios',
  '/profile',
  '/about',
  '/generate-image',
  '/settings/wizard',
  '/settings',
  '/settings?tab=system&section=memory',
  '/settings?tab=system',
  '/custom-tools',
  '/custom-tools?mount=m1',
  '/custom-tools?path=p/a.json',
  '/custom-tools?new=1',
  '/custom-tools?mount=m1&path=p/a.json&new=1',
  '/aurora/abc/edit',
  '/aurora/abc/edit?tab=system-prompts',
  '/characters/abc/edit?tab=system-prompts',
  '/aurora/abc/view',
  '/aurora/abc/view?tab=conversations',
  '/characters/abc/view?tab=conversations',
  '/aurora/abc', // bare detail → null
  '/aurora/', // trailing slash
  '/aurora#x', // hash fragment
  '/aurora?foo=bar', // stray query ignored on a singleton
  '/salon/abc123?ref=1', // salon with query
  '/salon/abc123#top', // salon with hash
  '/salon/abc123/', // salon trailing slash
  '/unlock', // unknown
  '/settings/', // trailing slash on a two-segment path
  'https://example.com', // external
  '', // empty
  'relative/path', // non-absolute
  '#anchor', // hash only
  // v4 `8d86847a`: the drill-in detail routes.
  '/prospero/p1',
  '/projects/p1', // legacy alias for the project detail
  '/scriptorium/s1',
  '/aurora/groups/g1',
  '/prospero/p1?ref=1', // query stripped before matching
  '/prospero/p1/', // trailing slash
  '/prospero/p1/extra', // too deep → null
  '/aurora/groups', // no group id → the bare-detail arm (null in v4)
  '/aurora/groups/g1/members', // too deep → null
];
const routeIntentCases = routeHrefs.map((href) => ({ href, intent: parseHrefToIntent(href) }));

// ---------------------------------------------------------------------------
// Emit.
// ---------------------------------------------------------------------------

const corpus = {
  _meta: {
    note: 'Generated by harness/oracle/cases/workspace-core.ts from v4 real lib/. Do not hand-edit; regenerate.',
    baseline: 'e646f58b',
  },
  reducer: reducerScenarios,
  tabIdentity: identityCases,
  selectors: selectorProbes,
  persistence: {
    storageKey: storageKeyCases,
    deserialize: deserializeCases,
    prune: pruneCases,
    hydrate: hydrateCases,
    roundTrip: roundTripCases,
  },
  routeIntent: routeIntentCases,
  tabMeta: DEFAULT_TAB_META,
};

process.stdout.write(JSON.stringify(corpus, null, 2) + '\n');
