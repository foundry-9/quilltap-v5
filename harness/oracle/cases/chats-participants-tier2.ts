/**
 * Tier-2 oracle case — the chats participant ops (Phase-2, the chats repo —
 * sub-unit 5: addParticipant / updateParticipant / removeParticipant /
 * setParticipantStatus).
 *
 * Runs the fixed op sequence (`chats-participants-tier2.json`) via v4's REAL
 * `ChatsRepository` on a copy of the seed fixture, then dumps the `chats` rows
 * canonically. Each op is a read-modify-write of the `participants` JSON column;
 * the CHAT's own `updatedAt` is never bumped (v4 `_update` preserves it).
 *
 * `setParticipantStatus` is driven through the NAMED repository method v4 added
 * in `d553f72a` (it used to exist only on the ops), and its `{chat, oldStatus}`
 * return — the half no table dump can show — is emitted per op as `returns`.
 * `removeParticipant` of the last present participant throws
 * (v4 `safeQuery` rethrow); ops flagged `expectThrow` are caught so the run
 * continues and that chat stays unmutated.
 *
 * NORMALIZATION SPEC (applied identically to both dumps in the Rust test):
 *   - participant `id`s (pinned seed AND minted) → first-appearance tokens
 *     `p0`, `p1`, … in dump order; the same map rewrites
 *     `impersonatingParticipantIds` + `activeTypingParticipantId`.
 *   - participant `createdAt`/`updatedAt`/`removedAt`: a value EQUAL to the seed
 *     sentinel stays pinned (diffed exactly — proves createdAt preservation and
 *     no-stray-mint); any OTHER value (a genuine mint) → `<ts>`; explicit `null`
 *     (a cleared `removedAt`) stays `null`.
 *   - chat-level `createdAt`/`updatedAt` are NOT normalized (seed sentinel,
 *     never minted here — diffed exactly).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHATSPARTS=/tmp/qt-chatsparts-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-participants-tier2.ts > /tmp/oracle-chatsparts.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'addParticipant' | 'updateParticipant' | 'removeParticipant' | 'setParticipantStatus';
  chatId: string;
  participantId?: string;
  participant?: Record<string, unknown>;
  data?: Record<string, unknown>;
  status?: string;
  expectThrow?: boolean;
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
  const specPath = join(here, '..', 'fixtures', 'chats-participants-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHATSPARTS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHATSPARTS must point at the seed fixture from build-chats-participants-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chatsparts-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chatsparts-work.db');
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

  // P4.D65 (the banked P4.D63 unit-5 arm): `setParticipantStatus` is now a
  // NAMED repository method (`d553f72a`), not just an ops internal, and its
  // return shape `{chat, oldStatus}` is the half no dump can show. The archive
  // service's participant flips are its production caller, so the shape is
  // recorded per op alongside the state.
  const returns: Array<Record<string, unknown>> = [];

  for (const [index, op] of spec.ops.entries()) {
    try {
      if (op.kind === 'addParticipant') {
        await repo.addParticipant(op.chatId, op.participant as never);
      } else if (op.kind === 'updateParticipant') {
        await repo.updateParticipant(op.chatId, op.participantId as string, op.data as never);
      } else if (op.kind === 'removeParticipant') {
        await repo.removeParticipant(op.chatId, op.participantId as string);
      } else {
        const outcome = await repo.setParticipantStatus(
          op.chatId,
          op.participantId as string,
          op.status as never,
        );
        // The returned chat is RELOADED after the write, so the participant's
        // status inside it is already the new one — recording it is what tells
        // a reload apart from an echo of the pre-write row. Nothing else about
        // the chat is emitted: the `chats` dump already diffs it whole.
        const chat = outcome.chat as { id: string; participants?: Array<{ id: string; status?: string }> } | null;
        returns.push({
          op: index,
          oldStatus: outcome.oldStatus,
          chatId: chat?.id ?? null,
          statusInReturnedChat:
            chat?.participants?.find((p) => p.id === op.participantId)?.status ?? null,
        });
      }
      if (op.expectThrow) {
        throw new Error(`op ${op.kind} on ${op.chatId} was expected to throw but did not`);
      }
    } catch (err) {
      if (!op.expectThrow) throw err;
    }
  }

  const chats = await dumpTable(rawQuery, 'chats');

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'chats-participants-tier2', chats, returns }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats-participants-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
