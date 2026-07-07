/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the Courier transport (W4.4a4; v4
 * `lib/services/chat-message/courier-transport.service.ts`
 * `dispatchCourierTransport`).
 *
 * Drives v4's REAL `dispatchCourierTransport` over the committed corpus
 * (harness/oracle/fixtures/courier-transport-tier3.json) against the REAL two-DB
 * fixture. The dispatch calls NO model and touches no host I/O — it renders the
 * assembled `formattedMessages` as Markdown, persists a placeholder ASSISTANT
 * message (with the rendered bundle in `pendingExternalPrompt`), pauses the chat,
 * and emits the `pendingExternalTurn` + `done` SSE frames — so ONLY the DB layer
 * is un-mocked (real cipher driver + repositories).
 *
 * Per call we record, with the minted placeholder id normalized to `<id>`:
 *   - the returned `ProcessMessageResult`;
 *   - the ordered SSE-event trace (decoded from the recording controller);
 *   - the persisted placeholder row's `role`/`content`/`participantId`/`provider`/
 *     `modelName` + the three `pendingExternal*` columns (the rendered-bundle
 *     BYTES — this is what proves the renderers + the delta events);
 *   - the chat's `isPaused`.
 *
 * The paste/cancel resolvers live in a Next.js route handler (not exported), so
 * their differential is deferred to the Phase-4 HTTP transport; their constituent
 * repo ops (`updateMessage` / `chats.update` / the enqueues) are each
 * tier-2/tier-3-proven, and the ported service functions are Rust-unit-tested.
 *
 * Run (Node 24, from the v4 checkout; cases copied out of `.claude`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; TMPO=/tmp/qt-oracle-w444
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-cour-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-cour-mount.db \
 *     $N/npx tsx $TMPO/oracle/fixtures/build-courier-transport-fixture.ts
 *   QT_FIXTURE_COUR_MAIN=/tmp/qt-cour-main.db QT_FIXTURE_COUR_MOUNT=/tmp/qt-cour-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-courier.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/oracle/cases" -- courier-transport-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface AttachmentSpec {
  id: string;
  filename?: string;
  mimeType?: string;
  size?: number;
}
interface FormattedMsgSpec {
  role: string;
  content: string;
  name?: string;
  attachments?: AttachmentSpec[];
}
interface CallSpec {
  name: string;
  chatId: string;
  responderCharacter: string;
  responderParticipant: string;
  userParticipantId: string | null;
  isMultiCharacter: boolean;
  participantCharacters: string[];
  courierDeltaMode: boolean;
  provider: string;
  modelName: string;
  formattedMessages: FormattedMsgSpec[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  resolvedIdentityName: string;
  calls: CallSpec[];
}

function normId(v: unknown, minted: string): unknown {
  return v === minted ? '<id>' : v;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'courier-transport-tier3.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_COUR_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_COUR_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_COUR_MAIN / QT_FIXTURE_COUR_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cour-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'cour-main.db');
  const workMount = join(scratch, 'cour-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories')
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { dispatchCourierTransport } = await import(
    '@/lib/services/chat-message/courier-transport.service'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const decoder = new TextDecoder();
  function makeController(sink: unknown[]) {
    return {
      enqueue: (u: Uint8Array) => {
        const text = decoder.decode(u);
        for (const line of text.split('\n')) {
          const t = line.trim();
          if (t.startsWith('data:')) {
            const body = t.slice('data:'.length).trim();
            if (body) sink.push(JSON.parse(body));
          }
        }
      },
      close: () => {},
      error: () => {},
    };
  }
  const encoder = new TextEncoder();

  const lines: string[] = [];

  for (const call of spec.calls) {
    const chat = await repos.chats.findById(call.chatId);
    if (!chat) throw new Error(`chat not found: ${call.chatId}`);
    const character = await repos.characters.findById(call.responderCharacter);
    if (!character) throw new Error(`character not found: ${call.responderCharacter}`);
    const characterParticipant = (
      chat.participants as Array<{ id: string; status?: string }>
    ).find((p) => p.id === call.responderParticipant);
    if (!characterParticipant) {
      throw new Error(`responder participant not found: ${call.responderParticipant}`);
    }
    const participantCharacters = new Map<string, unknown>();
    for (const cid of call.participantCharacters) {
      const c = await repos.characters.findById(cid);
      if (c) participantCharacters.set(cid, c);
    }

    const eventSink: unknown[] = [];
    const controller = makeController(eventSink);
    const streaming = {
      effectiveProfile: {
        id: 'profile-cour-0000-0000-000000000001',
        provider: call.provider,
        modelName: call.modelName,
        baseUrl: null,
        courierDeltaMode: call.courierDeltaMode,
      },
      effectiveApiKey: '',
    };

    const result = await dispatchCourierTransport({
      repos,
      chatId: call.chatId,
      chat: chat as never,
      character: character as never,
      characterParticipant: characterParticipant as never,
      userParticipantId: call.userParticipantId,
      isMultiCharacter: call.isMultiCharacter,
      participantCharacters: participantCharacters as never,
      resolvedIdentity: {
        name: spec.resolvedIdentityName,
        description: '',
        characterId: null,
      },
      formattedMessages: call.formattedMessages as never,
      streaming: streaming as never,
      controller: controller as never,
      encoder,
    });

    const minted = result.messageId as string;

    // Normalize the minted placeholder id in the result + the events.
    const resultOut = { ...result, messageId: normId(result.messageId, minted) };
    const eventsOut = (eventSink as Array<Record<string, unknown>>).map((e) => {
      const o = { ...e };
      if ('messageId' in o) o.messageId = normId(o.messageId, minted);
      return o;
    });

    // Read the persisted placeholder row back and record its bytes (id +
    // timestamps dropped — the bundle bytes are the proof).
    const messages = await repos.chats.getMessages(call.chatId);
    const placeholder = (messages as Array<Record<string, unknown>>).find(
      (m) => m.id === minted
    );
    if (!placeholder) throw new Error(`placeholder not persisted: ${minted}`);
    const placeholderOut = {
      role: placeholder.role,
      content: placeholder.content,
      participantId: placeholder.participantId ?? null,
      provider: placeholder.provider ?? null,
      modelName: placeholder.modelName ?? null,
      pendingExternalPrompt: placeholder.pendingExternalPrompt ?? null,
      pendingExternalPromptFull: placeholder.pendingExternalPromptFull ?? null,
      pendingExternalAttachments: placeholder.pendingExternalAttachments ?? null,
    };

    const freshChat = await repos.chats.findById(call.chatId);

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result: resultOut }));
    lines.push(JSON.stringify({ kind: 'events', call: call.name, events: eventsOut }));
    lines.push(JSON.stringify({ kind: 'placeholder', call: call.name, row: placeholderOut }));
    lines.push(
      JSON.stringify({ kind: 'chat', call: call.name, isPaused: freshChat?.isPaused === true })
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`courier-transport oracle wrote ${outPath}\n`);
}

test('courier-transport tier-3 oracle', async () => {
  await main();
});
