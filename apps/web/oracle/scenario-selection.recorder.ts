/**
 * The v4-side recorder behind `src/app/scenario/scenario-selection.oracle.spec.ts`
 * (P4.D302).
 *
 * v4 `545e299bb` ("Scenarios from anywhere") taught the scenario picker's
 * vocabulary a fifth kind of selection — `store`, a scenario plucked from any
 * shelf in the instance through the browser — with its own token
 * (`store:<mountPointId>:<relativePath>`, only the FIRST colon splits), a
 * label that rides in the selection because the token cannot carry it, a
 * `scenarioRef` payload, and a "Browse all scenarios…" token that is never a
 * selection at all. The same commit gave New Chat ONE seeded selection
 * (`components/new-chat/scenario-seed.ts`: character default > project
 * default > general default > Custom, never re-seeded once touched).
 *
 * This recorder drives v4's REAL `components/scenario/types.ts` and
 * `components/new-chat/scenario-seed.ts` over every kind, every malformed /
 * colon-bearing / empty / browse / custom token with and without `current`,
 * and the defaults grid, so the SPA's transcription is compared row for row.
 * With `QT_RECORD_BROWSER=1` it ALSO drives the browser's three pure helpers
 * (`isInHierarchy` / `pickToSelection` / `scenarioMatchesSearch`), which live
 * in v4's `ScenarioBrowserDialog.tsx` beside React — see the Tier-2 note below.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/components/...`
 * and would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree at `83334b9a5` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 83334b9a5}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/scenario-selection.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/scenario-selection.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_RECORD_BROWSER=1 npx tsx scenario-selection.recorder.ts > "$OUT")
 * wc -l "$OUT"          # expect 607 (also printed on stderr as rows=607)
 * grep -c '"kind":"store"' "$OUT"   # 279
 * ```
 *
 * Expect 607 lines: 10 `scenarioSelectionToValue` + 10
 * `scenarioSelectionToPayload` + 144 `scenarioValueToSelection` (24 tokens × 6
 * currents) + 10 `roundTrip` + 64 `defaultScenarioSelection` + 256
 * `seedScenarioSelection`, then the browser's 27 `isInHierarchy` + 54
 * `pickToSelection` + 32 `scenarioMatchesSearch`. Without `QT_RECORD_BROWSER=1`
 * the last 113 are missing and the spec's count row fails. A shorter file
 * means the recorder errored and the redirect already truncated the old one
 * (the empty-file trap).
 */

import {
  scenarioSelectionToPayload,
  scenarioSelectionToValue,
  scenarioValueToSelection,
  type ScenarioSelection,
} from '@/components/scenario/types';
import {
  defaultScenarioSelection,
  seedScenarioSelection,
  type ScenarioDefaults,
} from '@/components/new-chat/scenario-seed';

/** `undefined` cannot ride JSON — the spec maps this tag back. */
const UNDEFINED = { $undefined: true } as const;
const tag = (v: unknown): unknown => (v === undefined ? UNDEFINED : v);

let rows = 0;
const emit = (fn: string, id: string, args: unknown[], out: unknown): void => {
  rows += 1;
  console.log(JSON.stringify({ fn, id, args: args.map(tag), out: tag(out) }));
};

// --- The selections --------------------------------------------------------

const STORE: ScenarioSelection = {
  kind: 'store',
  mountPointId: 'mp-9',
  path: 'Scenarios/the-attic.md',
  label: 'Farhouse › The Attic',
};

const SELECTIONS: Array<[string, ScenarioSelection]> = [
  ['custom', { kind: 'custom' }],
  ['character', { kind: 'character', scenarioId: 'sc-1' }],
  ['project', { kind: 'project', path: 'Scenarios/a.md' }],
  ['general', { kind: 'general', path: 'Scenarios/b.md' }],
  ['group', { kind: 'group', groupId: 'g-1', path: 'Scenarios/c.md' }],
  ['group colon path', { kind: 'group', groupId: 'g-1', path: 'Scenarios/c:d.md' }],
  ['store', STORE],
  ['store colon path', { ...STORE, path: 'Scenarios/a:b.md', label: 'X › a:b' }],
  ['store empty label', { ...STORE, label: '' }],
  ['store unicode', { kind: 'store', mountPointId: 'mp-ü', path: 'Scenarios/Café Noir.md', label: 'Ünë › Café' }],
];

