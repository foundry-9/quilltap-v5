/**
 * Oracle case: v4 `lib/services/ai-import.service.ts` — the pure leaf of
 * Summon From Lore (`p4.9k`, P4.9K2 unit 2): the exported assembly functions
 * `assembleWardrobeItems`, `assembleQtapExport`, `restampStructuralFields`,
 * plus `CHARACTER_BASICS_PROMPT`.
 *
 * Drives v4's REAL exports — no reimplementation. The clock is FROZEN at the
 * corpus `frozenNowMs` (the assembler's `new Date()` / `Date.now()`); the
 * `crypto.randomUUID()` mints stay random and are remapped `<minted-N>` in
 * first-seen order by the Rust diff, on both sides. Outputs are emitted as
 * `JSON.stringify` bytes so key ORDER and `undefined` omission are part of the
 * comparand; a throw is emitted as `ok: false` + the message.
 * `restampStructuralFields` MUTATES its argument and returns a count, so its
 * row carries both (`{fixes, data}`).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/ai-import-assembly.ts \
 *     > /tmp/oracle-ai-import-assembly.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  CHARACTER_BASICS_PROMPT,
  assembleWardrobeItems,
  assembleQtapExport,
  restampStructuralFields,
} from '@/lib/services/ai-import.service';

interface Corpus {
  frozenNowMs: number;
  wardrobe: Array<{ id: string; characterId: string; now: string; items: unknown }>;
  export: Array<{
    id: string;
    stepResults: unknown;
    includeMemories: boolean;
    includeChats: boolean;
    appVersion: string;
  }>;
  restamp: Array<{ id: string; now: string; data: unknown }>;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  readFileSync(join(here, '..', 'fixtures', 'ai-import-assembly.json'), 'utf8'),
) as Corpus;

// Freeze the wall clock (constructor + `Date.now`), keeping every other Date use intact.
const RealDate = Date;
const nowMs = corpus.frozenNowMs;
class FrozenDate extends RealDate {
  constructor(...args: unknown[]) {
    if (args.length === 0) {
      super(nowMs);
    } else {
      super(...(args as [number]));
    }
  }
  static now(): number {
    return nowMs;
  }
}
(globalThis as { Date: unknown }).Date = FrozenDate;

const lines: string[] = [];
const covered: string[] = [];

function run(fn: () => string): { ok: boolean; out: string } {
  try {
    return { ok: true, out: fn() };
  } catch (err) {
    return { ok: false, out: err instanceof Error ? err.message : String(err) };
  }
}

lines.push(JSON.stringify({ kind: 'prompt', out: CHARACTER_BASICS_PROMPT }));

for (const row of corpus.wardrobe) {
  const r = run(() =>
    JSON.stringify(assembleWardrobeItems(row.items as never, row.characterId, row.now)),
  );
  lines.push(JSON.stringify({ kind: 'wardrobe', id: row.id, ...r }));
  covered.push(`wardrobe:${row.id}`);
}

for (const row of corpus.export) {
  const r = run(() =>
    JSON.stringify(
      assembleQtapExport(
        row.stepResults as never,
        row.includeMemories,
        row.includeChats,
        row.appVersion,
      ),
    ),
  );
  lines.push(JSON.stringify({ kind: 'export', id: row.id, ...r }));
  covered.push(`export:${row.id}`);
}

for (const row of corpus.restamp) {
  const data = JSON.parse(JSON.stringify(row.data));
  const r = run(() => {
    const fixes = restampStructuralFields(data as never, row.now);
    return JSON.stringify({ fixes, data });
  });
  lines.push(JSON.stringify({ kind: 'restamp', id: row.id, ...r }));
  covered.push(`restamp:${row.id}`);
}

lines.push(JSON.stringify({ kind: 'coverage', ids: covered }));
process.stdout.write(lines.join('\n') + '\n');
