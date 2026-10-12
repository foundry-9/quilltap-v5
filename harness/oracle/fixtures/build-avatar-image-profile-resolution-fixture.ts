/**
 * Tier-2 fixture builder — materializes the shared starting DB for the
 * `resolveAvatarImageProfile` family (P4.D279 §8) from the committed spec
 * (`avatar-image-profile-resolution-tier2.json`).
 *
 * Same shape as build-image-profiles-fixture.ts: the `image_profiles` table is
 * created by v4's OWN `ensureCollection('image_profiles', ImageProfileSchema)`
 * and every seed row is inserted through the real `ImageProfilesRepository
 * .create` with id + timestamps pinned (CreateOptions), so the starting state
 * is deterministic. Then `corruptNameIds` get a BLOB `name` through the raw
 * client (the P4.139 corrupt-cell idiom) — the one way to make v4's
 * `findById` / `findOneByFilter` FAIL inside their fallback `safeQuery`s, since
 * a missing table is healed by `getCollection` and is not a v4 arm.
 *
 * The output is the SEED-ONLY starting state. The resolution matrix is run
 * later, by `cases/avatar-image-profile-resolution.ts` (oracle) and the Rust
 * harness, each on its own fresh copy.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-avatar-profile-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-avatar-image-profile-resolution-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  userId: string;
  name: string;
  provider: string;
  apiKeyId: string | null;
  baseUrl: string | null;
  modelName: string;
  parameters: Record<string, unknown>;
  isDefault: boolean;
  isDangerousCompatible: boolean;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

interface Spec {
  testPepperBase64: string;
  seed: SeedRow[];
  corruptNameIds: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'avatar-image-profile-resolution-tier2.json');
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
  // startup physical backup, sibling llm-logs / mount-index DBs). The MAIN db
  // still lands at SQLITE_PATH.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-avatar-profile-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { ImageProfilesRepository } = await import(
    '@/lib/database/repositories/image-profiles.repository'
  );
  const { ImageProfileSchema } = await import('@/lib/schemas/types');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');

  await initializeDatabase();
  await ensureCollection('image_profiles', ImageProfileSchema);

  const repo = new ImageProfilesRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        userId: row.userId,
        name: row.name,
        provider: row.provider,
        apiKeyId: row.apiKeyId,
        baseUrl: row.baseUrl,
        modelName: row.modelName,
        parameters: row.parameters,
        isDefault: row.isDefault,
        isDangerousCompatible: row.isDangerousCompatible,
        tags: row.tags,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  // The corrupt-cell plant: a BLOB where the schema wants a string, so the
  // row exists for the filter but fails validation on read.
  const raw = getRawDatabase();
  if (!raw) throw new Error('raw sqlite client unavailable after initializeDatabase');
  for (const id of spec.corruptNameIds) {
    const r = raw.prepare("UPDATE image_profiles SET name = x'00000000' WHERE id = ?").run(id);
    if (r.changes !== 1) throw new Error(`the corrupt-name plant missed ${id}`);
  }

  await closeDatabase();

  process.stderr.write(
    `built avatar-profile-resolution fixture: ${out} (${spec.seed.length} seed rows, ${spec.corruptNameIds.length} corrupted)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
