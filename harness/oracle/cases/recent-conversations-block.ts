/**
 * P4.D208 oracle: the greeting's "Recent Conversations" block (v4 bug 158,
 * `da9c4f34f`). Drives v4's REAL `buildRecentConversationsBlock`
 * (`lib/memory/memory-recap.ts`) over a COPY of the pre-baked fixture — through
 * the REAL `ChatsRepository`, not a mock — and emits each rendered block
 * verbatim.
 *
 * Driving the real repository is the point. v4's own new test file mocks
 * `findRecentSummarizedByCharacter`, so it can pin the render but not the read;
 * here the read's `contextSummary IS NOT NULL` filter is under test too, which
 * is what makes the empty-gist arm reachable at all — a NULL summary never
 * arrives, a whitespace-only one does, and only the second renders a heading
 * alone. Before bug 158, that arm was unreachable in practice for a different
 * reason: every chat carried its scenario in the column.
 *
 * What each call proves, beyond the block's bytes:
 *   - the 445-char scenario is capped at 280 with an ellipsis (the bug's shape);
 *   - 280 exactly is NOT capped, 281 is (`trimmed.length <= maxChars`);
 *   - a padded summary is trimmed before measuring;
 *   - the block closes with READ_CONVERSATION_CALL_NOTE;
 *   - limit <= 0 returns '' AND asks the repository nothing (`repoCalls`).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_RECENT_CONVS=/tmp/qt-recent-convs.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/recent-conversations-block.ts \
 *     > /tmp/oracle-recent-conversations-block.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  calls: Array<[string, string, string | null, number]>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'recent-conversations-block.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_RECENT_CONVS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_RECENT_CONVS must point at the fixture from build-recent-conversations-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-recent-convs-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'recent-convs-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { buildRecentConversationsBlock } = await import('@/lib/memory/memory-recap');

  await initializeDatabase();

  // Count the reads WITHOUT standing in for them: the real method still runs,
  // so the repository is genuinely exercised. This is the only instrument that
  // can see v4's `limit <= 0` short circuit, which is otherwise
  // indistinguishable from "the query returned nothing".
  //
  // The patch is on the PROTOTYPE, not on the module namespace:
  // `memory-recap.ts` imports `getRepositories` as a named binding, so
  // replacing the export would not be seen by the importer. Whatever instance
  // the real factory hands back is a `ChatsRepository`, so this is.
  let repoCalls = 0;
  const realFind = ChatsRepository.prototype.findRecentSummarizedByCharacter;
  ChatsRepository.prototype.findRecentSummarizedByCharacter = async function (
    this: unknown,
    ...args: unknown[]
  ) {
    repoCalls++;
    return (realFind as (...a: unknown[]) => unknown).apply(this, args);
  } as typeof realFind;

  let totalRepoCalls = 0;
  for (const [label, characterId, currentChatId, limit] of spec.calls) {
    const before = repoCalls;
    const block = await buildRecentConversationsBlock(
      characterId,
      currentChatId ?? undefined,
      limit
    );
    process.stdout.write(
      JSON.stringify({
        label,
        characterId,
        currentChatId,
        limit,
        block,
        repoCalls: repoCalls - before,
      }) + '\n'
    );
    totalRepoCalls += repoCalls - before;
  }

  if (totalRepoCalls === 0) {
    throw new Error(
      'the prototype patch was never seen — every `repoCalls: 0` would be a lie (see the header)'
    );
  }

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`recent-conversations-block oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
