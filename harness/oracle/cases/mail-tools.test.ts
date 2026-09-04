/**
 * @jest-environment node
 *
 * Differential ORACLE for the W4.1d5 `send_mail` / `list_email` tools. Drives v4's
 * REAL handlers over the shared two-DB fixture, ONE fresh copy per scenario
 * (send_mail WRITES), with the delivery clock (`new Date().toISOString()`) PINNED
 * via fake timers so the minted `sentAt` — and thus the letter path + formatted
 * date — is deterministic. Emits one NDJSON line per scenario:
 *   { label, send?: {json, fmt}, content?, list?: {json, fmt} }
 *
 * TZ MUST be UTC (the mailbox date uses the system timezone by v4's design; the
 * Rust port computes it in UTC — run this oracle with `TZ=UTC`).
 *
 * Real-DB-under-jest (search-tools recipe). NO model boundary is touched.
 *
 * Run (Node 24, from the v4 checkout; STAGE outside any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5 ; STAGE=/tmp/qt-mail-carina-tools-stage
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-mail-carina-tools-fixture.ts
 *   TZ=UTC QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-mail-tools.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- mail-tools
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  fixedSentAt: string;
  senderId: string;
  recipientId: string;
  emptyId: string;
  archivedId: string;
  archivedName: string;
  seedLetterInSenderMailbox: { path: string };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'mail-carina-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must point at the seeded fixtures');
  }
  const meta = JSON.parse(readFileSync(mainFixture + '.meta.json', 'utf8')) as {
    recipientVault: string;
  };
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  type SendArgs = { character?: string; message?: string; in_reply_to?: string };
  interface Scenario {
    label: string;
    send?: { args: unknown; characterId: string | undefined };
    readContent?: boolean; // read the delivered letter back
    listAfterSend?: string; // characterId to list after send
    listOnly?: string; // characterId to list (no send)
  }
  const scenarios: Scenario[] = [
    {
      label: 'send_and_list',
      send: { args: { character: 'Aurora', message: 'Hello Aurora, the stars are bright tonight.' } as SendArgs, characterId: spec.senderId },
      readContent: true,
      listAfterSend: spec.recipientId,
    },
    {
      label: 'reply',
      send: { args: { character: 'Aurora', message: 'I will be there.', in_reply_to: spec.seedLetterInSenderMailbox.path } as SendArgs, characterId: spec.senderId },
      readContent: true,
    },
    {
      label: 'reply_not_found',
      send: { args: { character: 'Aurora', message: 'x', in_reply_to: 'Mail/9999999999999-from-nobody.md' } as SendArgs, characterId: spec.senderId },
    },
    {
      label: 'missing_message',
      send: { args: { character: 'Aurora' } as SendArgs, characterId: spec.senderId },
    },
    {
      label: 'recipient_not_found',
      send: { args: { character: 'Nobody', message: 'x' } as SendArgs, characterId: spec.senderId },
    },
    {
      label: 'no_character',
      send: { args: { character: 'Aurora', message: 'x' } as SendArgs, characterId: undefined },
    },
    { label: 'list_empty', listOnly: spec.emptyId },
    { label: 'list_single', listOnly: spec.recipientId },
    // ── P4.D65: the three archived-character refusals (the banked P4.D63
    // unit-4 arms). All three fire on the tombstone FLAG, over a character
    // whose vault is perfectly intact.
    {
      label: 'send_from_archived_sender',
      send: { args: { character: 'Aurora', message: 'One last letter.' } as SendArgs, characterId: spec.archivedId },
    },
    {
      // BY ID. `resolveCharacterByNameOrId` deliberately keeps resolving an
      // archived character by exact id while skipping it on a NAME match, so
      // this is the only way to reach the RECIPIENT refusal at all.
      label: 'send_to_archived_recipient_by_id',
      send: { args: { character: spec.archivedId, message: 'Are you still there?' } as SendArgs, characterId: spec.senderId },
    },
    {
      // BY NAME — the other side of that same resolver rule: the archived
      // character is simply not found, and the refusal is the ordinary
      // no-such-soul one. Without this arm the id case above could pass on a
      // resolver that had stopped skipping archived name matches.
      label: 'send_to_archived_recipient_by_name',
      send: { args: { character: spec.archivedName, message: 'Are you still there?' } as SendArgs, characterId: spec.senderId },
    },
    { label: 'list_archived', listOnly: spec.archivedId },
  ];

  const lines: string[] = [];

  for (const sc of scenarios) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-mail-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    await initializeDatabase();

    // Pin the delivery clock (Date only; leave all timer fns real so async works).
    jest.useFakeTimers({
      doNotFake: [
        'setTimeout', 'setInterval', 'setImmediate', 'clearTimeout', 'clearInterval',
        'clearImmediate', 'nextTick', 'queueMicrotask', 'requestAnimationFrame',
        'cancelAnimationFrame', 'hrtime', 'performance',
      ],
    });
    jest.setSystemTime(new Date(spec.fixedSentAt));

    const record: Record<string, unknown> = { label: sc.label };

    if (sc.send) {
      const { executeSendMailTool, formatSendMailResults } = await import(
        '@/lib/tools/handlers/send-mail-handler'
      );
      const out = await executeSendMailTool(sc.send.args, {
        userId: spec.userId,
        chatId: 'chat-x',
        characterId: sc.send.characterId,
        callingParticipantId: null,
      });
      record.send = { json: JSON.stringify(out), fmt: formatSendMailResults(out) };

      if (sc.readContent && out.success && out.path) {
        const { readDatabaseDocument } = await import('@/lib/mount-index/database-store');
        const { content } = await readDatabaseDocument(meta.recipientVault, out.path);
        record.content = content;
      }
    }

    if (sc.listAfterSend || sc.listOnly) {
      const characterId = sc.listAfterSend ?? sc.listOnly;
      const { executeListEmailTool, formatListEmailResults } = await import(
        '@/lib/tools/handlers/list-email-handler'
      );
      const out = await executeListEmailTool({}, {
        userId: spec.userId,
        chatId: 'chat-x',
        characterId,
      });
      record.list = { json: JSON.stringify(out), fmt: formatListEmailResults(out) };
    }

    jest.useRealTimers();
    closeMountIndexSQLiteClient();
    await closeDatabase();

    lines.push(JSON.stringify(record));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`mail-tools oracle wrote ${outPath} (${lines.length} scenarios)\n`);
}

test('mail-tools oracle', async () => {
  await main();
});
