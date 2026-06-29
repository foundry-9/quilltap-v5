/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `roleplay_templates` repo (Phase-2) from the committed plaintext spec
 * (`roleplay-templates-tier2.json`).
 *
 * Same shape as build-text-replacement-rules-fixture.ts: the `roleplay_templates`
 * table is created by v4's OWN `ensureCollection('roleplay_templates',
 * RoleplayTemplateSchema)` so the DDL (column set/order, the boolean + JSON column
 * registration) is identical to production by construction. Seed rows are
 * inserted via the real `RoleplayTemplatesRepository.create` with id + timestamps
 * pinned (CreateOptions), so the starting state is fully deterministic. JS
 * objects/arrays are passed straight through for the JSON columns (`tags`,
 * `renderingPatterns`, `dialogueDetection`, `delimiters`).
 *
 * The output file is the SEED-ONLY starting state. The op sequence under test is
 * applied later, by `cases/roleplay-templates-tier2.ts` (oracle) and the Rust
 * harness, each on its own fresh copy.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-rt-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-roleplay-templates-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface RenderingPattern {
  pattern: string;
  className: string;
  flags?: string;
  scope?: string;
  hideDelimiters?: boolean;
}

interface DialogueDetection {
  openingChars: string[];
  closingChars: string[];
  className: string;
}

interface Spec {
  testPepperBase64: string;
  seed: Array<{
    id: string;
    userId: string | null;
    name: string;
    description: string | null;
    systemPrompt: string;
    isBuiltIn: boolean;
    tags: string[];
    renderingPatterns: RenderingPattern[];
    dialogueDetection: DialogueDetection | null;
    narrationDelimiters: string;
    createdAt: string;
    updatedAt: string;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'roleplay-templates-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }

  // Fresh output: drop any prior fixture so we never seed on top of stale state.
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  // Throwaway data dir absorbs v4's operational scaffolding (instance lock,
  // startup physical backup, sibling llm-logs / mount-index DBs). A unique dir
  // per run avoids stale-lock collisions. The MAIN db still lands at SQLITE_PATH.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-rt-fixture-build-'));
  // v4 nests its working files under `<dataDir>/data/` (instance lock, sibling
  // DBs). Pre-create it so `acquireInstanceLock` can open the lock file.
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error'; // keep stdout/stderr quiet for clean runs

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { RoleplayTemplatesRepository } = await import(
    '@/lib/database/repositories/roleplay-templates.repository'
  );
  const { RoleplayTemplateSchema } = await import('@/lib/schemas/template.types');

  await initializeDatabase();
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);

  const repo = new RoleplayTemplatesRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        userId: row.userId,
        name: row.name,
        description: row.description,
        systemPrompt: row.systemPrompt,
        isBuiltIn: row.isBuiltIn,
        tags: row.tags,
        delimiters: [],
        renderingPatterns: row.renderingPatterns,
        dialogueDetection: row.dialogueDetection,
        narrationDelimiters: row.narrationDelimiters,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built roleplay_templates fixture: ${out} (${spec.seed.length} seed rows)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
