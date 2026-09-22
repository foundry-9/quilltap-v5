/**
 * Tier-1 oracle case — v4's REAL `isLosslessWebP` (P4.D209 / bug 159,
 * `186eb09cb`).
 *
 * Runs a committed corpus of hand-built RIFF buffers through
 * `lib/mount-index/blob-transcode.ts`'s exported predicate and emits one
 * verdict per case. The buffers are hex in the corpus so both sides receive
 * byte-identical input; the comparison is exact.
 *
 * Also emits `LOSSLESS_WEBP_REENCODE_MIN_BYTES`, so the floor cannot drift on
 * one side without the family noticing.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server        # or a worktree pinned at the baseline
 *   $N/npx tsx $V5W/harness/oracle/cases/blob-transcode-lossless-webp.ts \
 *     > /tmp/oracle-lossless-webp.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

interface Case {
  name: string;
  dataHex: string;
  why: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'blob-transcode-lossless-webp.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as { cases: Case[] };

  process.env.LOG_LEVEL = 'error';
  const { isLosslessWebP, LOSSLESS_WEBP_REENCODE_MIN_BYTES } = await import(
    '@/lib/mount-index/blob-transcode'
  );

  const out = {
    case: 'blob-transcode-lossless-webp',
    losslessWebpReencodeMinBytes: LOSSLESS_WEBP_REENCODE_MIN_BYTES,
    results: spec.cases.map((c) => ({
      name: c.name,
      lossless: isLosslessWebP(Buffer.from(c.dataHex, 'hex')),
    })),
  };
  process.stdout.write(JSON.stringify(out) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`blob-transcode-lossless-webp oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
