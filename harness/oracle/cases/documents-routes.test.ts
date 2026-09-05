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
}

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
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'doc-'));
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
    const out = await c.run({ chatActions, standalone, context });
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
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
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const GEN = 'Quilltap General';
  const SB = 'http://localhost/api/v1/documents';

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
        respond(await chatActions.handleAccessibleStores(CHAT_A, context, { all: true })),
    },
    {
      name: 'chat_accessible_stores_default',
      run: async ({ chatActions, context }) =>
        respond(await chatActions.handleAccessibleStores(CHAT_A, context, {})),
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
}

// Jest needs a test to invoke; the "oracle" is the side effect (the NDJSON).
it('emits the documents route oracle', async () => {
  await main();
}, 120000);
