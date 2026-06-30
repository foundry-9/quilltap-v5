/**
 * Tier-2 oracle case — the chats impersonation ops (Phase-2, the chats repo —
 * sub-unit 6: addImpersonation / removeImpersonation /
 * getImpersonatedParticipantIds / setActiveTypingParticipant /
 * updateAllLLMPauseTurnCount).
 *
 * Runs the fixed op sequence (`chats-impersonation-tier2.json`) via v4's REAL
 * `ChatsRepository` on a copy of the seed fixture, then dumps the `chats` rows
 * canonically. All five ops are exposed directly on the repository surface.
 *
 * Each op only rewrites `impersonatingParticipantIds` /
 * `activeTypingParticipantId` / `allLLMPauseTurnCount`; the CHAT's own
 * `updatedAt` is never bumped (v4 `_update` preserves it) and NO ids/timestamps
 * are minted — so the run is ZERO normalization (the participant ids + all
 * timestamps are pinned in the seed; the `chats` dump is diffed exactly).
 *
 * `getImpersonatedParticipantIds` reads no state, so it is exercised by issuing
 * the call (proving it does not throw / mutate) and emitted alongside the dump.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHIMP=/tmp/qt-chimp-fixture.db \
 *     npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-impersonation-tier2.ts > /tmp/oracle-chimp.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind:
    | 'addImpersonation'
    | 'removeImpersonation'
    | 'getImpersonatedParticipantIds'
    | 'setActiveTypingParticipant'
    | 'updateAllLLMPauseTurnCount';
  chatId: string;
  participantId?: string | null;
  count?: number;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function dumpTable(rawQuery: (sql: string) => Promise<unknown>, table: string) {
  const columns = (
    (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
  return canonicalizeRows({ table, columns, rawRows, orderBy: 'id' });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chats-impersonation-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHIMP;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHIMP must point at the seed fixture from build-chats-impersonation-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chimp-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chimp-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();
  const repo = new ChatsRepository();

  for (const op of spec.ops) {
    if (op.kind === 'addImpersonation') {
      await repo.addImpersonation(op.chatId, op.participantId as string);
    } else if (op.kind === 'removeImpersonation') {
      await repo.removeImpersonation(op.chatId, op.participantId as string);
    } else if (op.kind === 'getImpersonatedParticipantIds') {
      await repo.getImpersonatedParticipantIds(op.chatId);
    } else if (op.kind === 'setActiveTypingParticipant') {
      await repo.setActiveTypingParticipant(op.chatId, (op.participantId ?? null) as string | null);
    } else {
      await repo.updateAllLLMPauseTurnCount(op.chatId, op.count as number);
    }
  }

  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-impersonation-tier2', chats }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-impersonation-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
