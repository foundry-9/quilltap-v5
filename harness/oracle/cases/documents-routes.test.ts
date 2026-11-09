/**
 * @jest-environment node
 *
 * P4.6w DOCUMENT MODE route-surface ORACLE: drives v4's REAL chat-scoped
 * (`app/api/v1/chats/[id]/actions/documents.ts`) + standalone
 * (`app/api/v1/documents/route.ts`) handlers over a FRESH copy of the committed
 * documents-{main,mount}.db fixture per case, and emits each response body
 * (+ status + a chat_documents/documentMode state dump for the writes) so the
 * Rust ports (`api::documents::*`) can be diffed.
 *
 * Only the seams the Rust harness also neutralizes are mocked: the auth session,
 * the startup gate, and the fire-and-forget document-store refresh (reindex +
 * embedding enqueue) — the DB stack is doMocked to the REAL modules + the real
 * cipher binding.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-doc-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/documents-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/documents-web.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/documents-reach.json" "$TMPO/fixtures/"   # P4.D376
 *   cp "$V5W/harness/oracle/fixtures/documents-core.json" "$TMPO/fixtures/"   # P4.D380
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MAIN=$V5W/crates/quilltap-web/tests/fixtures/documents-main.db \
 *   QT_FIXTURE_DOC_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/documents-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-documents-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- documents-routes
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

const CHAT_A = 'c1000000-0000-4000-8000-00000000000a';
const R1 = 'f1000000-0000-4000-8000-000000000001';
const R2 = 'f1000000-0000-4000-8000-000000000002';
const R3 = 'f1000000-0000-4000-8000-000000000003';
const STANDALONE = '00000000-0000-0000-0000-000000000000';

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
  // jest.setup mocks the character-vault-bridge to a null-returning stub; un-mock
  // it so getCharacterVaultStore resolves the real vault (the P4.6i gotcha —
  // otherwise the default accessible-stores mode silently drops participant vaults).
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // Neutralize the fire-and-forget store refresh (reindex + embedding enqueue) so
  // the compared chat_documents / documentMode state is deterministic.
  jest.doMock('@/lib/doc-edit', () => ({
    ...jest.requireActual('@/lib/doc-edit'),
    reindexSingleFile: async () => {},
  }));
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    ...jest.requireActual('@/lib/mount-index/embedding-scheduler'),
    enqueueEmbeddingJobsForMountPoint: async () => {},
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
  run: (ctx: RunCtx) => Promise<{ status: number; body: unknown; tables?: unknown }>;
  // === P4.D376 === SQL applied to the per-case COPY before the database opens.
  plants?: Array<{ db: 'main' | 'mount'; sql: string }>;
  // === end P4.D376 ===
  // === P4.D380 === capture v4's logger lines (DEBUG included) for this case.
  captureLines?: boolean;
  // === end P4.D380 ===
}

// === P4.D380 === dumps for the document-core rows.
/** Every chat_documents row (any chat), id-free, in a stable order. */
async function dumpAllChatDocs(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const db = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown[] } };
  return db
    .prepare(
      'SELECT chatId, filePath, scope, mountPoint, displayTitle, isActive FROM chat_documents ORDER BY chatId, scope, filePath, mountPoint',
    )
    .all();
}
/** The saved upload's link + what it now points at (#134 / item 11). */
async function dumpUpload(rel: string): Promise<unknown> {
  const { getRawMountIndexDatabase } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const db = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => unknown[]; get: (...a: unknown[]) => unknown };
  };
  return {
    link: db
      .prepare(
        `SELECT l.relativePath, f.fileType, l.conversionStatus, l.extractedText, l.extractedTextSha256,
                l.extractionStatus, l.extractionError, l.plainTextLength, d.content
           FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId
           LEFT JOIN doc_mount_documents d ON d.fileId = l.fileId
          WHERE l.relativePath = ?`,
      )
      .all(rel),
    blobs: db.prepare('SELECT storedMimeType, sizeBytes FROM doc_mount_blobs ORDER BY storedMimeType').all(),
  };
}
// === end P4.D380 ===

