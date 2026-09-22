/**
 * Oracle case (tier 1): v4's `isRecordOnlyMessage` (P4.106 item 4).
 *
 * Drives the REAL export from `lib/services/chat-message/context-builder.
 * service.ts` (`e7d77bb60` — the Inform commit widened it from "Commonplace
 * Book whispers" to also strip the Host's `inform` record). The predicate is
 * the ONE gate `buildMessageContext` applies before anything else touches the
 * history; v5's twin is `quilltap_core::services::message_context::
 * is_record_only_message`.
 *
 * Reads the corpus `harness/oracle/fixtures/is-record-only-message.json` and
 * emits one NDJSON row per case: `{ id, input, output }`.
 *
 * Run from inside the (pinned) server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/is-record-only-message.ts \
 *     > /tmp/oracle-is-record-only-message.ndjson
 */

import * as fs from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { isRecordOnlyMessage } from '@/lib/services/chat-message/context-builder.service';

interface Case {
  id: string;
  input: { systemSender?: string | null; systemKind?: string | null };
}

const here = dirname(fileURLToPath(import.meta.url));
const corpusPath = join(here, '..', 'fixtures', 'is-record-only-message.json');
const corpus = JSON.parse(fs.readFileSync(corpusPath, 'utf8')) as { cases: Case[] };

for (const c of corpus.cases) {
  const output = isRecordOnlyMessage(c.input);
  process.stdout.write(JSON.stringify({ id: c.id, input: c.input, output }) + '\n');
}