// scenarioSelectionToValue + scenarioSelectionToPayload — every kind.
for (const [id, selection] of SELECTIONS) {
  emit('scenarioSelectionToValue', id, [selection], scenarioSelectionToValue(selection));
}
for (const [id, selection] of SELECTIONS) {
  emit('scenarioSelectionToPayload', id, [selection], scenarioSelectionToPayload(selection));
}

// --- The tokens ------------------------------------------------------------

const TOKENS: string[] = [
  '',
  '__custom__',
  '__browse__',
  'store:',
  'store::Scenarios/x.md',
  'store:mp-9',
  'store:mp-9:',
  'store:mp-9:Scenarios/the-attic.md',
  'store:mp-9:Scenarios/a:b.md',
  'store:mp-9:Scenarios/UPPER.MD',
  'store:mp-9:Scenarios/x.md.md',
  'store:mp-9:nested/Scenarios/x.md',
  'store:mp-9:the-attic',
  'store:mp-other:Scenarios/the-attic.md',
  'store',
  'STORE:mp-9:Scenarios/x.md',
  'project:Scenarios/a.md',
  'project:',
  'general:Scenarios/b.md',
  'group:g-1:Scenarios/c.md',
  'group:g-1',
  'group::Scenarios/c.md',
  'sc-uuid',
  ' ',
];

const CURRENTS: Array<[string, ScenarioSelection | undefined]> = [
  ['no current', undefined],
  ['current same store', STORE],
  ['current store other mount', { ...STORE, mountPointId: 'mp-other' }],
  ['current store other path', { ...STORE, path: 'Scenarios/elsewhere.md' }],
  ['current general', { kind: 'general', path: 'Scenarios/the-attic.md' }],
  ['current custom', { kind: 'custom' }],
];

// scenarioValueToSelection(value, current?) — every token × every current.
for (const token of TOKENS) {
  for (const [cid, current] of CURRENTS) {
    const out = current === undefined ? scenarioValueToSelection(token) : scenarioValueToSelection(token, current);
    emit('scenarioValueToSelection', `${JSON.stringify(token)} / ${cid}`, [token, current], out);
  }
}

// Round trip: value → selection (with itself as current) → value.
for (const [id, selection] of SELECTIONS) {
  const value = scenarioSelectionToValue(selection);
  emit('roundTrip', id, [selection], scenarioValueToSelection(value, selection));
}

// --- The seed --------------------------------------------------------------

/** Each default as absent / null / empty string / a value. */
const FIELD_STATES: Array<[string, unknown]> = [
  ['absent', UNDEFINED],
  ['null', null],
  ['empty', ''],
  ['set', 'SET'],
];

function defaults(c: unknown, p: unknown, g: unknown): ScenarioDefaults {
  const d: Record<string, unknown> = {};
  if (c !== UNDEFINED) d.characterDefaultScenarioId = c === 'SET' ? 'sc-study' : c;
  if (p !== UNDEFINED) d.projectDefaultPath = p === 'SET' ? 'Scenarios/atrium.md' : p;
  if (g !== UNDEFINED) d.generalDefaultPath = g === 'SET' ? 'Scenarios/moor.md' : g;
  return d as ScenarioDefaults;
}

const GRID: Array<[string, ScenarioDefaults]> = [];
for (const [cn, c] of FIELD_STATES) {
  for (const [pn, p] of FIELD_STATES) {
    for (const [gn, g] of FIELD_STATES) {
      GRID.push([`character=${cn} project=${pn} general=${gn}`, defaults(c, p, g)]);
    }
  }
}

// defaultScenarioSelection — the 4³ grid.
for (const [id, d] of GRID) {
  emit('defaultScenarioSelection', id, [d], defaultScenarioSelection(d));
}

// seedScenarioSelection — touched × two prior selections × the grid.
const PRIORS: Array<[string, ScenarioSelection]> = [
  ['prev custom', { kind: 'custom' }],
  ['prev store', STORE],
];
for (const touched of [false, true]) {
  for (const [pid, prevSelection] of PRIORS) {
    for (const [id, d] of GRID) {
      const prev = { scenarioSelection: prevSelection, scenarioTouched: touched };
      emit('seedScenarioSelection', `touched=${touched} ${pid} ${id}`, [prev, d], seedScenarioSelection(prev, d));
    }
  }
}

