/**
 * Oracle case: v4 `lib/wardrobe/generated-items.ts` — the shared shape,
 * generation prompt and sanitizer every AI wardrobe generator uses (the AI
 * Wizard, Summon From Lore, the character optimizer's wardrobe suggestions).
 * `p4.9k`, P4.9K1/K2 — the shared leaf K0 did not port.
 *
 * Drives v4's REAL exports — no reimplementation:
 *   WARDROBE_ITEMS_GENERATION_PROMPT   (wire bytes — reaches a paid model verbatim)
 *   sanitizeGeneratedWardrobeItems     (over the committed corpus rows)
 *   orderGeneratedItemsLeafFirst       (over the committed corpus rows)
 *
 * Outputs are emitted as `JSON.stringify` bytes so key ORDER and `undefined`
 * omission are part of the comparand (a sanitized item that clears
 * `components`/`replace` drops those keys entirely).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/generated-wardrobe-items.ts \
 *     > /tmp/oracle-generated-wardrobe-items.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  WARDROBE_ITEMS_GENERATION_PROMPT,
  sanitizeGeneratedWardrobeItems,
  orderGeneratedItemsLeafFirst,
} from '@/lib/wardrobe/generated-items';

interface Corpus {
  sanitize: Array<{ id: string; items: unknown }>;
  order: Array<{ id: string; items: unknown }>;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  readFileSync(join(here, '..', 'fixtures', 'generated-wardrobe-items.json'), 'utf8'),
) as Corpus;

const lines: string[] = [];
const covered: string[] = [];

lines.push(JSON.stringify({ kind: 'prompt', out: WARDROBE_ITEMS_GENERATION_PROMPT }));

for (const row of corpus.sanitize) {
  let out: string;
  let ok = true;
  try {
    out = JSON.stringify(sanitizeGeneratedWardrobeItems(row.items as never));
  } catch (err) {
    ok = false;
    out = err instanceof Error ? err.message : String(err);
  }
  lines.push(JSON.stringify({ kind: 'sanitize', id: row.id, ok, out }));
  covered.push(`sanitize:${row.id}`);
}

for (const row of corpus.order) {
  let out: string;
  let ok = true;
  try {
    out = JSON.stringify(orderGeneratedItemsLeafFirst(row.items as never));
  } catch (err) {
    ok = false;
    out = err instanceof Error ? err.message : String(err);
  }
  lines.push(JSON.stringify({ kind: 'order', id: row.id, ok, out }));
  covered.push(`order:${row.id}`);
}

lines.push(JSON.stringify({ kind: 'coverage', ids: covered }));
process.stdout.write(lines.join('\n') + '\n');
