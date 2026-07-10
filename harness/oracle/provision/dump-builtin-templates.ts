/**
 * Data-module generator for the built-in roleplay templates (P4.4u3, family 1).
 *
 * Emits `crates/quilltap-core/src/services/builtin_templates.json` — the verbatim
 * "Standard" / "Quilltap RP" seed data the v5 seeder replays. The bytes come from
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
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
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

  await initializeDatabase();
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);

  const repo = new RoleplayTemplatesRepository();
  // Seed twice: the FIRST run inserts (schema-order delimiters); the SECOND run
  // takes the update path, rewriting `delimiters` in SEED-LITERAL order — which is
  // what a real upgraded v4 instance carries and what the v5 UPDATE path replays.
  await repo.seedBuiltInTemplates();
  await repo.seedBuiltInTemplates();

  const rows = (await rawQuery(
    "SELECT name, description, systemPrompt, delimiters, renderingPatterns, dialogueDetection, narrationDelimiters FROM roleplay_templates WHERE isBuiltIn = 1 ORDER BY name",
  )) as Array<Record<string, string | null>>;

  await closeDatabase();

  // `narrationDelimiters` is stored as plain TEXT for the string form and JSON
  // array text for the pair form (v4's union is NOT a registered JSON column, so
  // documentToRow only JSON-encodes it when the value is an array). Recover the
  // structured value: a leading '[' means the pair form.
  const parseNarration = (raw: string): unknown => {
    if (raw.startsWith('[')) return JSON.parse(raw);
    return raw;
  };

  const templates = rows.map((r) => ({
    name: r.name,
    description: r.description,
    systemPrompt: r.systemPrompt,
    // SEED-LITERAL key order (from the second, update-path seed).
    delimiters: JSON.parse(r.delimiters as string),
    renderingPatterns: JSON.parse(r.renderingPatterns as string),
    dialogueDetection:
      r.dialogueDetection == null ? null : JSON.parse(r.dialogueDetection),
    narrationDelimiters: parseNarration(r.narrationDelimiters as string),
  }));

  // Order the two built-ins the way v4's BUILT_IN_TEMPLATES literal lists them
  // (Standard first, then Quilltap RP) for a stable, readable module.
  const rank = (name: string | null) => (name === 'Standard' ? 0 : 1);
  templates.sort((a, b) => rank(a.name) - rank(b.name));

  writeFileSync(out, JSON.stringify(templates, null, 2) + '\n');
  process.stderr.write(`wrote ${templates.length} built-in templates → ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`dump-builtin-templates failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
