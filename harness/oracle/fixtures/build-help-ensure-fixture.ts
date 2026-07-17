/**
 * P4.d6 fixture builder — one seeded database PER SCENARIO for the
 * `ensureHelpDocsSynced` differential (`help-ensure.json` + the committed tree
 * at `fixtures/help-ensure/help/`).
 *
 * Every row is seeded through v4's REAL repositories with pinned ids and
 * timestamps, so both sides start from byte-identical state. The scenarios vary
 * only the seed — the help tree is shared — so each isolates one trigger path
 * (see help-ensure.json's per-scenario `note`).
 *
 * Writes `$QT_FIXTURE_ENSURE_DIR/qt-ensure-<name>-main.db` for each scenario.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ENSURE_DIR=/tmp/qt-ensure \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-help-ensure-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedDoc {
  id: string;
  path: string;
  hash: 'aurora' | 'brahma';
  embedding: number[] | null;
}

interface Scenario {
  name: string;
  seedProfile: boolean;
  helpDocs: SeedDoc[];
}

interface Spec {
  testPepperBase64: string;
  seedSentinel: string;
  auroraHash: string;
  brahmaHash: string;
  user: { id: string; username: string; createdAt: string; updatedAt: string };
  embeddingProfile: Record<string, unknown> & { id: string; createdAt: string; updatedAt: string };
  scenarios: Scenario[];
}

async function buildScenario(spec: Spec, scenario: Scenario, outDir: string): Promise<void> {
  const out = join(outDir, `qt-ensure-${scenario.name}-main.db`);
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), `qt-ensure-build-${scenario.name}-`));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { HelpDocsRepository } = await import(
    '@/lib/database/repositories/help-docs.repository'
  );
  const { HelpDocSchema } = await import('@/lib/schemas/help-doc.types');
  const { EmbeddingStatusSchema } = await import('@/lib/schemas/embedding-job.types');
  const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { EmbeddingProfileSchema } = await import('@/lib/schemas/profile.types');

  await initializeDatabase();
  // Every collection ensureHelpDocsSynced touches: help_docs (sync), the prune's
  // embedding_status, and the enqueue's users / embedding_profiles /
  // background_jobs. A missing table would make the repo call RETHROW and the
  // differential would compare two identical failures.
  await ensureCollection('help_docs', HelpDocSchema);
  await ensureCollection('embedding_status', EmbeddingStatusSchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);
  await ensureCollection('users', UserSchema);
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);

  // The enqueue reads users.findAll()[0] for the job owner.
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  await repos.users.create(
    { username: spec.user.username } as never,
    { id: spec.user.id, createdAt: spec.user.createdAt, updatedAt: spec.user.updatedAt }
  );

  if (scenario.seedProfile) {
    const { id, createdAt, updatedAt, ...profileData } = spec.embeddingProfile;
    await repos.embeddingProfiles.create(profileData as never, { id, createdAt, updatedAt });
  }

  const helpRepo = new HelpDocsRepository();
  for (const doc of scenario.helpDocs) {
    await helpRepo.create(
      {
        title: `Seeded ${doc.path}`,
        path: doc.path,
        url: '/seeded',
        content: 'seeded content',
        contentHash: doc.hash === 'aurora' ? spec.auroraHash : spec.brahmaHash,
        embedding: doc.embedding,
      } as never,
      { id: doc.id, createdAt: spec.seedSentinel, updatedAt: spec.seedSentinel }
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built ensure fixture: ${out} (${scenario.helpDocs.length} help_docs, ` +
      `profile=${scenario.seedProfile})\n`
  );
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'help-ensure.json'), 'utf8')) as Spec;

  const outDir = process.env.QT_FIXTURE_ENSURE_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_ENSURE_DIR must be set');
  mkdirSync(outDir, { recursive: true });

  const only = process.env.QT_ENSURE_SCENARIO;
  const scenarios = only ? spec.scenarios.filter((s) => s.name === only) : spec.scenarios;
  if (scenarios.length === 0) throw new Error(`no scenario matched ${only}`);

  for (const scenario of scenarios) {
    await buildScenario(spec, scenario, outDir);
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-ensure fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
