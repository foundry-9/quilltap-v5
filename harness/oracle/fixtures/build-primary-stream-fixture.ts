/**
 * Tier-3 SEED fixture builder — the primary-stream / recovery / provider-failover
 * services (Phase-3 Unit-3 wave 3).
 *
 * Creates the corpus chats via v4's REAL `repos.chats.create` (ids + timestamps
 * pinned to the sentinel, each with one CHARACTER participant) and materializes
 * the empty `chat_messages` table (via a `getMessageCount` read), so both the
 * jest oracle and the Rust port — neither of which owns DDL here — can INSERT the
 * assistant / recovery messages the services write. Main-DB only: these services
 * touch only `chats` + `chat_messages`.
 *
 * The op sequence itself is NOT run here; both sides run it on a fresh copy.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-primary-stream.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-primary-stream-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ParticipantSpec {
  id: string;
  type: string;
  characterId: string;
}
interface ChatSpec {
  id: string;
  userId: string;
  title: string;
  participants: ParticipantSpec[];
}
interface ProfileSpec {
  id: string;
  userId: string;
  name: string;
  provider: string;
  modelName: string;
  baseUrl: string | null;
  apiKeyId?: string | null;
  modelClass?: string | null;
  fallbackProfileId?: string | null;
  allowTierFallback?: boolean;
}
interface ApiKeySpec {
  id: string;
  provider: string;
  label: string;
  keyValue: string;
}
interface Spec {
  testPepperBase64: string;
  sentinel: string;
  chats: ChatSpec[];
  userId: string;
  profile: ProfileSpec;
  understudyProfile: ProfileSpec;
  tierSpareProfile: ProfileSpec;
  // P4.74 — the credential-gate arm's own company. All three carry
  // `modelClass: null`, which keeps them out of every EXISTING case's tier
  // pool: v4's `tierMatches` calls unknown-vs-a-known-class a non-match in
  // both directions, and every pre-existing profile here is Standard.
  authPrimaryProfile: ProfileSpec;
  keylessUnderstudyProfile: ProfileSpec;
  danglingKeyUnderstudyProfile: ProfileSpec;
  apiKeys: ApiKeySpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'primary-stream-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-primary-stream-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();

  const repo = new ChatsRepository();
  for (const c of spec.chats) {
    const participants = c.participants.map((p) => ({
      id: p.id,
      type: p.type,
      characterId: p.characterId,
      createdAt: spec.sentinel,
      updatedAt: spec.sentinel,
    }));
    await repo.create(
      {
        userId: c.userId,
        title: c.title,
        participants,
      } as never,
      { id: c.id, createdAt: spec.sentinel, updatedAt: spec.sentinel }
    );
  }
  // Force chat_messages into existence (empty) so the port can INSERT into it.
  await repo.getMessageCount(spec.chats[0].id);

  // P4.D135: the fallback chain reads REAL rows — `repos.connections.findById`
  // for the named understudy, `findByUserId` for the tier pick, and
  // `resolveConnectionProfileApiKey` for each candidate's key. Seeded here so
  // both sides walk the same company.
  const { ConnectionProfilesRepository } = await import(
    '@/lib/database/repositories/connection-profiles.repository'
  );
  const { ensureCollection, getCollection } = await import('@/lib/database/manager');
  const { ApiKeySchema } = await import('@/lib/schemas/types');
  const connections = new ConnectionProfilesRepository();
  await ensureCollection('api_keys', ApiKeySchema);
  // `createApiKey` mints its own id, and the ids have to be PINNED (the
  // profiles name them and the committed spec carries both sides' view), so the
  // rows go in through the collection — the `build-settings-fixture` idiom.
  const apiKeyCol = await getCollection('api_keys');
  for (const k of spec.apiKeys) {
    await apiKeyCol.insertOne(
      ApiKeySchema.parse({
        id: k.id,
        userId: spec.userId,
        label: k.label,
        provider: k.provider,
        key_value: k.keyValue,
        isActive: true,
        createdAt: spec.sentinel,
        updatedAt: spec.sentinel,
      }) as never,
    );
  }
  const seededProfiles = [
    spec.profile,
    spec.understudyProfile,
    spec.tierSpareProfile,
    spec.authPrimaryProfile,
    spec.keylessUnderstudyProfile,
    spec.danglingKeyUnderstudyProfile,
  ];
  for (const p of seededProfiles) {
    await connections.create(
      {
        userId: p.userId,
        name: p.name,
        provider: p.provider,
        transport: 'api',
        apiKeyId: p.apiKeyId ?? null,
        baseUrl: p.baseUrl,
        modelName: p.modelName,
        parameters: {},
        isDefault: false,
        isCheap: false,
        modelClass: p.modelClass ?? null,
        fallbackProfileId: p.fallbackProfileId ?? null,
        allowTierFallback: p.allowTierFallback ?? false,
        tags: [],
      } as never,
      { id: p.id, createdAt: spec.sentinel, updatedAt: spec.sentinel } as never,
    );
  }

  await closeDatabase();
  process.stderr.write(
    `built primary-stream seed fixture: ${out} (${spec.chats.length} chats, ` +
      `${seededProfiles.length} connection profiles, ${spec.apiKeys.length} api keys)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`primary-stream fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
