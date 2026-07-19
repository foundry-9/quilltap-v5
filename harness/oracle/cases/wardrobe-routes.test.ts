/**
 * @jest-environment node
 *
 * P4.9f1 WARDROBE-ROUTES ORACLE: drives v4's REAL route handlers — the GLOBAL
 * archetype tier (`app/api/v1/wardrobe/route.ts` + `[itemId]/route.ts`), the
 * transfers pair (`app/api/v1/wardrobe/transfers/route.ts`), the chat outfit
 * actions (`app/api/v1/chats/[id]/actions/outfit.ts` — `handleGetOutfit` +
 * `handleEquipSlot`, all seven modes), `regenerate-avatar.ts`, and
 * `preview-avatar/route.ts` — over a FRESH copy of the committed
 * `wardrobe-routes-{main,mount}.db` family per case. Emits each response
 * `{status, body}` so the Rust port (`api::wardrobe` + `api::chat_outfits`)
 * diffs field-by-field.
 *
 * THE CASE LIST IS NOT DECLARED HERE: both this oracle and the Rust
 * differential read the SAME `wardrobe-routes.json#cases` corpus (the
 * p4.6ay-unit-12 shared-corpus rule), so neither side can silently
 * partial-pass — the Rust side asserts its case count against this NDJSON's
 * row count.
 *
 * ── THE ONE MOCK BOUNDARY (preview only) ────────────────────────────────────
 * `createImageProvider` is stubbed to return the corpus's canned PNG (the
 * model boundary). `convertToWebP` runs REAL (sharp) and its OUTPUT is
 * captured and emitted as the `pv_ok_bytes` row — that post-transcode
 * buffer/filename/mime is exactly the seam the Rust `AvatarPreviewRenderer`
 * models, so everything below it (sha256, the vault write, the files row, the
 * response) runs real on both sides.
 *
 * Chained rows: a case with `thenOutfit` emits a second `${name}__outfit` row
 * (the same DB copy) — that is what pins PERSISTENCE, not just the response.
 *
 * Run (Node 24 — from the PINNED v4 worktree at 616930db; cp to a /tmp mirror
 * because jest ignores .claude/ paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-wroutes-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-routes.json" "$TMPO/fixtures/"
 *   cd /private/tmp/qt-v4-pin-616930db
 *   QT_FIXTURE_WROUTES_MAIN=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-main.db \
 *   QT_FIXTURE_WROUTES_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- wardrobe-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseEntry {
  name: string;
  kind: string;
  body?: unknown;
  itemId?: string;
  chatId?: string;
  thenOutfit?: string;
  emitBytes?: boolean;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  preview: { providerImageBase64: string; providerMimeType: string; revisedPrompt: string };
  cases: CaseEntry[];
}

/** The captured post-transcode image (the Rust render-seam replay). */
const webpCapture: { buffer?: Buffer; mimeType?: string; filename?: string } = {};