// --- Tier 2: the browser's pure helpers ------------------------------------
//
// They live in `components/scenario/ScenarioBrowserDialog.tsx`, which imports
// React, `react-markdown` and the BaseModal. Loaded lazily behind the env flag
// so the token / seed rows above never depend on the component graph loading.
// Measured 2026-10-14: `tsx` loads the module from the pin as-is.

async function recordBrowser(): Promise<void> {
  const m = await import('@/components/scenario/ScenarioBrowserDialog');
  const HIERARCHIES: Array<[string, Record<string, unknown>]> = [
    ['full', { projectId: 'proj-1', groupIds: ['g-1'], characterId: 'char-1' }],
    ['empty', {}],
    ['nulls', { projectId: null, groupIds: [], characterId: null }],
  ];
  const SOURCES: Array<[string, Record<string, unknown>]> = [
    ['general', { scope: 'general', id: null, name: 'Quilltap General', mountPointId: 'mp-gen' }],
    ['own project', { scope: 'project', id: 'proj-1', name: 'Glasshouse', mountPointId: 'mp-proj-1' }],
    ['other project', { scope: 'project', id: 'proj-2', name: 'Farhouse', mountPointId: 'mp-far' }],
    ['null project', { scope: 'project', id: null, name: 'Orphan', mountPointId: 'mp-orph' }],
    ['own group', { scope: 'group', id: 'g-1', name: 'The Company', mountPointId: 'mp-g1' }],
    ['other group', { scope: 'group', id: 'g-2', name: 'Strangers', mountPointId: 'mp-g2' }],
    ['own character', { scope: 'character', id: 'char-1', name: 'Alice', mountPointId: 'mp-alice' }],
    ['other character', { scope: 'character', id: 'char-9', name: 'Bob', mountPointId: 'mp-bob' }],
    ['unknown scope', { scope: 'planet', id: 'x', name: 'Mars', mountPointId: 'mp-mars' }],
  ];
  const SCENARIOS: Array<[string, Record<string, unknown>]> = [
    ['plain', { path: 'Scenarios/the-attic.md', filename: 'the-attic.md', name: 'The Attic', isDefault: false, archived: false, excerpt: 'Dust, trunks and a single round window.', updatedAt: '2026-10-01T00:00:00.000Z' }],
    ['with id + description', { path: 'Scenarios/study.md', scenarioId: 'sc-study', filename: 'study.md', name: 'Her Study', description: 'A forgotten room', isDefault: true, archived: true, excerpt: 'Books to the ceiling…', updatedAt: '2026-10-02T00:00:00.000Z' }],
  ];
  for (const [sid, source] of SOURCES) {
    for (const [hid, hierarchy] of HIERARCHIES) {
      emit('isInHierarchy', `${sid} / ${hid}`, [source, hierarchy], m.isInHierarchy(source as never, hierarchy as never));
      for (const [cid, scenario] of SCENARIOS) {
        const full = { ...source, scenarios: [], warnings: [] };
        emit('pickToSelection', `${sid} / ${hid} / ${cid}`, [full, scenario, hierarchy], m.pickToSelection(full as never, scenario as never, hierarchy as never));
      }
    }
  }
  const QUERIES = ['', '   ', 'attic', 'ATTIC', '  attic  ', 'forgotten', 'FORGOTTEN', 'round window', 'glasshouse', 'farhouse', 'study', 'ceiling…', 'conservatory', 'the', 'İ', 'ß'];
  const owner = { scope: 'project', id: 'proj-1', name: 'Glasshouse', mountPointId: 'mp-proj-1', scenarios: [], warnings: [] };
  for (const [cid, scenario] of SCENARIOS) {
    for (const q of QUERIES) {
      emit('scenarioMatchesSearch', `${cid} / ${JSON.stringify(q)}`, [owner, scenario, q], m.scenarioMatchesSearch(owner as never, scenario as never, q));
    }
  }
}

(async () => {
  if (process.env.QT_RECORD_BROWSER === '1') await recordBrowser();
  process.stderr.write(`rows=${rows}\n`);
})().catch((err) => {
  process.stderr.write(`recorder failed: ${err instanceof Error ? err.stack : String(err)}\n`);
  process.exit(1);
});
