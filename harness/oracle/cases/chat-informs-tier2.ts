/**
 * Tier-2 oracle case — the `chat_informs` repository (P4.D205, v4 `e7d77bb60`).
 *
 * Drives v4's REAL `ChatInformsRepository` through a fixed sequence over all ten
 * methods and emits TWO comparands, because final state alone cannot see this
 * repo's whole contract:
 *
 *   - **`reads`** — the RESULT of every read op, in order. Three of the ten
 *     methods sort (`createdAt` ascending, tie-broken by `id.localeCompare`) and
 *     one folds rows into batches; none of that is visible in a table dump, so
 *     the results are the comparand that pins it. `deletePendingByBatch` /
 *     `markConsumed` return counts, which land here too.
 *   - **`dump`** — the final table state, canonically shaped.
 *
 * MINTED VALUES: `createBatch` mints a `batchId`, one `id` per target, and the
 * timestamps; `markConsumed` mints `consumedAt`/`updatedAt`. Nothing is pinned on
 * those ops, so this case emits everything RAW and the harness applies one
 * normalization to BOTH sides (see
 * crates/quilltap-harness/tests/chat_informs_tier2_equivalence.rs): a value that
 * appears in the committed spec is compared EXACTLY; anything else was minted at
 * run time and becomes a first-seen token (`ID_n`) or `<ts>`. That keeps the
 * seed's pinned timestamps — the whole ordering proof — under byte comparison.
 *
 * Run from a v4 checkout pinned at the target under Node 24, AFTER building the
 * fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd /tmp/qt-v4-pin-p4d205-f45a517a9
 *   QT_FIXTURE_CHAT_INFORMS=/tmp/p4d205/qt-chat-informs-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-informs-tier2.ts \
 *     > /tmp/p4d205/oracle-chat-informs.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: string;
  label: string;
  chatId?: string;
  participantId?: string;
  batchId?: string;
  messageIds?: string[];
  contentMarkdown?: string;
  participantIds?: string[];
  recordMessageId?: string | null;
  ids?: string[];
  messageId?: string;
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'chat-informs-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_CHAT_INFORMS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_CHAT_INFORMS must point at the seed fixture from build-chat-informs-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-chat-informs-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'chat-informs-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { ChatInformsRepository } = await import(
    '@/lib/database/repositories/chat-informs.repository'
  );

  await initializeDatabase();
  const repo = new ChatInformsRepository();

  const reads: Array<{ kind: string; label: string; result: unknown }> = [];

  for (const op of spec.ops) {
    let result: unknown;
    switch (op.kind) {
      case 'findPendingForParticipant':
        result = await repo.findPendingForParticipant(op.chatId!, op.participantId!);
        break;
      case 'findConsumedByMessages':
        result = await repo.findConsumedByMessages(
          op.chatId!,
          op.participantId!,
          op.messageIds!
        );
        break;
      case 'findPendingBatches':
        result = await repo.findPendingBatches(op.chatId!);
        break;
      case 'findByChatId':
        result = await repo.findByChatId(op.chatId!);
        break;
      case 'findByBatchId':
        result = await repo.findByBatchId(op.batchId!);
        break;
      case 'createBatch':
        result = await repo.createBatch({
          chatId: op.chatId!,
          contentMarkdown: op.contentMarkdown!,
          participantIds: op.participantIds!,
          recordMessageId: op.recordMessageId ?? null,
        });
        break;
      case 'markConsumed':
        result = await repo.markConsumed(op.ids!, op.messageId!);
        break;
      case 'deletePendingByBatch':
        result = await repo.deletePendingByBatch(op.batchId!);
        break;
      case 'deletePendingForParticipant':
        result = await repo.deletePendingForParticipant(op.chatId!, op.participantId!);
        break;
      case 'deleteByChatId':
        result = await repo.deleteByChatId(op.chatId!);
        break;
      default:
        throw new Error(`unknown op kind: ${op.kind}`);
    }
    reads.push({ kind: op.kind, label: op.label, result });
  }

  // Final state, RAW through v4's own connected backend.
  const columns = (
    (await rawQuery('PRAGMA table_info(chat_informs)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM chat_informs')) as Array<
    Record<string, unknown>
  >;

  await closeDatabase();

  // Sorted by the minted `id` here only as a stable starting order; the harness
  // re-sorts BOTH dumps by the deterministic natural key (contentMarkdown,
  // participantId) before diffing, because `id` is minted on the created rows.
  const dump = canonicalizeRows({
    table: 'chat_informs',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'chat-informs-tier2', reads, dump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-informs-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
