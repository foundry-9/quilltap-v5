/**
 * Data-module generator for the built-in roleplay templates (P4.4u3, family 1).
 *
 * Emits `crates/quilltap-core/src/services/builtin_templates.json` — the verbatim
 * "Standard" / "Quilltap RP" / "Novel" seed data the v5 seeder replays (Novel's
 * `renderingPatterns` are COMPUTED by v4's generator at module load — never
 * hand-transcribed). `narrationDelimiters` is dumped DECODED through v4's real
 * codec; `unmarkedStyle` / `markdownAsterisks` ride along (P4.D341). The bytes come from
 * v4's REAL `seedBuiltInTemplates()`, so there is no hand transcription of the
 * (large, whitespace-sensitive) system prompts.
 *
 * v4's seeder has TWO write paths with DIFFERENT `delimiters` key orders:
 *   - INSERT (`collection.insertOne(validate(...))`) → Zod parse → SCHEMA order;
 *   - UPDATE (`collection.updateOne({id}, {$set: {delimiters: template.delimiters,
 *     ...}})`) → the RAW seed literal → SEED-LITERAL order.
 * A fresh v5 instance runs the INSERT path; an upgraded instance runs the UPDATE
 * path. To carry both, this generator seeds TWICE (insert, then update) and dumps
 * the resulting row — so `delimiters` here is in SEED-LITERAL order (what the
 * UPDATE path stores). The v5 seeder parses it into the typed union for the
 * INSERT path (which re-emits SCHEMA order, matching v4's Zod parse) and writes it
 * raw for the UPDATE path. Every byte is proven by `builtin_templates_equivalence`.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/dump-builtin-templates.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';

async function main(): Promise<void> {
  // Default to the checked-in module path (relative to this script), but allow an
  // override so the script can run from a /tmp mirror (tsx's `@/` alias breaks on
  // `.claude/worktrees/...` paths — the standing worktree gotcha).
  const here = dirname(fileURLToPath(import.meta.url));
  const out =
    process.env.QT_BUILTIN_OUT ??
    join(
      here,
      '..',
      '..',
      '..',
      'crates',
      'quilltap-core',
      'src',
      'services',
      'builtin_templates.json',
    );

  const scratch = mkdtempSync(join(tmpdir(), 'qt-builtin-templates-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainPath = join(scratch, 'quilltap.db');

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainPath;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, rawQuery, closeDatabase } =
    await import('@/lib/database/manager');
  const { RoleplayTemplatesRepository } = await import(
    '@/lib/database/repositories/roleplay-templates.repository'
  );
  const { RoleplayTemplateSchema } = await import('@/lib/schemas/template.types');
  // P4.D341: v4 now writes `narrationDelimiters` as JSON text (`"*"`,
  // `["[","]"]`, `null`) and decodes it on every read — decode the stored cell
  // through v4's REAL codec so the data module carries the DECODED value (the v5
  // seeder re-encodes it on both write paths).
  const { decodeNarrationDelimiters } = await import(
    '@/lib/database/repositories/roleplay-template-narration-codec'
  );

  await initializeDatabase();
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);

  const repo = new RoleplayTemplatesRepository();
  // Seed twice: the FIRST run inserts (schema-order delimiters); the SECOND run
  // takes the update path, rewriting `delimiters` in SEED-LITERAL order — which is
  // what a real upgraded v4 instance carries and what the v5 UPDATE path replays.
  await repo.seedBuiltInTemplates();
  await repo.seedBuiltInTemplates();

  const rows = (await rawQuery(
    "SELECT name, description, systemPrompt, delimiters, renderingPatterns, dialogueDetection, narrationDelimiters, unmarkedStyle, markdownAsterisks FROM roleplay_templates WHERE isBuiltIn = 1 ORDER BY name",
  )) as Array<Record<string, string | number | null>>;

  await closeDatabase();

  const templates = rows.map((r) => ({
    name: r.name,
    description: r.description,
    systemPrompt: r.systemPrompt,
    // SEED-LITERAL key order (from the second, update-path seed).
    delimiters: JSON.parse(r.delimiters as string),
    renderingPatterns: JSON.parse(r.renderingPatterns as string),
    dialogueDetection:
      r.dialogueDetection == null ? null : JSON.parse(r.dialogueDetection as string),
    // The stored cell is the codec's JSON text; carry the DECODED value
    // ('*', ['[', ']'], or null for Novel's unmarked prose).
    narrationDelimiters: decodeNarrationDelimiters(r.narrationDelimiters),
    // SEED-LITERAL key order (update path); `null` for the two legacy built-ins.
    unmarkedStyle: r.unmarkedStyle == null ? null : JSON.parse(r.unmarkedStyle as string),
    markdownAsterisks: r.markdownAsterisks === 1,
  }));

  // Order the built-ins the way v4's BUILT_IN_TEMPLATES literal lists them
  // (Standard, Quilltap RP, Novel) for a stable, readable module.
  const ORDER = ['Standard', 'Quilltap RP', 'Novel'];
  const rank = (name: unknown) => {
    const i = ORDER.indexOf(name as string);
    if (i < 0) throw new Error(`unexpected built-in template ${String(name)}`);
    return i;
  };
  templates.sort((a, b) => rank(a.name) - rank(b.name));
  if (templates.length !== ORDER.length) {
    throw new Error(`expected ${ORDER.length} built-ins, dumped ${templates.length}`);
  }

  writeFileSync(out, JSON.stringify(templates, null, 2) + '\n');
  process.stderr.write(`wrote ${templates.length} built-in templates → ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`dump-builtin-templates failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
