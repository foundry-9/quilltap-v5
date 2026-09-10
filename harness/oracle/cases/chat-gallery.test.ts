/**
 * @jest-environment node
 *
 * P4.D174 CHAT-GALLERY ORACLE: drives v4's REAL `lib/photos/chat-gallery.ts`
 * (through the REAL `GET /chats/[id]?action=gallery` route) and the REAL
 * chat-scoped `POST /chats/[id]?action=save-image` action over a FRESH copy of
 * the committed chat-gallery fixture per case, and emits each response body so
 * the Rust ports (`photos::chat_gallery`, `api::chat_media::{chat_gallery,
 * chat_save_gallery_image}`) diff byte-for-byte.
 *
 * Cases: the whole roll (`gallery` — every classifier arm, the dedup, the sort
 * and the seven-key `counts`), the portrait-only roll, the 404, the entry KEY
 * ORDER as a raw sequence (v4's entry key order is its JS construction order
 * and a later pass may APPEND `messageId`, so the shape is not a fixed
 * declaration order — every other family sorts keys and cannot see it), the
 * `/chats/[id]/files` listing (the message-attachment walk the gallery now
 * shares with it), and the save-image ladder: a `files` id, a vault
 * (character-attributed) album, a `doc_mount_file_links` id, the ALREADY_SAVED
 * 409, a non-member 400, the sha-twin ALIAS id (a real v4 quirk — the twin's
 * link id resolves in the collector but names no ENTRY, so the membership guard
 * refuses it), the schema sentences, the 404, and the MESSAGE-scoped leg's
 * newly-shared schema (`86d59660c` deleted its private uuid schema, so a
 * non-uuid `fileId` now passes the parse and fails at the attachment guard).
 *
 * Two more arms carry API.md's claim that `DELETE /api/v1/chat-files/[id]`
 * accepts only the `"file"` species: a `doc_mount_file_links.id` is a 404 there
 * (which is what makes `idKind` load-bearing in the gallery modal), a
 * `files.id` from the same roll is not.
 *
 * `readImageBuffer` is jest.setup's storage-manager stub ("mock file content"),
 * exactly as the courier-images oracle leaves it — the `image_bytes` case emits
 * those bytes so the Rust canned `FileBytesStore` returns the same. The clock is
 * frozen to NOW_MS for the mutating cases (`keptAt`).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cg-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-gallery.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-gallery-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_CG_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-gallery-main.db \
 *   QT_FIXTURE_CG_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-gallery-mount.db \
 *   QT_FIXTURE_CG_META=$V5W/crates/quilltap-web/tests/fixtures/chat-gallery-main.db.meta.json \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-gallery.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-gallery\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}
interface Meta {
  aldaVault: string;
  branVault: string;
  coraVault: string;
  generalMp: string;
  keptLinkId: string;
  twinLinkId: string;
  notesLinkId: string;
}

// Pinned entity ids (shared verbatim with the builder + the Rust differential).
const CHAT = 'c1000000-0000-4000-8000-000000000001';
const PORTRAIT_CHAT = 'c1000000-0000-4000-8000-000000000002';
const NO_CHAT = 'c9000000-0000-4000-8000-0000000000ff';
const M_GEN = 'd1000000-0000-4000-8000-000000000001';
const F_GEN = 'f1000000-0000-4000-8000-000000000007';
const F_UPLOAD = 'f1000000-0000-4000-8000-000000000008';
const F_MISSING = '99999999-9999-4999-8999-999999999999';

const NOW_MS = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // jest.setup stubs the character-vault bridge to a single "mock-vault-mount";
  // un-mock it so each character resolves its own real vault (the save-image
  // attribution branch). See [[jest-oracle-character-vault-bridge-is-mocked]].
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // Side-effect seams saveImageToAlbum touches → no-ops (mirror NoSideEffects).
  jest.doMock('@/lib/mount-index/mount-chunk-cache', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/mount-chunk-cache'),
    invalidateMountPoint: () => {},
  }));
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/embedding-scheduler'),
    enqueueEmbeddingJobsForMountPoint: async () => {},
  }));
  // The chat GET handler pulls the markdown renderer (unified/ESM) — mock it
  // away (the chat GET body isn't under test here).
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

interface CaseSpec {
  name: string;
  freezeClock?: boolean;
  run: () => Promise<{ status: number; body: unknown; extra?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';
const FILES_ROUTE = '@/app/api/v1/chats/[id]/files/route';
const MSG_ROUTE = '@/app/api/v1/chats/[id]/messages/[messageId]/route';
const CHATFILE_ROUTE = '@/app/api/v1/chat-files/[id]/route';

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'cg-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  const RealDate = Date;
  if (c.freezeClock) {
    const iso = new RealDate(NOW_MS).toISOString();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    global.Date = class extends RealDate {
      constructor(...a: unknown[]) {
        if (a.length === 0) super(iso);
        // @ts-expect-error forward variadic args
        else super(...a);
      }
      static now(): number {
        return NOW_MS;
      }
    } as unknown as DateConstructor;
  }

  try {
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.extra !== undefined ? { extra: out.extra } : {}),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-gallery-web.json'), 'utf8'),
  ) as Spec;
  const metaPath = process.env.QT_FIXTURE_CG_META ?? '';
  const meta = JSON.parse(fs.readFileSync(metaPath, 'utf8')) as Meta;

  const fixtures = {
    main: process.env.QT_FIXTURE_CG_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CG_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cg-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1';
  const galleryGet = async (chatId: string) =>
    respond(
      await (await loadRoute(CHAT_ROUTE)).GET(mockRequest(`${B}/chats/${chatId}?action=gallery`), {
        params: Promise.resolve({ id: chatId }),
      }),
    );
  const savePost = async (chatId: string, body: unknown) =>
    respond(
      await (await loadRoute(CHAT_ROUTE)).POST(
        mockRequest(`${B}/chats/${chatId}?action=save-image`, body),
        { params: Promise.resolve({ id: chatId }) },
      ),
    );

  const cases: CaseSpec[] = [
    // The exact bytes saveImageToAlbum reads for a `files` id (jest.setup's
    // storage-manager stub) — the Rust canned FileBytesStore returns these.
    {
      name: 'image_bytes',
      run: async () => {
        const { readImageBuffer } = await import('@/lib/images-v2');
        const buf = await readImageBuffer(F_GEN);
        return { status: 200, body: { base64: Buffer.from(buf).toString('base64') } };
      },
    },
    // --- The roll ---
    { name: 'gallery', run: async () => galleryGet(CHAT) },
    { name: 'gallery_portrait_only', run: async () => galleryGet(PORTRAIT_CHAT) },
    { name: 'gallery_missing_chat', run: async () => galleryGet(NO_CHAT) },
    // The RAW per-entry key sequence — the one comparand a key-sorting
    // normalizer cannot see. `messageId` appended after `deletable` is v4's
    // `noteMessage` mutating an entry an earlier pass already built.
    //
    // ⚠ The keys are taken off the SERIALIZED value, not the live object.
    // `NextResponse.json(x).json()` hands back x BY REFERENCE in this
    // environment, and v4 sets `characterId` unconditionally in
    // `passLinkedFiles` — so the live object carries `characterId: undefined`
    // on every non-avatar entry, a key `JSON.stringify` drops and no client
    // ever sees. The wire is the contract; round-trip first.
    {
      name: 'gallery_key_order',
      run: async () => {
        const { status, body } = await galleryGet(CHAT);
        const wire = JSON.parse(JSON.stringify(body)) as {
          entries: Array<Record<string, unknown>>;
          counts: object;
        };
        return {
          status,
          body: {
            entries: wire.entries.map((e) => ({ id: e.id, keys: Object.keys(e) })),
            countsKeys: Object.keys(wire.counts),
            topKeys: Object.keys(wire),
          },
        };
      },
    },
    // --- The listing that now shares the walk ---
    {
      name: 'files_list',
      run: async () =>
        respond(
          await (await loadRoute(FILES_ROUTE)).GET(mockRequest(`${B}/chats/${CHAT}/files`), {
            params: Promise.resolve({ id: CHAT }),
          }),
        ),
    },
    // --- Save: a `files` id into a non-vault album (operator attribution) ---
    {
      name: 'save_image_file',
      freezeClock: true,
      run: async () =>
        savePost(CHAT, { fileId: F_GEN, mountPointId: meta.generalMp, caption: 'The alley' }),
    },
    // --- Save: into a participant's vault (character attribution) ---
    {
      name: 'save_image_vault',
      freezeClock: true,
      run: async () =>
        savePost(CHAT, { fileId: F_UPLOAD, mountPointId: meta.aldaVault, tags: ['alley'] }),
    },
    // --- Save: a doc_mount_file_links id (the `kept` entry) ---
    {
      name: 'save_image_link',
      freezeClock: true,
      run: async () => savePost(CHAT, { fileId: meta.keptLinkId, mountPointId: meta.generalMp }),
    },
    // --- Save: already in that album → 409 with relativePath + keptAt. The
    //     SAME save runs twice; the second is the answer. (Saving a link id
    //     into the vault it already lives in does NOT refuse, because
    //     `readImageBuffer` is jest.setup's stub for every id — the bytes it
    //     hashes are the stub's, not the blob's.) ---
    {
      name: 'save_image_already',
      freezeClock: true,
      run: async () => {
        await savePost(CHAT, { fileId: F_GEN, mountPointId: meta.generalMp });
        return savePost(CHAT, { fileId: F_GEN, mountPointId: meta.generalMp });
      },
    },
    // --- Save: an id the gallery does not hold ---
    {
      name: 'save_image_not_member',
      run: async () => savePost(CHAT, { fileId: F_MISSING, mountPointId: meta.generalMp }),
    },
    // --- Save: the sha twin's LINK id. The collector aliases it to the `files`
    //     entry, so `has(id)` is true but no ENTRY carries that id — the
    //     membership guard refuses it. ---
    {
      name: 'save_image_twin_alias',
      run: async () => savePost(CHAT, { fileId: meta.twinLinkId, mountPointId: meta.generalMp }),
    },
    // --- Save: the shared schema's sentences, joined '; ' ---
    { name: 'save_image_empty_body', run: async () => savePost(CHAT, {}) },
    {
      name: 'save_image_blank_ids',
      run: async () => savePost(CHAT, { fileId: '', mountPointId: '' }),
    },
    {
      name: 'save_image_wrong_types',
      run: async () => savePost(CHAT, { fileId: 42, mountPointId: true, tags: 'nope' }),
    },
    // --- Save: no such chat ---
    {
      name: 'save_image_missing_chat',
      run: async () => savePost(NO_CHAT, { fileId: F_GEN, mountPointId: meta.generalMp }),
    },
    // --- The delete the gallery modal relies on: `DELETE /chat-files/{id}`
    //     resolves a `files` row and NOTHING else, so a `doc_mount_file_links`
    //     id — half the gallery's entries — is a 404. That is what makes
    //     `idKind === 'file'` load-bearing in the modal (API.md says so in
    //     those words), and it is asserted nowhere else. ---
    {
      name: 'chat_file_delete_link_id',
      run: async () =>
        respond(
          await (await loadRoute(CHATFILE_ROUTE)).DELETE(
            mockRequest(`${B}/chat-files/${meta.keptLinkId}`),
            { params: Promise.resolve({ id: meta.keptLinkId }) },
          ),
        ),
    },
    // …and the `file` species it DOES accept, from the same roll.
    {
      name: 'chat_file_delete_file_id',
      run: async () =>
        respond(
          await (await loadRoute(CHATFILE_ROUTE)).DELETE(
            mockRequest(`${B}/chat-files/${F_UPLOAD}`),
            { params: Promise.resolve({ id: F_UPLOAD }) },
          ),
        ),
    },

    // --- The MESSAGE-scoped leg, now on the shared schema: a non-uuid fileId
    //     passes the parse (v4's private uuid schema is gone) and fails at the
    //     attachment guard instead. ---
    {
      name: 'message_save_non_uuid',
      run: async () =>
        respond(
          await (await loadRoute(MSG_ROUTE)).POST(
            mockRequest(`${B}/chats/${CHAT}/messages/${M_GEN}?action=save-image`, {
              fileId: 'not-a-uuid',
              mountPointId: 'also-not-a-uuid',
            }),
            { params: Promise.resolve({ id: CHAT, messageId: M_GEN }) },
          ),
        ),
    },
    {
      name: 'message_save_empty_body',
      run: async () =>
        respond(
          await (await loadRoute(MSG_ROUTE)).POST(
            mockRequest(`${B}/chats/${CHAT}/messages/${M_GEN}?action=save-image`, {}),
            { params: Promise.resolve({ id: CHAT, messageId: M_GEN }) },
          ),
        ),
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`chat-gallery oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('chat-gallery oracle', async () => {
  await main();
});