interface RunCtx {
  chatActions: Record<string, (...a: unknown[]) => Promise<unknown>>;
  standalone: Record<string, (...a: unknown[]) => Promise<unknown>>;
  context: { repos: unknown };
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

/** id-free chat_documents state for a chat, sorted (scope, filePath), + the
 *  chat's documentMode. Blanks minted ids so both sides compare on identity. */
async function dumpChatDocs(chatId: string): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const db = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => unknown[]; get: (...a: unknown[]) => unknown };
  };
  const rows = db
    .prepare(
      'SELECT filePath, scope, mountPoint, displayTitle, isActive FROM chat_documents WHERE chatId = ? ORDER BY scope, filePath',
    )
    .all(chatId) as Array<Record<string, unknown>>;
  const chat = db.prepare('SELECT documentMode FROM chats WHERE id = ?').get(chatId) as
    | { documentMode: string }
    | undefined;
  return {
    documentMode: chat?.documentMode ?? null,
    docs: rows.map((r) => ({
      filePath: r.filePath,
      scope: r.scope,
      mountPoint: r.mountPoint ?? null,
      displayTitle: r.displayTitle ?? null,
      isActive: r.isActive === 1 || r.isActive === true,
    })),
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  // === P4.D380 === the logger reads LOG_LEVEL when it loads (per case, after
  // resetModules), so a capturing case sets it first.
  process.env.LOG_LEVEL = c.captureLines ? 'debug' : 'error';
  // === end P4.D380 ===
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'doc-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  // === P4.D376 ===
  if (c.plants) {
    const Database = require(
      require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'),
    );
    const hex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
    for (const [db, path] of [['main', mainWork], ['mount', mountWork]] as const) {
      const conn = new Database(path);
      conn.pragma(`key = "x'${hex}'"`);
      for (const p of c.plants.filter((x) => x.db === db)) conn.exec(p.sql);
      conn.close();
    }
  }
  // === end P4.D376 ===
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();
  try {
    const chatActions = (await import(
      '@/app/api/v1/chats/[id]/actions/documents'
    )) as unknown as Record<string, (...a: unknown[]) => Promise<unknown>>;
    const standalone = (await import('@/app/api/v1/documents/route')) as unknown as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const context = { repos: getRepositories() };
    // === P4.D380 === splice the singleton logger's transports (children share
    // the array) to record every line this case logs.
    const lines: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
    if (c.captureLines) {
      const { logger } = await import('@/lib/logger');
      const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
      internal.transports.splice(0, internal.transports.length, {
        write: (d: unknown) => {
          const data = d as { level: string; message: string; context?: Record<string, unknown> };
          const { service: _s, environment: _e, module: _m, ...fields } = data.context ?? {};
          lines.push({ level: data.level, message: data.message, fields });
        },
      });
    }
    // === end P4.D380 ===
    const out = await c.run({ chatActions, standalone, context });
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
      ...(c.captureLines ? { lines } : {}),
    };
  } finally {
    await new Promise((r) => setTimeout(r, 250));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'documents-web.json'), 'utf8'),
  ) as Spec;
  const fixtures = {
    main: process.env.QT_FIXTURE_DOC_MAIN ?? '',
    mount: process.env.QT_FIXTURE_DOC_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-doc-oracle-'));
  try {
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const GEN = 'Quilltap General';
  const SB = 'http://localhost/api/v1/documents';
  // === P4.D376 ===
  const reachSpec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'documents-reach.json'), 'utf8'),
  ) as { chatId: string; plants: Array<{ db: 'main' | 'mount'; sql: string }> };
  // === end P4.D376 ===

  const cases: CaseSpec[] = [
    // ── Chat-scoped reads ──────────────────────────────────────────────────
    {
      name: 'chat_active_document',
      run: async ({ chatActions, context }) =>
        respond(await chatActions.handleActiveDocument(CHAT_A, context)),
    },
    {
      name: 'chat_open_documents',
      run: async ({ chatActions, context }) =>
        respond(await chatActions.handleOpenDocuments(CHAT_A, context)),
    },
    {
      name: 'chat_recent_documents',
      run: async ({ chatActions, context }) =>
        respond(await chatActions.handleRecentDocuments(CHAT_A, context)),
    },
    {
      name: 'chat_accessible_stores_all',
      run: async ({ chatActions, context }) =>
        // P4.D380: v4 `0c47f4539` reads the session `user` (the cast pool) —
        // the context carries it, as the P4.D376 rows' do.
        respond(await chatActions.handleAccessibleStores(CHAT_A, { ...context, user: { id: spec.userId } }, { all: true })),
    },
    {
      name: 'chat_accessible_stores_default',
      run: async ({ chatActions, context }) =>
        respond(await chatActions.handleAccessibleStores(CHAT_A, { ...context, user: { id: spec.userId } }, {})),
    },
    {
      name: 'chat_read_document_existing',
      run: async ({ chatActions, context }) =>
        respond(
          await chatActions.handleReadDocument(
            mockRequest(SB, { filePath: 'notes/alpha.md', scope: 'document_store', mountPoint: GEN }),
            CHAT_A,
            context,
          ),
        ),
    },
    {
      name: 'chat_read_document_missing',
      run: async ({ chatActions, context }) =>
        respond(
          await chatActions.handleReadDocument(
            mockRequest(SB, { filePath: 'notes/missing.md', scope: 'document_store', mountPoint: GEN }),
            CHAT_A,
            context,
          ),
        ),
    },
    {
      name: 'chat_resolve_document_exists',
      run: async ({ chatActions, context }) =>
        respond(
          await chatActions.handleResolveDocument(
            mockRequest(SB, { filePath: 'notes/alpha.md', scope: 'document_store', mountPoint: GEN }),
            CHAT_A,
            context,
          ),
        ),
    },
    {
      name: 'chat_resolve_document_missing',
      run: async ({ chatActions, context }) =>
        respond(
          await chatActions.handleResolveDocument(
            mockRequest(SB, { filePath: 'notes/missing.md', scope: 'document_store', mountPoint: GEN }),
            CHAT_A,
            context,
          ),
        ),
    },
    // ── Chat-scoped writes (with a chat_documents/documentMode state dump) ──
    {
      name: 'chat_open_document_reactivate',
      run: async ({ chatActions, context }) => {
        const r = await respond(
          await chatActions.handleOpenDocument(
            mockRequest(SB, {
              filePath: 'notes/beta.md',
              scope: 'document_store',
              mountPoint: GEN,
              mode: 'split',
            }),
            CHAT_A,
            context,
          ),
        );
        return { ...r, tables: await dumpChatDocs(CHAT_A) };
      },
    },
    {
      name: 'chat_open_document_new_blank',
      run: async ({ chatActions, context }) => {
        const r = await respond(
          await chatActions.handleOpenDocument(
            mockRequest(SB, {
              scope: 'document_store',
              mountPoint: GEN,
              targetFolder: 'notes',
              mode: 'split',
            }),
            CHAT_A,
            context,
          ),
        );
        return { ...r, tables: await dumpChatDocs(CHAT_A) };
      },
    },
    {
      name: 'chat_close_document',
      run: async ({ chatActions, context }) => {
        const r = await respond(
          await chatActions.handleCloseDocument(
            mockRequest(SB, { chatDocumentId: R1 }),
            CHAT_A,
            context,
          ),
        );
        return { ...r, tables: await dumpChatDocs(CHAT_A) };
      },
    },
    {
      name: 'chat_write_document',
      run: async ({ chatActions, context }) => {
        const r = await respond(
          await chatActions.handleWriteDocument(
            mockRequest(SB, {
              filePath: 'notes/alpha.md',
              scope: 'document_store',
              mountPoint: GEN,
              content: '# Alpha\n\nEdited body.\n',
              diffContent: 'I\'ve made changes to "alpha.md":\n- edited',
            }),
            CHAT_A,
            context,
          ),
        );
        return r;
      },
    },
    {
      name: 'chat_write_document_conflict',
      run: async ({ chatActions, context }) =>
        respond(
          await chatActions.handleWriteDocument(
            mockRequest(SB, {
              filePath: 'notes/alpha.md',
              scope: 'document_store',
              mountPoint: GEN,
              content: 'x',
              mtime: 111,
            }),
            CHAT_A,
            context,
          ),
        ),
    },
    {
      name: 'chat_rename_document',
      run: async ({ chatActions, context }) => {
        const r = await respond(
          await chatActions.handleRenameDocument(
            mockRequest(SB, { newTitle: 'renamed', chatDocumentId: R1 }),
            CHAT_A,
            context,
          ),
        );
        return {
          ...r,
          tables: { chatA: await dumpChatDocs(CHAT_A), standalone: await dumpChatDocs(STANDALONE) },
        };
      },
    },
    {
      name: 'chat_rename_document_conflict',
      run: async ({ chatActions, context }) =>
        respond(
          await chatActions.handleRenameDocument(
            mockRequest(SB, { newTitle: 'beta', chatDocumentId: R1 }),
            CHAT_A,
            context,
          ),
        ),
    },
    {
      name: 'chat_delete_document',
      run: async ({ chatActions, context }) => {
        const r = await respond(
          await chatActions.handleDeleteDocument(
            mockRequest(SB, { chatDocumentId: R3 }),
            CHAT_A,
            context,
          ),
        );
        return { ...r, tables: await dumpChatDocs(CHAT_A) };
      },
    },
    {
      // The 404 body (v4 `0506517d3` correction (c): `notFound('File')` renders
      // "File not found", where the pre-fix `notFound('File not found')`
      // rendered "File not found not found"). Reached by deleting the SAME row
      // twice: `resolveTargetDocument` looks a named id up by id alone — no
      // active filter — so the second call still finds the (now closed) row,
      // resolves its path, and the database store answers "no such document".
      // A `chatDocumentId` absent from the fixture would NOT reach here: it
      // resolves to null and answers 400 "No active document to delete".
      name: 'chat_delete_document_missing',
      run: async ({ chatActions, context }) => {
        await chatActions.handleDeleteDocument(
          mockRequest(SB, { chatDocumentId: R1 }),
          CHAT_A,
          context,
        );
        const r = await respond(
          await chatActions.handleDeleteDocument(
            mockRequest(SB, { chatDocumentId: R1 }),
            CHAT_A,
            context,
          ),
        );
        return { ...r, tables: await dumpChatDocs(CHAT_A) };
      },
    },
    // === P4.D380 === (v4 `a3d53eae1` / `30a3b4cb7` / `5efe53ea7` /
    // `c17033f04`): the document core under Document Mode — the uploaded `.md`
    // read and saved as a document (#134), the read-only viewer and its write
    // refusal (C14), the resolve 400, the rename 404 / 400 and the delete 400,
    // and a rename's sweep over every row that reaches the same store. Plants:
    // `documents-core.json` (applied to the per-case COPY, as P4.D376's).
    ...((): CaseSpec[] => {
      const core = JSON.parse(
        fs.readFileSync(join(here, '..', 'fixtures', 'documents-core.json'), 'utf8'),
      ) as { plants: Array<{ db: 'main' | 'mount'; sql: string }> };
      const P = core.plants;
      const UP = { filePath: 'uploads/up.md', scope: 'document_store', mountPoint: GEN };
      const PIC = { filePath: 'uploads/pic.png', scope: 'document_store', mountPoint: GEN };
      const chatPost = (fn: string, body: unknown) => async ({ chatActions, context }: RunCtx) =>
        respond(await chatActions[fn](mockRequest(SB, body), CHAT_A, context));
      return [
        { name: 'p4d380_open_uploaded_md', plants: P, run: chatPost('handleOpenDocument', { ...UP, mode: 'split' }) },
        { name: 'p4d380_open_viewer', plants: P, run: chatPost('handleOpenDocument', { ...PIC, mode: 'split' }) },
        { name: 'p4d380_read_viewer', plants: P, captureLines: true, run: chatPost('handleReadDocument', PIC) },
        { name: 'p4d380_read_uploaded_md', plants: P, captureLines: true, run: chatPost('handleReadDocument', UP) },
        { name: 'p4d380_write_readonly', plants: P, captureLines: true, run: chatPost('handleWriteDocument', { ...PIC, content: 'x' }) },
        {
          name: 'p4d380_save_uploaded_md',
          plants: P,
          run: async (ctx) => {
            const r = await chatPost('handleWriteDocument', { ...UP, content: '# Up\n\nedited words\n' })(ctx);
            return { ...r, tables: await dumpUpload('uploads/up.md') };
          },
        },
        {
          name: 'p4d380_read_resolve_400',
          plants: P,
          captureLines: true,
          run: chatPost('handleReadDocument', { filePath: '../escape.md', scope: 'document_store', mountPoint: GEN }),
        },
        {
          name: 'p4d380_rename_source_missing',
          plants: P,
          run: chatPost('handleRenameDocument', { newTitle: 'phantom', chatDocumentId: 'd3800000-0000-4000-8000-0000000000c1' }),
        },
        {
          name: 'p4d380_rename_folder',
          plants: P,
          run: chatPost('handleRenameDocument', { newTitle: 'renamed', chatDocumentId: 'd3800000-0000-4000-8000-0000000000c2' }),
        },
        {
          name: 'p4d380_delete_folder',
          plants: P,
          run: chatPost('handleDeleteDocument', { chatDocumentId: 'd3800000-0000-4000-8000-0000000000c2' }),
        },
        {
          name: 'p4d380_rename_sweeps',
          plants: P,
          run: async (ctx) => {
            const r = await chatPost('handleRenameDocument', { newTitle: 'omega', chatDocumentId: R1 })(ctx);
            return { ...r, tables: await dumpAllChatDocs() };
          },
        },
        {
          name: 'p4d380_standalone_read_viewer',
          plants: P,
          run: async ({ standalone }) =>
            respond(await standalone.POST(mockRequest(`${SB}?action=read-document`, PIC))),
        },
        {
          name: 'p4d380_standalone_write_readonly',
          plants: P,
          captureLines: true,
          run: async ({ standalone }) =>
            respond(await standalone.POST(mockRequest(`${SB}?action=write-document`, { ...PIC, content: 'x' }))),
        },
        {
          name: 'p4d380_standalone_open_uploaded_md',
          plants: P,
          run: async ({ standalone }) =>
            respond(await standalone.POST(mockRequest(`${SB}?action=open-document`, UP))),
        },
        {
          name: 'p4d380_accessible_stores_lines',
          plants: P,
          captureLines: true,
          run: async ({ chatActions, context }) =>
            respond(await chatActions.handleAccessibleStores(CHAT_A, { ...context, user: { id: spec.userId } }, {})),
        },
      ];
    })(),
    // === end P4.D380 ===
    // === P4.D376 === (v4 `0c47f4539`): the accessible stores on the cast pool
    // and the persona group stores on `castGroupMounts`, over the
    // `documents-reach.json` plants. The comparand (Rust side) is `stores`
    // minus Quilltap General + `projectLibrary` (General's hold-back is
    // P4.D380's). The context carries v4's session `user`.
    {
      name: 'p4d376_accessible_stores_cast_pool',
      plants: reachSpec.plants,
      run: async ({ chatActions, context }) =>
        respond(
          await chatActions.handleAccessibleStores(
            reachSpec.chatId,
            { ...context, user: { id: spec.userId } },
            {},
          ),
        ),
    },
    {
      name: 'p4d376_group_stores_persona',
      plants: reachSpec.plants,
      run: async ({ context }) => {
        const groupStores = (await import('@/app/api/v1/chats/[id]/actions/group-stores')) as unknown as {
          handleGetGroupStores: (id: string, ctx: unknown) => Promise<unknown>;
        };
        return respond(await groupStores.handleGetGroupStores(reachSpec.chatId, context));
      },
    },
    // === end P4.D376 ===
    // ── Standalone surface ─────────────────────────────────────────────────
    {
      name: 'standalone_stores',
      run: async ({ standalone }) =>
        respond(await standalone.GET(mockRequest(`${SB}?action=accessible-stores`))),
    },
    {
      name: 'standalone_recent',
      run: async ({ standalone }) =>
        respond(
          await standalone.POST(mockRequest(`${SB}?action=recent-documents`, {})),
        ),
    },
    {
      name: 'standalone_open_existing',
      run: async ({ standalone }) =>
        respond(
          await standalone.POST(
            mockRequest(`${SB}?action=open-document`, {
              filePath: 'notes/alpha.md',
              scope: 'document_store',
              mountPoint: GEN,
            }),
          ),
        ),
    },
    {
      name: 'standalone_read',
      run: async ({ standalone }) =>
        respond(
          await standalone.POST(
            mockRequest(`${SB}?action=read-document`, {
              filePath: 'notes/gamma.md',
              scope: 'document_store',
              mountPoint: GEN,
            }),
          ),
        ),
    },
    {
      name: 'standalone_write',
      run: async ({ standalone }) =>
        respond(
          await standalone.POST(
            mockRequest(`${SB}?action=write-document`, {
              filePath: 'notes/gamma.md',
              scope: 'document_store',
              mountPoint: GEN,
              content: '# Gamma\n\nEdited.\n',
            }),
          ),
        ),
    },
    {
      name: 'standalone_rename',
      run: async ({ standalone }) =>
        respond(
          await standalone.POST(
            mockRequest(`${SB}?action=rename-document`, {
              filePath: 'notes/gamma.md',
              scope: 'document_store',
              mountPoint: GEN,
              newTitle: 'delta',
            }),
          ),
        ),
    },
    {
      name: 'standalone_delete',
      run: async ({ standalone }) =>
        respond(
          await standalone.POST(
            mockRequest(`${SB}?action=delete-document`, {
              filePath: 'notes/gamma.md',
              scope: 'document_store',
              mountPoint: GEN,
            }),
          ),
        ),
    },
  ];

  const results: Record<string, unknown>[] = [];
  for (const c of cases) {
    results.push(await runCase(spec, c, scratch, fixtures));
  }
  fs.writeFileSync(outPath, results.map((r) => JSON.stringify(r)).join('\n') + '\n');
  process.stderr.write(`wrote ${results.length} documents-route oracle rows to ${outPath}\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

// Jest needs a test to invoke; the "oracle" is the side effect (the NDJSON).
it('emits the documents route oracle', async () => {
  await main();
}, 120000);
