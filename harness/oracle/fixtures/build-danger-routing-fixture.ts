/**
 * Fixture builder for the W4.2 dangerous-content provider-routing differential
 * (v4 `resolveProviderForDangerousContent` / `resolveImageProviderForDangerousContent`
 * / `resolveUncensoredImageProfileForReroute`).
 *
 * Seeds `connection_profiles` + `image_profiles` (pinned ids + pinned
 * `apiKeyId` column values) for two users. NO `api_keys` rows are seeded — the
 * key resolution is a canned seam on BOTH sides (the oracle monkey-patches
 * `repos.connections.findApiKeyByIdAndUserId`; the Rust port injects an
 * `ApiKeyResolver` from the same spec map).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-danger-routing.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-routing-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ProfileSpec {
  id: string;
  user: 'A' | 'B';
  name: string;
  provider: string;
  modelName: string;
  baseUrl?: string;
  apiKeyId: string;
  isDangerousCompatible: boolean;
  /** P4.D136 (v4 `a1d88aa3a`, bug 106): the attachment-carry half of the scan. */
  supportsImageUpload?: boolean;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userA: string;
  userB: string;
  connectionProfiles: ProfileSpec[];
  imageProfiles: ProfileSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'danger-routing.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-routing-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();
  const ts = spec.seedTimestamp;
  const uid = (u: 'A' | 'B') => (u === 'A' ? spec.userA : spec.userB);

  for (const cp of spec.connectionProfiles) {
    await repos.connections.create(
      {
        userId: uid(cp.user),
        name: cp.name,
        provider: cp.provider,
        modelName: cp.modelName,
        baseUrl: cp.baseUrl ?? null,
        apiKeyId: cp.apiKeyId,
        isDangerousCompatible: cp.isDangerousCompatible,
        supportsImageUpload: cp.supportsImageUpload ?? false,
      } as never,
      { id: cp.id, createdAt: ts, updatedAt: ts }
    );
  }

  for (const ip of spec.imageProfiles) {
    await repos.imageProfiles.create(
      {
        userId: uid(ip.user),
        name: ip.name,
        provider: ip.provider,
        modelName: ip.modelName,
        baseUrl: ip.baseUrl ?? null,
        apiKeyId: ip.apiKeyId,
        isDangerousCompatible: ip.isDangerousCompatible,
      } as never,
      { id: ip.id, createdAt: ts, updatedAt: ts }
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built danger-routing fixture: ${out} (${spec.connectionProfiles.length} conn, ${spec.imageProfiles.length} image profiles)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`danger-routing fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
