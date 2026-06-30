/**
 * Tier-1 oracle — the wardrobe YAML emitter (Decision A: the only eemeli/yaml
 * site). Drives v4's REAL `buildWardrobeItemFile` + `buildSlugByItemIdMap`
 * (lib/mount-index/character-vault) over the corpus and emits, per case, the
 * `Wardrobe/*.md` file content for each item. The Rust port
 * (vault_overlay::build_wardrobe_item_file) must reproduce each byte-for-byte.
 *
 * No DB — pure functions. Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-emit.ts \
 *     > /tmp/oracle-vault-wardrobe-emit.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

import type { WardrobeItem } from '@/lib/schemas/wardrobe.types';
import {
  buildWardrobeItemFile,
  buildSlugByItemIdMap,
} from '@/lib/mount-index/character-vault';

interface Spec {
  cases: WardrobeItem[][];
}

function main(): void {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-wardrobe-emit.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const results: string[][] = spec.cases.map((items) => {
    const slugByItemId = buildSlugByItemIdMap(items);
    return items.map((it) => buildWardrobeItemFile(it, slugByItemId));
  });

  process.stdout.write(JSON.stringify({ results }) + '\n');
  process.exit(0);
}

main();
