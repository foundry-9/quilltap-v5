/**
 * Tier-1 oracle case — `buildInformBlock` (P4.D205, v4 `e7d77bb60`).
 *
 * Drives v4's REAL `lib/chat/context/inform-block.ts` over a fixed corpus. The
 * module takes its `repos` as a parameter, so no database is involved: the case
 * injects a recording stub whose two read methods answer the corpus rows, and
 * whose WRITE methods record any call. That is v4's own test seam, used here
 * against v4's real module rather than transcribing v4's test file.
 *
 * Each row emits:
 *   - `content` / `rowIds` — the module's answer;
 *   - `calledMethod` — which of the two reads it chose (`findPendingForParticipant`
 *     or `findConsumedByMessages`), which pins the `isSwipe` predicate,
 *     including v4's `Array.isArray(ids) && ids.length > 0` (an EMPTY list falls
 *     back to pending);
 *   - `consumedArgs` — the message-id list handed to the swipe read;
 *   - `writeCalls` — every write method called. **It must be 0 on every row**:
 *     selection is not delivery, and `markConsumed` belongs to the finalizer.
 *
 * Run from a v4 checkout pinned at the target under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/inform-block.ts \
 *     > /tmp/oracle-inform-block.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

interface Row {
  id: string;
  chatId: string;
  batchId: string;
  participantId: string;
  contentMarkdown: string;
  recordMessageId: string | null;
  createdAt: string;
  updatedAt: string;
  consumedAt: string | null;
  consumedByMessageId: string | null;
}

interface Case {
  label: string;
  chatId: string;
  participantId: string;
  regenerationOfMessageIds?: string[];
  pending: Row[];
  consumed: Row[];
}

interface Spec {
  cases: Case[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'inform-block.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  process.env.LOG_LEVEL = 'error';

  const { buildInformBlock, INFORM_BLOCK_SEPARATOR } = await import(
    '@/lib/chat/context/inform-block'
  );

  const out: string[] = [];

  // The separator itself, so the Rust constant is pinned against v4's, not
  // against a transcription of it.
  out.push(
    JSON.stringify({
      case: 'inform-block-separator',
      separator: INFORM_BLOCK_SEPARATOR,
    })
  );

  for (const c of spec.cases) {
    const calls: string[] = [];
    let consumedArgs: unknown = null;

    const repos = {
      chatInforms: {
        findPendingForParticipant: async () => {
          calls.push('findPendingForParticipant');
          return c.pending;
        },
        findConsumedByMessages: async (
          _chatId: string,
          _participantId: string,
          messageIds: string[]
        ) => {
          calls.push('findConsumedByMessages');
          consumedArgs = messageIds;
          return c.consumed;
        },
        // Every write the repository exposes. A call to any of these is a
        // failure of the "never writes" rule, and the count is the comparand.
        createBatch: async () => {
          calls.push('WRITE:createBatch');
          return [];
        },
        markConsumed: async () => {
          calls.push('WRITE:markConsumed');
          return 0;
        },
        deletePendingByBatch: async () => {
          calls.push('WRITE:deletePendingByBatch');
          return 0;
        },
        deletePendingForParticipant: async () => {
          calls.push('WRITE:deletePendingForParticipant');
          return 0;
        },
        deleteByChatId: async () => {
          calls.push('WRITE:deleteByChatId');
          return 0;
        },
      },
    } as never;

    const result = await buildInformBlock({
      repos,
      chatId: c.chatId,
      participantId: c.participantId,
      regenerationOfMessageIds: c.regenerationOfMessageIds,
    });

    out.push(
      JSON.stringify({
        case: 'inform-block',
        label: c.label,
        content: result.content,
        rowIds: result.rowIds,
        calledMethod: calls.find((k) => !k.startsWith('WRITE:')) ?? null,
        consumedArgs,
        writeCalls: calls.filter((k) => k.startsWith('WRITE:')),
      })
    );
  }

  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inform-block oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
