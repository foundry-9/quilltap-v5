/**
 * P4.D381 pure tier-1 oracle (v4 `bf7a457f6`): `lib/files/cloud-placeholder.ts`
 * whole — `STAT_BLOCK_BYTES`, `isNotFullyMaterialized`, and
 * `isEvictedCloudPlaceholder(stat, platform)` — driven through v4's REAL
 * module over v4's ten `cloud-placeholder.test.ts` vectors plus a grid
 * (size × blocks × platform). The Rust side
 * (`cloud_placeholder_equivalence`) runs the same rows through
 * `quilltap_core::services::mount_index::cloud_placeholder` and compares
 * exactly.
 *
 * Run from inside the (pinned) server checkout:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/cloud-placeholder.ts \
 *     > /tmp/oracle-cloud-placeholder.ndjson
 */

import {
  STAT_BLOCK_BYTES,
  isEvictedCloudPlaceholder,
  isNotFullyMaterialized,
} from '@/lib/files/cloud-placeholder';

process.stdout.write(JSON.stringify({ id: 'const', statBlockBytes: STAT_BLOCK_BYTES }) + '\n');

// v4's ten test vectors, by `it` title.
const vectors: Array<[string, number, number, string]> = [
  ['v4-evicted-macos', 118035, 0, 'darwin'],
  ['v4-evicted-windows', 4013, 0, 'win32'],
  ['v4-resident', 80241, 160, 'darwin'],
  ['v4-partial-or-compressed', 100_000, 8, 'darwin'],
  ['v4-empty', 0, 0, 'darwin'],
  ['v4-linux', 4096, 0, 'linux'],
  ['v4-mat-evicted', 4096, 0, 'darwin'],
  ['v4-mat-partial', 100_000, 8, 'darwin'],
  ['v4-mat-rounds-up', 1000, 8, 'darwin'],
  ['v4-mat-empty', 0, 0, 'darwin'],
];

const rows: Array<[string, number, number, string]> = [...vectors];
for (const size of [0, 1, 511, 512, 513, 4096, 118035]) {
  for (const blocks of [0, 1, 8, 160]) {
    for (const platform of ['darwin', 'win32', 'linux', 'freebsd']) {
      rows.push([`grid-${size}-${blocks}-${platform}`, size, blocks, platform]);
    }
  }
}

for (const [id, size, blocks, platform] of rows) {
  const stat = { size, blocks };
  process.stdout.write(
    JSON.stringify({
      id,
      size,
      blocks,
      platform,
      evicted: isEvictedCloudPlaceholder(stat, platform as NodeJS.Platform),
      notFullyMaterialized: isNotFullyMaterialized(stat),
    }) + '\n',
  );
}
