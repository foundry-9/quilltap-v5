/**
 * Tier-3 SEED fixture builder — the context-summary service half (v4
 * `generateContextSummary` / `invalidateContextSummaryIfMessageCovered` /
 * `checkAndGenerateSummaryIfNeeded`, `lib/chat/context-summary.ts`), ported as
 * `quilltap-core::services::context_summary`.
 *
 * Builds a SINGLE main database (the service never touches the mount-index /
 * vault): each chat is created via v4's REAL `repos.chats.create` (ids +
 * participants + createdAt/updatedAt pinned), its conversation messages added
 * via `repos.chats.addMessages` (message ids + createdAt pinned) — which mints
 * chat-metadata (`lastMessageAt`/`updatedAt`/`messageCount`/
 * `spokenThisCycleParticipantIds`) as a side effect — then the summary-cadence
 * columns and metadata are RESET back to the pinned seed via `repos.chats.update`
 * so the starting state is fully deterministic. Pre-existing Librarian summary
 * whispers (for the sweep case) are added as separate messages carrying
 * `systemSender: 'librarian'` + `summaryAnchor` / the legacy content prefix.
 *
 * Both the oracle and the Rust port then COPY this file and run the same op
 * sequence — which mints new ids/timestamps — so the seed rows are byte-identical
 * and only the service's own effect differs.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ctxsum-main.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-context-summary-service-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface MessageSeed {
  id: string;
  role?: 'USER' | 'ASSISTANT';
  content: string;
  systemSender?: string;
  summaryAnchor?: { compactionGeneration: number };
}
interface ChatSeed {
  id: string;
  title: string;
  chatType?: string;
  isDangerousChat?: boolean;
  conciergeOverride?: 'OFF';
  contextSummary?: string | null;
  compactionGeneration?: number;
  lastSummaryTurn?: number;
  lastFullRebuildTurn?: number;
  lastRenameCheckInterchange?: number;
  summaryAnchorMessageIds?: string[];
  participants: Array<Record<string, unknown>>;
  messages: MessageSeed[];
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  chats: ChatSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'context-summary-service-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) throw new Error('QT_FIXTURE_OUT must point at the fixture .db file');
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ctxsum-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { BackgroundJobSchema } = await import('@/lib/schemas/types');

  await initializeDatabase();
  // Materialize the background_jobs table so both sides open a fixture that
  // already has it (the title-checkpoint enqueue writes here; the Rust port
  // does not create tables on the fly).
  await ensureCollection('background_jobs', BackgroundJobSchema);
  const repo = new ChatsRepository();

  for (const c of spec.chats) {
    await repo.create(
      {
        userId: spec.userId,
        title: c.title,
        participants: c.participants,
        chatType: c.chatType ?? 'salon',
        isDangerousChat: c.isDangerousChat ?? null,
        conciergeOverride: c.conciergeOverride ?? null,
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );

    if (c.messages.length > 0) {
      await repo.addMessages(
        c.id,
        c.messages.map((m) => {
          const base: Record<string, unknown> = {
            type: 'message',
            id: m.id,
            role: m.role ?? 'ASSISTANT',
            content: m.content,
            createdAt: spec.seedTimestamp,
          };
          if (m.systemSender !== undefined) base.systemSender = m.systemSender;
          if (m.summaryAnchor !== undefined) base.summaryAnchor = m.summaryAnchor;
          return base;
        }) as never
      );
    }

    // Reset the summary-cadence columns + metadata to the pinned seed so the
    // starting state is fully deterministic.
    await repo.update(c.id, {
      contextSummary: c.contextSummary ?? null,
      compactionGeneration: c.compactionGeneration ?? 0,
      lastSummaryTurn: c.lastSummaryTurn ?? 0,
      lastFullRebuildTurn: c.lastFullRebuildTurn ?? 0,
      lastRenameCheckInterchange: c.lastRenameCheckInterchange ?? 0,
      summaryAnchorMessageIds: c.summaryAnchorMessageIds ?? [],
      lastMessageAt: null,
      updatedAt: spec.seedTimestamp,
    } as never);
  }

  await closeDatabase();
  process.stderr.write(
    `built context-summary-service fixture: main=${out} (${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`context-summary-service fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
