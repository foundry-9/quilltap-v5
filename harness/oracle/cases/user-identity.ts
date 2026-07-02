/**
 * Oracle case — the user-identity resolver (v4
 * `lib/services/chat-message/user-identity-resolver.service.ts`
 * `resolveUserIdentity`), the four-step fallback chain.
 *
 * A pure READ path (no LLM, no write), so — like the turn-orchestrator /
 * memory-delete cases — this runs directly under the tsx real-DB path (no jest).
 * We copy the seed fixture, load each chat via `repos.chats.findById`, drive v4's
 * REAL `resolveUserIdentity`, and emit each resolved identity object. The Rust
 * port runs the same ops on the same fixture and the identities are compared
 * exactly (no normalization — the resolver mints nothing).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_USERIDENTITY=/tmp/qt-useridentity-main.db \
 *   QT_FIXTURE_USERIDENTITY_MOUNT=/tmp/qt-useridentity-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/user-identity.ts > /tmp/oracle-useridentity.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  chatId: string;
  userId: string;
  activeTypingParticipantId: string | null;
  note?: string;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'user-identity.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_USERIDENTITY;
  const fixtureMount = process.env.QT_FIXTURE_USERIDENTITY_MOUNT;
  if (!fixture || !existsSync(fixture) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_USERIDENTITY and QT_FIXTURE_USERIDENTITY_MOUNT must point at the seed fixtures',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-useridentity-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'useridentity-main.db');
  const workMount = join(scratch, 'useridentity-mount.db');
  copyFileSync(fixture, work);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { resolveUserIdentity } = await import(
    '@/lib/services/chat-message/user-identity-resolver.service'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const results: Array<Record<string, unknown>> = [];
  for (const op of spec.ops) {
    const chat = await repos.chats.findById(op.chatId);
    if (!chat) throw new Error(`chat not found: ${op.chatId}`);
    const identity = await resolveUserIdentity(
      repos,
      op.userId,
      chat,
      op.activeTypingParticipantId,
    );
    results.push({
      chatId: op.chatId,
      name: identity.name,
      description: identity.description,
      characterId: identity.characterId ?? null,
      source: identity.source,
    });
  }

  await closeDatabase();
  await closeMountIndexSQLiteClient();

  process.stdout.write(JSON.stringify({ case: 'user-identity', results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`user-identity oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
