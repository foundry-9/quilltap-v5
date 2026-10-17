/**
 * Tier-1 oracle case — v4's markdown outfit description (`lib/wardrobe/
 * outfit-description.ts` `describeOutfit(slots, { omit })`), P4.D312 / v4
 * `7458ff94e` (the hair-styling precedence note).
 *
 * Drives v4's REAL `describeOutfit` over the shared corpus
 * `harness/oracle/fixtures/describe-outfit.json`, which the Rust side
 * (`describe_outfit_equivalence`) reads too — both implementations render the
 * SAME inputs and the strings are compared exactly. The corpus holds the whole
 * 243-row product (each of the five slots empty / one item / two items), the
 * hand rows (hair alone, hair sharing a value with another slot, hair omitted,
 * a hair value equal to a garment fallback, an em dash and a non-BMP
 * character), and every `omit` subset over a styled and a hair-only outfit.
 *
 * Before this case no oracle imported `describeOutfit` directly; its eight v4
 * callers reached it only by composition.
 *
 * Pure function — no DB, no jest. Emits one NDJSON row per corpus row:
 * `{ name, out }`, `out` the exact rendered string.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx "$V5W/harness/oracle/cases/describe-outfit.ts" > /tmp/oracle-describe-outfit.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { describeOutfit } from '@/lib/wardrobe/outfit-description';

type Slot = 'top' | 'bottom' | 'footwear' | 'accessories' | 'hair';

interface Row {
  name: string;
  slots: Record<Slot, string[]>;
  omit: Slot[];
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'describe-outfit.json'), 'utf8'),
) as { rows: Row[] };

for (const row of corpus.rows) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const out = describeOutfit(row.slots as any, { omit: row.omit as any });
  process.stdout.write(JSON.stringify({ name: row.name, out }) + '\n');
}
