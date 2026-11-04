/**
 * Oracle case (tier 1): v4's `fitImageShape` (`plugins/dist/qtap-plugin-nanogpt/
 * image-sizes.ts`, NEW in `4b0d1cb04` — bug 269), driven through v4's REAL
 * export. Pure: no network, no clock, no logging.
 *
 * Rows:
 *   - `v1`…`v10` — v4's own ten vectors (`__tests__/unit/plugins/
 *     nanogpt-image-sizes.test.ts:32-93`), verbatim;
 *   - `g:<catalog>:<size>:<ratio>` — a grid of catalogs × sizes × aspect
 *     ratios (every cell a row);
 *   - `ws-*` — JS `\s` (Unicode incl. U+FEFF / U+00A0) around the spellings;
 *   - `ovf-*` — a digit run past `Number`'s range (`Infinity`);
 *   - `logdist-*` — rows where log distance and linear distance disagree
 *     (the `ratioDistance` mutation's witness).
 *
 * Each row is `{id, catalog?, size?, aspectRatio?, result}`; `undefined`
 * inputs and outputs are ABSENT keys (`JSON.stringify` drops them), which is
 * the shape the Rust side reads and writes.
 *
 * Generate the oracle output (Node 24), from the v4 checkout (a pin worktree
 * when v4 HEAD is past the round's baseline):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx "$V5W/harness/oracle/cases/nanogpt-image-sizes.ts" > /tmp/oracle-nanogpt-image-sizes.ndjson
 * Rust side:
 *   cd "$V5W"
 *   QT_ORACLE_NANOGPT_IMAGE_SIZES=/tmp/oracle-nanogpt-image-sizes.ndjson \
 *     cargo test -p quilltap-harness --test nanogpt_image_sizes_equivalence -- --nocapture
 */

import { fitImageShape, type NanoGPTSizeCatalog } from '@/plugins/dist/qtap-plugin-nanogpt/image-sizes';

const out = (row: Record<string, unknown>) => process.stdout.write(`${JSON.stringify(row)}\n`);

function row(id: string, catalog: NanoGPTSizeCatalog | undefined, size: string | undefined, aspectRatio: string | undefined) {
  out({ id, catalog, size, aspectRatio, result: fitImageShape(catalog, size, aspectRatio) });
}

// ---------------------------------------------------------------------------
// v4's own vectors
// ---------------------------------------------------------------------------

const QWEN3: NanoGPTSizeCatalog = {
  resolutions: ['1k', '2k'],
  aspect_ratio: ['1:1', '3:2', '16:9', '21:9', '9:16', '2:3', '3:4', '4:5'],
};
const STAR_SIX: NanoGPTSizeCatalog = {
  resolutions: ['1024*1024', '1280*720', '720*1280', '1536*1024', '1024*1536', 'auto'],
};
const SQUARES: NanoGPTSizeCatalog = { resolutions: ['512x512', '1024x1024', '2048x2048'] };
const TIERS_ONLY: NanoGPTSizeCatalog = { resolutions: ['1k', '2k'] };

row('v1', QWEN3, '832x1248', undefined);
row('v2', QWEN3, '1248x832', undefined);
row('v3', QWEN3, '1024x1024', undefined);
row('v4', QWEN3, '1536x2304', undefined);
row('v5', QWEN3, '2k', undefined);
row('v6', QWEN3, undefined, '9:16');
row('v7', TIERS_ONLY, '832x1248', undefined);
row('v8', STAR_SIX, '832x1248', undefined);
row('v9', SQUARES, '1000x1000', undefined);
row('v10a', undefined, '832x1248', undefined);
row('v10b', QWEN3, undefined, undefined);

// ---------------------------------------------------------------------------
// The grid
// ---------------------------------------------------------------------------

const CATALOGS: Array<[string, NanoGPTSizeCatalog | undefined]> = [
  ['none', undefined],
  ['qwen3', QWEN3],
  ['star6', STAR_SIX],
  ['squares', SQUARES],
  ['tiers', TIERS_ONLY],
  ['mixedtiers', { resolutions: ['1k', '2K', '1.5k', 'auto'], aspect_ratio: ['1:1', '2:3', '3:2'] }],
  ['empty', { resolutions: [] }],
  ['autoonly', { resolutions: ['auto'] }],
  ['concretewins', { resolutions: ['1k', '1024x1024'], aspect_ratio: ['2:3'] }],
  ['ratioonly', { aspect_ratio: ['2:3'] }],
  ['tiersbadratios', { resolutions: ['1k', '2k'], aspect_ratio: ['x:y', '0:1', ''] }],
  ['tiedratios', { resolutions: ['1k'], aspect_ratio: ['2:3', '3:2', '1:2', '2:1'] }],
];
const SIZES: Array<string | undefined> = [
  '832x1248', '1248x832', '1024x1024', '1536x2304', '2k', '1K', '1024×1024', '1024*1536',
  ' 832 x 1248 ', '832X1248', '0x100', '100x0', 'abc', '', undefined,
];
const RATIOS: Array<string | undefined> = [undefined, '9:16', '16:9', '1:1', '2.5:1', '0:1', 'x:y', ''];

