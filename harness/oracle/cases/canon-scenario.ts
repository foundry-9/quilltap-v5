/**
 * Oracle case #21 (Wave 4 / B12): canon-block renderers + scenario-text combiner.
 *
 * Drives the REAL pure helpers:
 *   renderSelfCanonBlock, renderOtherCanonBlock, loadCanonForSelf,
 *     NO_CANON_FALLBACK (lib/memory/cheap-llm-tasks/canon.ts)
 *   combineScenarioText (lib/chat/scenario-text.ts)
 * No impurity; no injection. `loadCanonForObserverAboutSubject` is impure
 * (vault I/O) and stays out of the port.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/canon-scenario.ts \
 *     > /tmp/oracle-canon-scenario.ndjson
 */

import {
  renderSelfCanonBlock,
  renderOtherCanonBlock,
  loadCanonForSelf,
  NO_CANON_FALLBACK,
  type SelfCanon,
  type CanonSource,
} from '@/lib/memory/cheap-llm-tasks/canon';
import { combineScenarioText } from '@/lib/chat/scenario-text';

const rows: unknown[] = [];

// ---- renderSelfCanonBlock -------------------------------------------------
const selfCases: Array<[string, SelfCanon]> = [
  [
    'all-fields',
    {
      characterId: 'c1',
      characterName: 'Friday',
      manifesto: 'I serve.',
      personality: 'Wry.',
      description: 'A valet.',
      identity: 'The Host.',
    },
  ],
  [
    'manifesto-only',
    {
      characterId: 'c2',
      characterName: 'Aurora',
      manifesto: 'Light first.',
      personality: null,
      description: null,
      identity: null,
    },
  ],
  [
    'whitespace-dropped',
    {
      characterId: 'c3',
      characterName: 'Jeeves',
      manifesto: '   ',
      personality: '',
      description: '  Tidy.  ',
      identity: null,
    },
  ],
  [
    'none-present',
    {
      characterId: 'c4',
      characterName: 'Nemo',
      manifesto: null,
      personality: null,
      description: null,
      identity: null,
    },
  ],
];
for (const [id, canon] of selfCases) {
  rows.push({ kind: 'selfBlock', id, canon, out: renderSelfCanonBlock(canon) });
}

// ---- renderOtherCanonBlock ------------------------------------------------
const otherCases: Array<[string, CanonSource]> = [
  ['vault', { characterId: 'o1', characterName: 'Bertie', body: 'Notes from the vault.', source: 'vault' }],
  ['vault-empty-falls-back', { characterId: 'o2', characterName: 'Wooster', body: '   ', source: 'vault' }],
  ['identity', { characterId: 'o3', characterName: 'Carina', body: 'A navigator.', source: 'identity' }],
  ['description', { characterId: 'o4', characterName: 'Pascal', body: 'A tinkerer.', source: 'description' }],
  ['none', { characterId: 'o5', characterName: 'Prospero', body: null, source: 'none' }],
  ['identity-trimmed', { characterId: 'o6', characterName: 'Quill', body: '  Scribe.  ', source: 'identity' }],
];
for (const [id, canon] of otherCases) {
  rows.push({ kind: 'otherBlock', id, canon, out: renderOtherCanonBlock(canon) });
}

// ---- loadCanonForSelf -----------------------------------------------------
const loadCases: Array<[string, Parameters<typeof loadCanonForSelf>[0]]> = [
  [
    'full',
    { id: 'l1', name: 'Friday', manifesto: 'M', personality: 'P', description: 'D', identity: 'I' },
  ],
  [
    'nulls',
    { id: 'l2', name: 'Empty', manifesto: null, personality: null, description: null, identity: null },
  ],
];
for (const [id, character] of loadCases) {
  rows.push({ kind: 'loadSelf', id, character, out: loadCanonForSelf(character) });
}

// ---- combineScenarioText --------------------------------------------------
const scenarioCases: Array<[string, string | null | undefined, string | null | undefined]> = [
  ['both', 'Preset body.', 'Free notes.'],
  ['preset-only', 'Preset body.', null],
  ['free-only', null, 'Free notes.'],
  ['both-empty', '', '   '],
  ['both-null', null, null],
  ['preset-keeps-leading-ws', '   Indented preset.  ', 'Notes.'],
  ['free-trimmed', 'Preset.', '   trimmed free   '],
  ['preset-whitespace-only', '   ', 'Only free survives.'],
];
for (const [id, presetBody, freeText] of scenarioCases) {
  rows.push({
    kind: 'scenario',
    id,
    presetBody: presetBody ?? null,
    freeText: freeText ?? null,
    out: combineScenarioText(presetBody, freeText) ?? null,
  });
}

// ---- NO_CANON_FALLBACK constant -------------------------------------------
rows.push({ kind: 'fallbackConst', id: 'no-canon', value: NO_CANON_FALLBACK });

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
