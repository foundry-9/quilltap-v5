/**
 * Fixture builder for the W4.2 gatekeeper (CHAT_DANGER_CLASSIFICATION) tier-3
 * differential. Seeds `connection_profiles`, `chat_settings`, and `chats` (with
 * pinned ids/timestamps + a `token=<case>` marker in each `contextSummary` so the
 * canned model/moderation seams can key on the classification content).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-danger-gatekeeper.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-gatekeeper-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userA: string;
  userB: string;
  connectionProfiles: Array<Record<string, unknown> & { id: string; user: 'A' | 'B' }>;
  chatSettings: Array<Record<string, unknown> & { id: string; user: 'A' | 'B' }>;
  chats: Array<Record<string, unknown> & { id: string; user: 'A' | 'B' }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'danger-gatekeeper.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-gatekeeper-build-'));
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
    const { id, user, ...data } = cp;
    await repos.connections.create({ userId: uid(user), ...data } as never, {
      id,
      createdAt: ts,
      updatedAt: ts,
    });
  }

  for (const cs of spec.chatSettings) {
    const { id, user, ...data } = cs;
    await repos.chatSettings.create({ userId: uid(user), ...data } as never, {
      id,
      createdAt: ts,
      updatedAt: ts,
    });
  }

  let n = 0;
  for (const c of spec.chats) {
    const { id, user, ...data } = c;
    n += 1;
    const participantId = `fb0000${String(n).padStart(2, '0')}-0000-4000-8000-000000000001`;
    await repos.chats.create(
      {
        userId: uid(user),
        title: `Gatekeeper chat ${n}`,
        participants: [
          { id: participantId, type: 'CHARACTER', characterId: 'cccc3333-3333-4333-8333-333333333333', createdAt: ts, updatedAt: ts },
        ],
        ...data,
      } as never,
      { id, createdAt: ts, updatedAt: ts }
    );
  }

  // Materialize the chat_messages table (v4 ensureCollections it lazily on first
  // repo use; the Rust `add_message` expects it to exist). A read suffices.
  await repos.chats.getMessages(spec.chats[0].id);

  await closeDatabase();
  process.stderr.write(
    `built danger-gatekeeper fixture: ${out} (${spec.chats.length} chats, ${spec.chatSettings.length} settings)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`danger-gatekeeper fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
