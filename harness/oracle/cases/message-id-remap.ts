/**
 * P4.D294 — tier-1 oracle for v4's bug-228 remint (`8050958be`, widened by
 * `c2795733f` / `ac6348c5f` / `64743ee13` to the three extraction-cursor keys):
 * `remintChatMessageIds(chat, messages, mintId)` and
 * `remapImportedMessageId(idMaps, sourceChatId, messageId)`, driven through
 * v4's REAL `lib/import/quilltap-import/message-id-remap.ts` over the corpus
 * `harness/oracle/fixtures/message-id-remap.json`.
 *
 * Every remint row mints with a per-row counter (`m-1`, `m-2`, …), so the Rust
 * side (`message_id_remap_equivalence`) replays exactly what v4 saw with no
 * normalization. Emitted per remint row: the reminted `messages`, the
 * `chatPatch` as its `JSON.stringify` BYTES (key order is part of the patch —
 * it is spread over the bundle chat before `create`), the `idMap` entries in
 * insertion order, `droppedRefs`, `summaryCleared`. Per remap row: the
 * answer (`null` = reminted but never landed).
 *
 * Run (Node 24, from the v4 checkout — or the round's pin — so `@/` resolves):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx "$V5W/harness/oracle/cases/message-id-remap.ts" \
 *     > /tmp/oracle-message-id-remap.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

interface RemintRow {
  label: string;
  chat: Record<string, unknown>;
  messages: Array<Record<string, unknown>>;
}

interface RemapRow {
  label: string;
  maps: Record<string, Record<string, string>>;
  sourceChatId: string | null;
  messageId: string;
}

interface Spec {
  remint: RemintRow[];
  remap: RemapRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'message-id-remap.json'), 'utf8'),
  ) as Spec;

  process.env.LOG_LEVEL = 'error';
  const { remintChatMessageIds, remapImportedMessageId } = await import(
    '@/lib/import/quilltap-import/message-id-remap'
  );

  const out: string[] = [];
  for (const row of spec.remint) {
    let n = 0;
    const r = remintChatMessageIds(row.chat as never, row.messages as never, () => `m-${++n}`);
    out.push(
      JSON.stringify({
        kind: 'remint',
        label: row.label,
        messages: r.messages,
        chatPatch: JSON.stringify(r.chatPatch),
        idMap: Array.from(r.idMap.entries()),
        droppedRefs: r.droppedRefs,
        summaryCleared: r.summaryCleared,
      }),
    );
  }
  for (const row of spec.remap) {
    const messages = new Map(
      Object.entries(row.maps).map(([chat, m]) => [chat, new Map(Object.entries(m))]),
    );
    const got = remapImportedMessageId({ messages } as never, row.sourceChatId, row.messageId);
    out.push(JSON.stringify({ kind: 'remap', label: row.label, got }));
  }

  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`message-id-remap oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
