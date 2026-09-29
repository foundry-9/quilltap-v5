/**
 * @jest-environment node
 *
 * Differential ORACLE for the Post Office tools: `send_mail` / `list_mail` (W4.1d5,
 * renamed from `list_email` at v4 `39bc98ffc`) and — P4.D234 — `read_mail`
 * (`39bc98ffc`) / `discard_mail` (`12c336fad`). Drives v4's REAL handlers over the
 * shared two-DB fixture, ONE fresh copy per scenario (the tools WRITE), with the
 * delivery clock (`new Date().toISOString()`) PINNED via fake timers so the minted
 * `sentAt` — and thus the letter path + formatted date — is deterministic.
 *
 * Each scenario is an ordered list of OPS; the record is
 *   { label, steps: [ { op, json?, fmt?, content?, rows? } ] }
 * one step per op, in order:
 *   - send / list / read / discard — the handler's `JSON.stringify(out)` + format
 *   - content — a letter's stored content (null when absent), read back
 *   - mount — the reader vault's link rows (relativePath, grouped, the two
 *     character flags) + the file/document row counts, for the GC and
 *     case-sensitivity comparands
 *   - docRead — `doc_read_file` through the doc-edit handler (the opacity
 *     covenant's contrast: the SAME opaque character is refused its own vault
 *     there while `read_mail` reaches it)
 *
 * TZ MUST be UTC (the mailbox date uses the system timezone by v4's design; the
 * Rust port computes it in UTC — run this oracle with `TZ=UTC`).
 *
 * Real-DB-under-jest (search-tools recipe). NO model boundary is touched.
 *
 * Run (Node 24, from a v4 tree at or after `12c336fad`; STAGE outside any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5 ; STAGE=/tmp/qt-mail-carina-tools-stage
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-mail-carina-tools-fixture.ts
 *   TZ=UTC QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-mail-tools.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "mail-tools\.test\.ts$"
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
  readerId: string;
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
    senderVault: string;
    readerVault: string;
    readerPaths: Record<string, string>;
  };
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  type Op =
    | { op: 'send'; args: unknown; characterId: string | undefined }
    | { op: 'list'; characterId: string }
    | { op: 'read'; args: unknown; characterId: string | undefined }
    | { op: 'discard'; args: unknown; characterId: string | undefined }
    | { op: 'content'; vault: 'recipient' | 'sender' | 'reader'; path: string | 'SENT' }
    | { op: 'mount' }
    | { op: 'docRead'; uri: string; characterId: string };
  interface Scenario {
    label: string;
    ops: Op[];
  }

  const R = spec.readerId;
  const P = meta.readerPaths;
  const name = (path: string): string => path.replace(/^Mail\//, '');
  const MISSING_ID = 'dead0000-0000-4000-8000-00000000dead';

  const scenarios: Scenario[] = [
    {
      label: 'send_and_list',
      ops: [
        { op: 'send', args: { character: 'Aurora', message: 'Hello Aurora, the stars are bright tonight.' }, characterId: spec.senderId },
        { op: 'content', vault: 'recipient', path: 'SENT' },
        { op: 'list', characterId: spec.recipientId },
      ],
    },
    {
      label: 'reply',
      ops: [
        { op: 'send', args: { character: 'Aurora', message: 'I will be there.', in_reply_to: spec.seedLetterInSenderMailbox.path }, characterId: spec.senderId },
        { op: 'content', vault: 'recipient', path: 'SENT' },
      ],
    },
    // P4.D234 (v4 `39bc98ffc`): `in_reply_to` takes the FILE NAME and the
    // recipient's letter STORES the resolved `Mail/…` path, whichever form was
    // named. A bare name used to be reply-not-found; `.md` is optional; a
    // leading slash no longer survives into the frontmatter; a sub-path that
    // used to be read is now refused.
    {
      label: 'reply_bare_name',
      ops: [
        { op: 'send', args: { character: 'Aurora', message: 'By its name alone.', in_reply_to: name(spec.seedLetterInSenderMailbox.path).replace(/\.md$/, '') }, characterId: spec.senderId },
        { op: 'content', vault: 'recipient', path: 'SENT' },
      ],
    },
    {
      label: 'reply_leading_slash',
      ops: [
        { op: 'send', args: { character: 'Aurora', message: 'With a slash.', in_reply_to: '/' + spec.seedLetterInSenderMailbox.path }, characterId: spec.senderId },
        { op: 'content', vault: 'recipient', path: 'SENT' },
      ],
    },
    {
      label: 'reply_self_uri',
      ops: [
        { op: 'send', args: { character: 'Aurora', message: 'By its URI.', in_reply_to: 'qtap://self/' + spec.seedLetterInSenderMailbox.path }, characterId: spec.senderId },
        { op: 'content', vault: 'recipient', path: 'SENT' },
      ],
    },
    {
      label: 'reply_sub_path',
      ops: [
        { op: 'send', args: { character: 'Aurora', message: 'x', in_reply_to: 'Mail/sub/1700000000000-from-aurora.md' }, characterId: spec.senderId },
      ],
    },
    {
      label: 'reply_not_found',
      ops: [
        { op: 'send', args: { character: 'Aurora', message: 'x', in_reply_to: 'Mail/9999999999999-from-nobody.md' }, characterId: spec.senderId },
      ],
    },
    { label: 'missing_message', ops: [{ op: 'send', args: { character: 'Aurora' }, characterId: spec.senderId }] },
    { label: 'recipient_not_found', ops: [{ op: 'send', args: { character: 'Nobody', message: 'x' }, characterId: spec.senderId }] },
    { label: 'no_character', ops: [{ op: 'send', args: { character: 'Aurora', message: 'x' }, characterId: undefined }] },
    { label: 'list_empty', ops: [{ op: 'list', characterId: spec.emptyId }] },
    { label: 'list_single', ops: [{ op: 'list', characterId: spec.recipientId }] },
    // ── P4.D65: the three archived-character refusals. All three fire on the
    // tombstone FLAG, over a character whose vault is perfectly intact.
    { label: 'send_from_archived_sender', ops: [{ op: 'send', args: { character: 'Aurora', message: 'One last letter.' }, characterId: spec.archivedId }] },
    // BY ID. `resolveCharacterByNameOrId` deliberately keeps resolving an
    // archived character by exact id while skipping it on a NAME match, so
    // this is the only way to reach the RECIPIENT refusal at all.
    { label: 'send_to_archived_recipient_by_id', ops: [{ op: 'send', args: { character: spec.archivedId, message: 'Are you still there?' }, characterId: spec.senderId }] },
    // BY NAME — the other side of that same resolver rule.
    { label: 'send_to_archived_recipient_by_name', ops: [{ op: 'send', args: { character: spec.archivedName, message: 'Are you still there?' }, characterId: spec.senderId }] },
    { label: 'list_archived', ops: [{ op: 'list', characterId: spec.archivedId }] },

    // ── P4.D234: read_mail (v4 `39bc98ffc`). Bertie is OPAQUE (no
    // systemTransparency) — every arm here is also the covenant-bypass proof.
    { label: 'list_reader', ops: [{ op: 'list', characterId: R }, { op: 'mount' }] },
    {
      label: 'read_unalerted',
      ops: [
        { op: 'read', args: { letter: name(P.unalerted) }, characterId: R },
        { op: 'content', vault: 'reader', path: P.unalerted },
        { op: 'list', characterId: R },
      ],
    },
    {
      label: 'read_already_alerted_no_ext',
      ops: [
        { op: 'read', args: { letter: name(P.alerted).replace(/\.md$/, '') }, characterId: R },
        { op: 'content', vault: 'reader', path: P.alerted },
      ],
    },
    { label: 'read_self_uri', ops: [{ op: 'read', args: { letter: 'qtap://self/' + P.unalerted }, characterId: R }] },
    { label: 'read_leading_slash', ops: [{ op: 'read', args: { letter: '/' + P.unalerted }, characterId: R }] },
    { label: 'read_blank', ops: [{ op: 'read', args: { letter: name(P.blank) }, characterId: R }] },
    { label: 'read_extra_keys', ops: [{ op: 'read', args: { letter: name(P.unalerted), extra: 1 }, characterId: R }] },
    { label: 'read_not_found', ops: [{ op: 'read', args: { letter: 'no-such-letter' }, characterId: R }] },
    { label: 'read_rummage', ops: [{ op: 'read', args: { letter: 'Notes/secret.md' }, characterId: R }] },
    { label: 'read_rummage_spaces', ops: [{ op: 'read', args: { letter: '   ' }, characterId: R }] },
    { label: 'read_rummage_uri_after_slash', ops: [{ op: 'read', args: { letter: '/qtap://self/' + P.unalerted }, characterId: R }] },
    // The ref is resolved BEFORE the character lookup: an unknown character
    // with an escaping ref hears the rummage refusal, not "cannot find".
    { label: 'read_rummage_before_lookup', ops: [{ op: 'read', args: { letter: '../secret.md' }, characterId: MISSING_ID }] },
    { label: 'read_missing_character', ops: [{ op: 'read', args: { letter: name(P.unalerted) }, characterId: MISSING_ID }] },
    { label: 'read_no_character', ops: [{ op: 'read', args: { letter: name(P.unalerted) }, characterId: undefined }] },
    { label: 'read_parse_empty', ops: [{ op: 'read', args: { letter: '' }, characterId: R }] },
    { label: 'read_parse_nonstring', ops: [{ op: 'read', args: { letter: 5 }, characterId: R }] },
    { label: 'read_parse_missing', ops: [{ op: 'read', args: {}, characterId: R }] },
    { label: 'read_parse_nonobject', ops: [{ op: 'read', args: 'nope', characterId: R }] },
    { label: 'read_archived', ops: [{ op: 'read', args: { letter: 'anything.md' }, characterId: spec.archivedId }] },
    {
      label: 'read_mixed_case_by_lower',
      ops: [
        { op: 'read', args: { letter: name(P.mixedCase).toLowerCase() }, characterId: R },
        { op: 'mount' },
        { op: 'content', vault: 'reader', path: P.mixedCase },
      ],
    },
    {
      label: 'read_protected',
      ops: [
        { op: 'read', args: { letter: name(P.protected) }, characterId: R },
        { op: 'mount' },
      ],
    },
    {
      label: 'read_hard_linked',
      ops: [
        { op: 'read', args: { letter: name(P.hardLinked) }, characterId: R },
        { op: 'mount' },
        { op: 'content', vault: 'reader', path: P.hardLinkedTo },
      ],
    },
    {
      // The covenant contrast: the same opaque character, the same letter.
      label: 'covenant_contrast',
      ops: [
        { op: 'docRead', uri: 'qtap://self/' + P.unalerted, characterId: R },
        { op: 'read', args: { letter: name(P.unalerted) }, characterId: R },
      ],
    },

    // ── P4.D234: discard_mail (v4 `12c336fad`).
    {
      label: 'discard_ok',
      ops: [
        { op: 'discard', args: { letter: name(P.unalerted) }, characterId: R },
        { op: 'content', vault: 'reader', path: P.unalerted },
        { op: 'list', characterId: R },
        { op: 'mount' },
      ],
    },
    {
      label: 'discard_twice',
      ops: [
        { op: 'discard', args: { letter: name(P.blank) }, characterId: R },
        { op: 'discard', args: { letter: name(P.blank) }, characterId: R },
      ],
    },
    {
      label: 'discard_hard_linked',
      ops: [
        { op: 'discard', args: { letter: name(P.hardLinked) }, characterId: R },
        { op: 'mount' },
        { op: 'content', vault: 'reader', path: P.hardLinkedTo },
      ],
    },
    {
      label: 'discard_protected',
      ops: [
        { op: 'discard', args: { letter: 'qtap://self/' + P.protected }, characterId: R },
        { op: 'mount' },
      ],
    },
    {
      label: 'discard_mixed_case_by_lower',
      ops: [
        { op: 'discard', args: { letter: name(P.mixedCase).toLowerCase() }, characterId: R },
        { op: 'mount' },
      ],
    },
    {
      label: 'read_then_discard_then_read',
      ops: [
        { op: 'read', args: { letter: name(P.alerted) }, characterId: R },
        { op: 'discard', args: { letter: name(P.alerted) }, characterId: R },
        { op: 'read', args: { letter: name(P.alerted) }, characterId: R },
        { op: 'mount' },
      ],
    },
    { label: 'discard_not_found', ops: [{ op: 'discard', args: { letter: 'no-such-letter.md' }, characterId: R }] },
    { label: 'discard_rummage', ops: [{ op: 'discard', args: { letter: 'Mail/Mail/x' }, characterId: R }] },
    { label: 'discard_rummage_before_lookup', ops: [{ op: 'discard', args: { letter: 'a\\b' }, characterId: MISSING_ID }] },
    { label: 'discard_missing_character', ops: [{ op: 'discard', args: { letter: name(P.unalerted) }, characterId: MISSING_ID }] },
    { label: 'discard_no_character', ops: [{ op: 'discard', args: { letter: name(P.unalerted) }, characterId: undefined }] },
    { label: 'discard_parse_empty', ops: [{ op: 'discard', args: { letter: '' }, characterId: R }] },
    { label: 'discard_parse_nonobject', ops: [{ op: 'discard', args: null, characterId: R }] },
    { label: 'discard_archived', ops: [{ op: 'discard', args: { letter: 'anything.md' }, characterId: spec.archivedId }] },
    // A different character cannot reach Bertie's letter: the name resolves
    // into the CALLER's own postbox, where no such letter rests.
    { label: 'discard_other_characters_letter', ops: [{ op: 'discard', args: { letter: name(P.unalerted) }, characterId: spec.recipientId }, { op: 'mount' }] },
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
    const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
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

    const ctx = (characterId: string | undefined) => ({
      userId: spec.userId,
      chatId: 'chat-x',
      characterId,
    });
    const vaultOf = { recipient: meta.recipientVault, sender: meta.senderVault, reader: meta.readerVault };
    let sentPath: string | undefined;
    const steps: Record<string, unknown>[] = [];

    for (const op of sc.ops) {
      if (op.op === 'send') {
        const { executeSendMailTool, formatSendMailResults } = await import(
          '@/lib/tools/handlers/send-mail-handler'
        );
        const out = await executeSendMailTool(op.args, { ...ctx(op.characterId), callingParticipantId: null });
        sentPath = out.success ? out.path : undefined;
        steps.push({ op: 'send', json: JSON.stringify(out), fmt: formatSendMailResults(out) });
      } else if (op.op === 'list') {
        const { executeListMailTool, formatListMailResults } = await import(
          '@/lib/tools/handlers/list-mail-handler'
        );
        const out = await executeListMailTool({}, ctx(op.characterId));
        steps.push({ op: 'list', json: JSON.stringify(out), fmt: formatListMailResults(out) });
      } else if (op.op === 'read') {
        const { executeReadMailTool, formatReadMailResults } = await import(
          '@/lib/tools/handlers/read-mail-handler'
        );
        const out = await executeReadMailTool(op.args, ctx(op.characterId));
        steps.push({ op: 'read', json: JSON.stringify(out), fmt: formatReadMailResults(out) });
      } else if (op.op === 'discard') {
        const { executeDiscardMailTool, formatDiscardMailResults } = await import(
          '@/lib/tools/handlers/discard-mail-handler'
        );
        const out = await executeDiscardMailTool(op.args, ctx(op.characterId));
        steps.push({ op: 'discard', json: JSON.stringify(out), fmt: formatDiscardMailResults(out) });
      } else if (op.op === 'content') {
        const { readDatabaseDocumentIfExists } = await import('@/lib/mount-index/database-store');
        const path = op.path === 'SENT' ? sentPath : op.path;
        const content = path ? await readDatabaseDocumentIfExists(vaultOf[op.vault], path) : null;
        steps.push({ op: 'content', content });
      } else if (op.op === 'mount') {
        const midb = getRawMountIndexDatabase();
        if (!midb) throw new Error('mount-index DB handle unavailable');
        const links = midb
          .prepare(
            'SELECT relativePath, linkGroupId IS NOT NULL AS grouped, allowCharacterRead, allowCharacterWrite FROM doc_mount_file_links WHERE mountPointId = ? ORDER BY relativePath',
          )
          .all(meta.readerVault);
        const count = (t: string) => (midb.prepare(`SELECT COUNT(*) AS n FROM ${t}`).get() as { n: number }).n;
        steps.push({
          op: 'mount',
          rows: { links, files: count('doc_mount_files'), documents: count('doc_mount_documents') },
        });
      } else if (op.op === 'docRead') {
        const { executeDocEditTool, formatDocEditResults } = await import(
          '@/lib/tools/handlers/doc-edit-handler'
        );
        const out = await executeDocEditTool('doc_read_file', { uri: op.uri }, ctx(op.characterId) as never);
        steps.push({ op: 'docRead', success: out.success, fmt: formatDocEditResults('doc_read_file', out) });
      }
    }

    jest.useRealTimers();
    closeMountIndexSQLiteClient();
    await closeDatabase();

    lines.push(JSON.stringify({ label: sc.label, steps }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`mail-tools oracle wrote ${outPath} (${lines.length} scenarios)\n`);
}

test('mail-tools oracle', async () => {
  await main();
});
