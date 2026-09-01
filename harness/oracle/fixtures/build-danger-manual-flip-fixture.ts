/**
 * Fixture builder for the W4.2 manual Concierge-flip differential
 * (v4 `applyConciergeFlip`). Bakes a set of `chats` rows in the four states
 * (monitored / flagged / vouched / uncensored), one per flip scenario — all 12
 * ordered transitions plus the four no-ops — with pinned ids + timestamps so
 * both sides read the identical seed.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-danger-manual-flip.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-manual-flip-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  characterId: string;
  chats: Array<Record<string, unknown> & { id: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'danger-manual-flip.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-danger-manual-flip-build-'));
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

  const ts = spec.seedTimestamp;
  let n = 0;
  for (const c of spec.chats) {
    const { id, ...danger } = c;
    n += 1;
    // Zero-pad: the corpus is 16 chats, so a bare `${n}` would overflow the
    // 8-hex-digit UUID head at n=10.
    const participantId = `fb0000${String(n).padStart(2, '0')}-0000-4000-8000-000000000001`;
    await repo.create(
      {
        userId: spec.userId,
        participants: [
          {
            id: participantId,
            type: 'CHARACTER',
            characterId: spec.characterId,
            createdAt: ts,
            updatedAt: ts,
          },
        ],
        ...danger,
      } as never,
      { id, createdAt: ts, updatedAt: ts }
    );
  }

  // Materialize the chat_messages table (v4 ensureCollections it lazily on first
  // repo use; the Rust `add_message` — which the RealConciergeAnnouncer drives —
  // expects it to exist in the copied fixture). A read suffices.
  await repo.getMessages(spec.chats[0].id);

  await closeDatabase();
  process.stderr.write(`built danger-manual-flip fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`danger-manual-flip fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
