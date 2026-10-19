/**
 * Fixture builder for the outfit-change side-effects differential (P4.D279
 * keystone C1 §7; v4 `lib/wardrobe/outfit-change-effects.ts`).
 *
 * Bakes the MAIN database both sides drive `notifyWardrobeChanged` & co. against
 * (a copy each), from `outfit-change-effects-tier3.json`:
 *   1. A users row (pinned id).
 *   2. A DEFAULT image profile carrying an `apiKeyId` — `resolveAvatarImageProfile`
 *      passes over any candidate without one, so this is what lets the avatar
 *      trigger actually enqueue (the P4.123 avatar-trigger builder seeds the same
 *      shape).
 *   3. Four chats seating both characters as CHARACTER participants: avatar
 *      auto-update ON, a second ON (so a bare refresh has a chat with no pending
 *      job to dedupe against), OFF, and an autonomous room with the flag ON (the
 *      trigger's by-design skip). All through the REAL `repos.chats.create`.
 *   4. `background_jobs` materialized (v4 creates it lazily; the Rust side INSERTs).
 *   P4.D316: a chat may name its own `participants` / `activeTypingParticipantId`
 *   / `impersonatingParticipantIds` (the operator-provenance chats); the
 *   `chat_wardrobe_history` table is NOT baked — each case plants it on its
 *   per-case copy (R-S).
 *
 * The module never reads `characters`, so no character row or vault is
 * provisioned and the mount index v4 opens beside main is scratch, discarded.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUTFIT_EFFECTS_MAIN=/tmp/qt-outfit-effects-main.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-outfit-change-effects-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ChatSpec {
  id: string;
  title: string;
  chatType: string;
  avatarGenerationEnabled: boolean;
  /** P4.D316: a chat's own seats (default: A and B, both LLM-controlled). */
  participants?: Array<Record<string, unknown>>;
  activeTypingParticipantId?: string;
  impersonatingParticipantIds?: string[];
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  user: Record<string, unknown>;
  characterAId: string;
  characterBId: string;
  imageProfile: Record<string, unknown> & { id: string };
  chats: Record<string, ChatSpec>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'outfit-change-effects-tier3.json'), 'utf8'),
  ) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_OUTFIT_EFFECTS_MAIN;
  if (!mainOut) {
    throw new Error('QT_FIXTURE_OUTFIT_EFFECTS_MAIN must point at the .db file to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = mainOut + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-outfit-effects-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  // Scratch, discarded with the directory: nothing here touches the mount index.
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount-index.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ImageProfileSchema } = await import('@/lib/schemas/profile.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('image_profiles', ImageProfileSchema);
  const repos = getRepositories();

  // 1. User.
  await repos.users.create(spec.user as never, {
    id: spec.userId,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // 2. The default image profile, WITH an api key id (the resolver's third check).
  const { id: ipId, ...ipRest } = spec.imageProfile;
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      parameters: {},
      isDefault: true,
      isDangerousCompatible: false,
      tags: [],
      ...ipRest,
    } as never,
    { id: ipId, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. The chats, each seating both characters.
  const participants = [
    {
      id: '44444444-4444-4444-8444-444444444441',
      type: 'CHARACTER',
      characterId: spec.characterAId,
      controlledBy: 'llm',
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    },
    {
      id: '44444444-4444-4444-8444-444444444442',
      type: 'CHARACTER',
      characterId: spec.characterBId,
      controlledBy: 'llm',
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    },
  ];
  for (const chat of Object.values(spec.chats)) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        participants: chat.participants ?? participants,
        chatType: chat.chatType,
        contextSummary: null,
        avatarGenerationEnabled: chat.avatarGenerationEnabled,
        ...(chat.activeTypingParticipantId
          ? { activeTypingParticipantId: chat.activeTypingParticipantId }
          : {}),
        ...(chat.impersonatingParticipantIds
          ? { impersonatingParticipantIds: chat.impersonatingParticipantIds }
          : {}),
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: TS },
    );
    await repos.chats.getMessageCount(chat.id);
  }

  // 4. v4 creates `background_jobs` lazily; force it so the Rust side can INSERT.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built outfit-change-effects fixture: main=${mainOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`outfit-change-effects fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