function mockRequest(url: string, method = 'GET', body?: unknown): unknown {
  return {
    method,
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
  // The preview's MODEL boundary — the canned provider image. Everything below
  // (convertToWebP, sha256, the vault write, files.create) runs real.
  jest.doMock('@/lib/llm/plugin-factory', () => {
    const actual = jest.requireActual('@/lib/llm/plugin-factory');
    return {
      __esModule: true,
      ...actual,
      createImageProvider: () => ({
        generateImage: async () => ({
          images: [
            {
              b64Json: spec.preview.providerImageBase64,
              mimeType: spec.preview.providerMimeType,
              revisedPrompt: spec.preview.revisedPrompt,
            },
          ],
        }),
      }),
    };
  });
  // Capture convertToWebP's REAL output — the seam the Rust renderer replays.
  jest.doMock('@/lib/files/webp-conversion', () => {
    const actual = jest.requireActual('@/lib/files/webp-conversion');
    return {
      __esModule: true,
      ...actual,
      convertToWebP: async (buffer: Buffer, mimeType: string, filename: string) => {
        const out = await actual.convertToWebP(buffer, mimeType, filename);
        webpCapture.buffer = out.buffer;
        webpCapture.mimeType = out.mimeType;
        webpCapture.filename = out.filename;
        return out;
      },
    };
  });
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const B = 'http://localhost/api/v1';

async function authedCtx(): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const specHolder = JSON.parse(process.env.QT_WROUTES_SPEC ?? '{}') as { userId?: string };
  return { user: { id: specHolder.userId }, repos: getRepositories() };
}

async function runKind(c: CaseEntry): Promise<{ status: number; body: unknown }> {
  switch (c.kind) {
    case 'wardrobeList': {
      const mod = (await import('@/app/api/v1/wardrobe/route')) as {
        GET: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(await mod.GET(mockRequest(`${B}/wardrobe`)));
    }
    case 'wardrobeCreate': {
      const mod = (await import('@/app/api/v1/wardrobe/route')) as {
        POST: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(await mod.POST(mockRequest(`${B}/wardrobe`, 'POST', c.body)));
    }
    case 'wardrobeItemGet': {
      const mod = (await import('@/app/api/v1/wardrobe/[itemId]/route')) as {
        GET: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(
        await mod.GET(mockRequest(`${B}/wardrobe/${c.itemId}`), {
          params: Promise.resolve({ itemId: c.itemId }),
        }),
      );
    }
    case 'wardrobeUpdate': {
      const mod = (await import('@/app/api/v1/wardrobe/[itemId]/route')) as {
        PUT: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(
        await mod.PUT(mockRequest(`${B}/wardrobe/${c.itemId}`, 'PUT', c.body), {
          params: Promise.resolve({ itemId: c.itemId }),
        }),
      );
    }
    case 'wardrobeDelete': {
      const mod = (await import('@/app/api/v1/wardrobe/[itemId]/route')) as {
        DELETE: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(
        await mod.DELETE(mockRequest(`${B}/wardrobe/${c.itemId}`, 'DELETE'), {
          params: Promise.resolve({ itemId: c.itemId }),
        }),
      );
    }
    case 'transferDestinations': {
      const mod = (await import('@/app/api/v1/wardrobe/transfers/route')) as {
        GET: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(await mod.GET(mockRequest(`${B}/wardrobe/transfers`)));
    }
    case 'transferApply': {
      const mod = (await import('@/app/api/v1/wardrobe/transfers/route')) as {
        POST: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(await mod.POST(mockRequest(`${B}/wardrobe/transfers`, 'POST', c.body)));
    }
    case 'outfitGet': {
      const mod = (await import('@/app/api/v1/chats/[id]/actions/outfit')) as {
        handleGetOutfit: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(await mod.handleGetOutfit(c.chatId, await authedCtx()));
    }
    case 'equip': {
      const mod = (await import('@/app/api/v1/chats/[id]/actions/outfit')) as {
        handleEquipSlot: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(
        await mod.handleEquipSlot(
          mockRequest(`${B}/chats/${c.chatId}?action=equip`, 'POST', c.body),
          c.chatId,
          await authedCtx(),
        ),
      );
    }
    case 'regenerateAvatar': {
      const mod = (await import('@/app/api/v1/chats/[id]/actions/regenerate-avatar')) as {
        handleRegenerateAvatar: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(
        await mod.handleRegenerateAvatar(
          mockRequest(`${B}/chats/${c.chatId}?action=regenerate-avatar`, 'POST', c.body),
          c.chatId,
          await authedCtx(),
        ),
      );
    }
    case 'previewAvatar': {
      const mod = (await import('@/app/api/v1/wardrobe/preview-avatar/route')) as {
        POST: (...a: unknown[]) => Promise<unknown>;
      };
      return respond(await mod.POST(mockRequest(`${B}/wardrobe/preview-avatar`, 'POST', c.body)));
    }
    default:
      throw new Error(`unknown case kind: ${c.kind}`);
  }
}

async function runCase(
  spec: Spec,
  c: CaseEntry,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Array<Record<string, unknown>>> {
  jest.resetModules();
  applyMocks(spec);
  delete webpCapture.buffer;
  delete webpCapture.mimeType;
  delete webpCapture.filename;

  const work = mkdtempSync(join(scratch, 'wroutes-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QT_WROUTES_SPEC = JSON.stringify({ userId: spec.userId });

  const { closeDatabase, initializeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    const rows: Array<Record<string, unknown>> = [];
    const out = await runKind(c);
    rows.push({ name: c.name, status: out.status, body: out.body });

    if (c.thenOutfit) {
      const mod = (await import('@/app/api/v1/chats/[id]/actions/outfit')) as {
        handleGetOutfit: (...a: unknown[]) => Promise<unknown>;
      };
      const follow = await respond(await mod.handleGetOutfit(c.thenOutfit, await authedCtx()));
      rows.push({ name: `${c.name}__outfit`, status: follow.status, body: follow.body });
    }

    if (c.emitBytes) {
      if (!webpCapture.buffer) throw new Error(`${c.name}: emitBytes set but no capture`);
      rows.push({
        name: `${c.name}_bytes`,
        status: 200,
        body: {
          base64: Buffer.from(webpCapture.buffer).toString('base64'),
          mimeType: webpCapture.mimeType,
          filename: webpCapture.filename,
          revisedPrompt: spec.preview.revisedPrompt,
        },
      });
    }

    // Drain the fire-and-forget tails before closing (the photos lesson —
    // stray promises otherwise poison the NEXT case).
    await new Promise((r) => setTimeout(r, 250));
    return rows;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-routes.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_WROUTES_MAIN ?? '',
    mount: process.env.QT_FIXTURE_WROUTES_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wroutes-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of spec.cases) {
    const payloads = await runCase(spec, c, scratch, fixtures);
    for (const p of payloads) outLines.push(JSON.stringify(p));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `wardrobe-routes oracle wrote ${outPath} (${outLines.length} rows from ${spec.cases.length} cases)\n`,
  );
}

test('wardrobe-routes oracle', async () => {
  await main();
});
