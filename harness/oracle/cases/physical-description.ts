/**
 * Tier-1 oracle case — the physical-description pick (v4
 * `lib/characters/physical-description.ts` `pickPhysicalDescription` /
 * `hasPhysicalDescription`), P4.D283 / v4 `9ecebf986`.
 *
 * Drives v4's REAL module over the shared corpus
 * `harness/oracle/fixtures/physical-description.json`, which the Rust side
 * reads too — so both implementations pick from the SAME `desc` objects and
 * the answers are compared exactly. Every case runs under all five profiles.
 *
 * One sentinel: a `desc` of `"__UNDEFINED__"` becomes a real `undefined`
 * (v4's `if (!desc) return ''` arm); JSON `null` is passed as null.
 *
 * Pure functions — no DB, no jest. Emits one NDJSON row per (case, profile):
 * `{ name, profile, pick, has }`.
 *
 * Run (Node 24, from the v4 checkout — the module arrived at `9ecebf986`,
 * past the `01a83539d` baseline, so pin the checkout at or after it until the
 * baseline moves):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/physical-description.ts \
 *     > /tmp/oracle-physical-description.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  hasPhysicalDescription,
  pickPhysicalDescription,
  type PhysicalDescriptionProfile,
  type PhysicalDescriptionVariants,
} from '@/lib/characters/physical-description';

interface Case {
  name: string;
  desc: PhysicalDescriptionVariants | null | '__UNDEFINED__';
}

const PROFILES: readonly PhysicalDescriptionProfile[] = [
  'head-and-shoulders',
  'full-length',
  'scene',
  'self-image',
  'fullest',
];

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'physical-description.json'), 'utf8'),
) as { cases: Case[] };

const lines: string[] = [];
for (const c of corpus.cases) {
  const desc = c.desc === '__UNDEFINED__' ? undefined : c.desc;
  for (const profile of PROFILES) {
    lines.push(
      JSON.stringify({
        name: c.name,
        profile,
        pick: pickPhysicalDescription(desc, profile),
        has: hasPhysicalDescription(desc, profile),
      }),
    );
  }
}
process.stdout.write(lines.join('\n') + '\n');
