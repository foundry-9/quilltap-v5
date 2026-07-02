/**
 * Oracle case — the participant resolver (v4
 * `lib/services/chat-message/participant-resolver.service.ts`
 * `resolveRespondingParticipant` / `loadAllParticipantData` / `getRoleplayTemplate`
 * + `lib/chat/connection-resolver.ts`).
 *
 * DB reads/one write (getRoleplayTemplate persists an inherited default), NO LLM
 * — so it runs directly under the tsx real-DB path (no jest). We copy the seed
 * fixture, load each chat, drive v4's REAL resolver functions, and emit a
 * projection of each op's result (or the thrown flag/message for the throw
 * cases). `Math.random` is pinned to each op's `random01` (the sole impurity in
 * `selectNextSpeaker`); the Rust side threads the identical value.
 *
 * After the ops we dump the `chats` table so the ONE write path (getRoleplayTemplate
 * persisting the inherited `roleplayTemplateId`) is diffed. v4's `chats.update`
 * preserves `updatedAt` (re-passes existing) when the caller omits it, so the
 * diff is zero-normalization.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PARTRES=/tmp/qt-partres-main.db \
 *   QT_FIXTURE_PARTRES_MOUNT=/tmp/qt-partres-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/participant-resolver.ts > /tmp/oracle-partres.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'resolveResponding' | 'loadAllParticipantData' | 'getRoleplayTemplate';
  chatId: string;
  requestedRespondingParticipantId?: string | null;
  isContinueMode?: boolean;
  activeUserParticipantId?: string | null;
  random01?: number;
  primaryCharacterId?: string;
  settingsDefaultRoleplayTemplateId?: string | null;
  note?: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'participant-resolver.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_PARTRES;
  const fixtureMount = process.env.QT_FIXTURE_PARTRES_MOUNT;
  if (!fixture || !existsSync(fixture) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_PARTRES and QT_FIXTURE_PARTRES_MOUNT must point at the seed fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-partres-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'partres-main.db');
  const workMount = join(scratch, 'partres-mount.db');
  copyFileSync(fixture, work);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { resolveRespondingParticipant, loadAllParticipantData, getRoleplayTemplate } =
    await import('@/lib/services/chat-message/participant-resolver.service');

  await initializeDatabase();
  const repos = getRepositories();

  const results: Array<Record<string, unknown>> = [];
  const origRandom = Math.random;

  for (const op of spec.ops) {
    const chat = await repos.chats.findById(op.chatId);
    if (!chat) throw new Error(`chat not found: ${op.chatId}`);

    if (op.kind === 'resolveResponding') {
      Math.random = () => op.random01 as number;
      try {
        const r = await resolveRespondingParticipant(
          repos,
          chat,
          spec.userId,
          op.requestedRespondingParticipantId ?? undefined,
          op.isContinueMode ?? false,
          op.activeUserParticipantId ?? null,
        );
        results.push({
          op: 'resolveResponding',
          chatId: op.chatId,
          threw: false,
          characterParticipantId: r.characterParticipant.id,
          characterId: r.character.id,
          characterName: r.character.name,
          connectionProfileId: r.connectionProfile.id,
          imageProfileId: r.imageProfileId ?? null,
          userParticipantId: r.userParticipantId ?? null,
          isMultiCharacter: r.isMultiCharacter,
        });
      } catch (err) {
        results.push({
          op: 'resolveResponding',
          chatId: op.chatId,
          threw: true,
          message: (err as Error).message,
        });
      } finally {
        Math.random = origRandom;
      }
    } else if (op.kind === 'loadAllParticipantData') {
      const primary = await repos.characters.findById(op.primaryCharacterId as string);
      if (!primary) throw new Error(`primary character not found: ${op.primaryCharacterId}`);
      const data = await loadAllParticipantData(repos, chat, primary);
      // Emit a sorted list of the character ids in the map (deterministic).
      const ids = Array.from(data.participantCharacters.keys()).sort();
      results.push({
        op: 'loadAllParticipantData',
        chatId: op.chatId,
        characterIds: ids,
      });
    } else {
      // getRoleplayTemplate
      const chatSettings = op.settingsDefaultRoleplayTemplateId
        ? { defaultRoleplayTemplateId: op.settingsDefaultRoleplayTemplateId }
        : null;
      const rt = await getRoleplayTemplate(repos, chat, chatSettings);
      results.push({
        op: 'getRoleplayTemplate',
        chatId: op.chatId,
        systemPrompt: rt ? rt.systemPrompt : null,
      });
    }
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(chats)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chats')) as Array<Record<string, unknown>>;

  await closeDatabase();
  await closeMountIndexSQLiteClient();

  const dump = canonicalizeRows({ table: 'chats', columns, rawRows, orderBy: 'id' });
  process.stdout.write(JSON.stringify({ case: 'participant-resolver', results, dump }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`participant-resolver oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