for (const [cname, catalog] of CATALOGS) {
  for (const size of SIZES) {
    for (const ratio of RATIOS) {
      row(`g:${cname}:${size === undefined ? '<u>' : JSON.stringify(size)}:${ratio === undefined ? '<u>' : JSON.stringify(ratio)}`, catalog, size, ratio);
    }
  }
}

// ---------------------------------------------------------------------------
// Unicode whitespace, overflow, the tie-breaks, the log-distance witness
// ---------------------------------------------------------------------------

const WS = [' 832x1248', ' 832x1248', '832x1248 ', '﻿2k', '﻿832x1248', ' 832x1248', '\u0085832x1248', '832　x　1248'];
WS.forEach((s, i) => {
  row(`ws-${i}-tiers`, TIERS_ONLY, s, undefined);
  row(`ws-${i}-squares`, SQUARES, s, undefined);
  row(`ws-${i}-qwen3`, QWEN3, s, undefined);
});
row('ws-ratio-nbsp', QWEN3, undefined, ' 9:16 ');
row('ws-ratio-feff', QWEN3, undefined, '﻿9:16');
row('ws-ratio-nel', QWEN3, undefined, '\u00859:16');
row('ws-tier-k-kelvin', { resolutions: ['1K', '2k'], aspect_ratio: ['1:1'] }, '1024x1024', undefined);
row('ws-tier-k-space', { resolutions: [' 1 k ', '2k'], aspect_ratio: ['1:1'] }, '1024x1024', undefined);
row('ws-dims-arabic-digit', SQUARES, '١024x1024', undefined);
row('ws-dims-fullwidth-x', SQUARES, '1024ｘ1024', undefined);

const NINES = '9'.repeat(400);
row('ovf-w', SQUARES, `${NINES}x1`, undefined);
row('ovf-h', SQUARES, `1x${NINES}`, undefined);
row('ovf-both', SQUARES, `${NINES}x${NINES}`, undefined);
row('ovf-tier', TIERS_ONLY, `${NINES}x1`, undefined);
row('ovf-catalog', { resolutions: [`${NINES}x1`, '1024x1024', `1x${NINES}`] }, '832x1248', undefined);
row('ovf-ratio', QWEN3, undefined, `${NINES}:1`);
row('ovf-tierk', { resolutions: [`${NINES}k`, '1k'], aspect_ratio: ['1:1'] }, '1024x1024', undefined);

row('tie-area-first', { resolutions: ['1000x1000', '1048x1048', '1024x1024'] }, '1024x1024', undefined);
row('tie-area-equal', { resolutions: ['1000x1000', '1048x1048'] }, '1024x1024', undefined);
row('tie-tier-equal', { resolutions: ['1k', '3k', '2k'], aspect_ratio: ['1:1'] }, '2048x2048', undefined);
row('tie-ratio-first', { resolutions: ['1k'], aspect_ratio: ['2:1', '1:2'] }, '1024x1024', undefined);
row('decimal-ratio', { resolutions: ['1k'], aspect_ratio: ['1.5:1', '3:2'] }, '1248x832', undefined);
row('decimal-tier', { resolutions: ['0.5k', '1.25k'], aspect_ratio: ['1:1'] }, '800x800', undefined);
row('leading-zero', SQUARES, '01024x01024', undefined);
row('listed-verbatim-tier', QWEN3, '1k', undefined);
row('listed-verbatim-concrete', SQUARES, '1024x1024', '16:9');

// ratioDistance = |ln a − ln b|. Linear |a − b| picks differently: target 1:1,
// candidates 1:2 (ln-dist 0.693, lin-dist 0.5) and 3:2 (ln-dist 0.405, lin-dist 0.5)
// tie on linear; 1:3 vs 2:1 (lin 0.667 vs 1.0, ln 1.099 vs 0.693) disagree.
row('logdist-concrete', { resolutions: ['512x1536', '2048x1024'] }, '1024x1024', undefined);
row('logdist-tier', { resolutions: ['1k'], aspect_ratio: ['1:3', '2:1'] }, '1024x1024', undefined);
row('logdist-portrait', { resolutions: ['1k'], aspect_ratio: ['1:4', '3:1'] }, '1000x1000', undefined);
