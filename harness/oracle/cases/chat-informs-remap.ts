/**
 * Tier-1 oracle case — `remapChatInform` (P4.D205, v4 `e7d77bb60`).
 *
 * Drives v4's REAL `lib/import/quilltap-import/reconcile.ts` over a fixed
 * corpus. The function is pure over its three inputs (the row, the id maps, and
 * the destination's known participant/message sets), so no database is involved.
 *
 * The three rules under test:
 *   - a `participantId` that is not a seat of the destination chat → DROPPED;
 *   - a `recordMessageId` the destination does not have → DROPPED;
 *   - a `consumedByMessageId` the destination does not have → **KEPT**, with the
 *     field nulled and `consumedByMessageIdCleared` true. The row is still a
 *     real historical inform; it simply loses its swipe anchor.
 *
 * Each row emits v4's whole result — `ok`, the `reason` sentence with its ids
 * interpolated, the remapped `data`, and the cleared flag.
 *
 * Run from a v4 checkout pinned at the target under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-informs-remap.ts \
 *     > /tmp/oracle-chat-informs-remap.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';

interface Case {
  label: string;
  inform: Record<string, unknown>;
  /** Source chat id → destination chat id. */
  chatMap: Record<string, string>;
  knownParticipantIds: string[];
  knownMessageIds: string[];
}

interface Spec {
  cases: Case[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-informs-remap.json'), 'utf8')
  ) as Spec;

  process.env.LOG_LEVEL = 'error';

  const { remapChatInform } = await import('@/lib/import/quilltap-import/reconcile');

  const out: string[] = [];
  for (const c of spec.cases) {
    const idMaps = { chats: new Map(Object.entries(c.chatMap)) } as never;
    const known = {
      participantIds: new Set(c.knownParticipantIds),
      messageIds: new Set(c.knownMessageIds),
    };
    const result = remapChatInform(c.inform as never, idMaps, known);
    out.push(JSON.stringify({ case: 'chat-informs-remap', label: c.label, result }));
  }

  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-informs-remap oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
