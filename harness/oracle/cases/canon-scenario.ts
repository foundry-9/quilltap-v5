/**
 * Oracle case #21 (Wave 4 / B12): canon-block renderers + scenario-text combiner.
 *
 * Drives the REAL pure helpers:
 *   renderSelfCanonBlock, renderOtherCanonBlock, loadCanonForSelf,
 *     NO_CANON_FALLBACK, CANON_BLOCK_TOKEN_CAP (lib/memory/cheap-llm-tasks/canon.ts)
 *     — incl. the `d58548051` Commonplace digest arms + the block cap
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
  CANON_BLOCK_TOKEN_CAP,
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

// ---- Commonplace digests (v4 `d58548051`: appendCommonplace + the cap) -----
// The block is capped at CANON_BLOCK_TOKEN_CAP; only the digest yields.
const digest = '- Laura prefers tea now.\n- Laura keeps the charts.';
const manyLines = Array.from({ length: 300 }, (_, i) => `- Fact number ${i} about the charts she keeps.`).join('\n');
const oneLongLine = `- ${'w'.repeat(12000)} end`;
const commonplaceSelf: Array<[string, SelfCanon]> = [
  ['cp-self-digest-after-card', { characterId: 's1', characterName: 'Friday', manifesto: 'I serve.', personality: null, description: null, identity: null, commonplace: digest }],
  ['cp-self-all-empty-plus-digest', { characterId: 's2', characterName: 'Friday', manifesto: null, personality: '  ', description: null, identity: null, commonplace: digest }],
  ['cp-self-whitespace-digest-keeps-fallback', { characterId: 's3', characterName: 'Friday', manifesto: null, personality: null, description: null, identity: null, commonplace: ' \n\t ' }],
  ['cp-self-null-digest', { characterId: 's4', characterName: 'Friday', manifesto: 'M', personality: null, description: null, identity: null, commonplace: null }],
  ['cp-self-digest-lines-cut-near-cap', { characterId: 's5', characterName: 'Friday', manifesto: 'm'.repeat(5000), personality: null, description: null, identity: null, commonplace: manyLines }],
  ['cp-self-room-exhausted-bare-block', { characterId: 's6', characterName: 'Friday', manifesto: 'm'.repeat(9000), personality: null, description: null, identity: null, commonplace: digest }],
  ['cp-self-digest-truncates-to-null-bare-head', { characterId: 's7', characterName: 'N'.repeat(9000), manifesto: null, personality: null, description: null, identity: null, commonplace: digest }],
  ['cp-self-one-long-digest-line-proportional', { characterId: 's8', characterName: 'Friday', manifesto: null, personality: null, description: null, identity: 'Host.', commonplace: oneLongLine }],
  ['cp-self-bom-and-nel-padded-fields', { characterId: 's9', characterName: 'Friday', manifesto: '\uFEFFI serve.\uFEFF', personality: '\u0085Wry.\u0085', description: '\uFEFF', identity: '\u0085', commonplace: '\uFEFF- padded digest\uFEFF' }],
];
for (const [id, canon] of commonplaceSelf) {
  rows.push({ kind: 'selfBlock', id, canon, out: renderSelfCanonBlock(canon) });
}
const commonplaceOther: Array<[string, CanonSource]> = [
  ['cp-other-vault-then-digest', { characterId: 'p1', characterName: 'Laura', body: 'Laura is my oldest friend.', source: 'vault', commonplace: digest }],
  ['cp-other-identity-then-digest', { characterId: 'p2', characterName: 'Laura', body: 'A cartographer.', source: 'identity', commonplace: digest }],
  ['cp-other-fourth-arm-digest-alone', { characterId: 'p3', characterName: 'Laura', body: null, source: 'none', commonplace: digest }],
  ['cp-other-empty-vault-body-digest-alone', { characterId: 'p4', characterName: 'Laura', body: '   ', source: 'vault', commonplace: digest }],
  ['cp-other-none-no-digest-fallback', { characterId: 'p5', characterName: 'Laura', body: null, source: 'none', commonplace: null }],
  ['cp-other-none-whitespace-digest-fallback', { characterId: 'p6', characterName: 'Laura', body: null, source: 'none', commonplace: '   ' }],
  ['cp-other-hand-near-cap-line-cut', { characterId: 'p7', characterName: 'Laura', body: 'h'.repeat(6000), source: 'vault', commonplace: manyLines }],
  ['cp-other-room-exhausted', { characterId: 'p8', characterName: 'Laura', body: 'h'.repeat(9000), source: 'description', commonplace: digest }],
  ['cp-other-one-long-line-proportional', { characterId: 'p9', characterName: 'Laura', body: 'A cartographer.', source: 'identity', commonplace: oneLongLine }],
  ['cp-other-bom-padded-identity', { characterId: 'p10', characterName: 'Laura', body: '\uFEFFA cartographer.\uFEFF', source: 'identity', commonplace: null }],
  ['cp-other-nel-padded-identity', { characterId: 'p11', characterName: 'Laura', body: '\u0085A cartographer.\u0085', source: 'identity', commonplace: null }],
  ['cp-other-bom-only-body-digest-alone', { characterId: 'p12', characterName: 'Laura', body: '\uFEFF', source: 'identity', commonplace: digest }],
];
for (const [id, canon] of commonplaceOther) {
  rows.push({ kind: 'otherBlock', id, canon, out: renderOtherCanonBlock(canon) });
}
rows.push({ kind: 'canonCap', id: 'cap', value: CANON_BLOCK_TOKEN_CAP });

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
