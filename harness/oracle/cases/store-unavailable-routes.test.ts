/**
 * @jest-environment node
 *
 * P4.23 store-unavailable ENVELOPE oracle — the character-vault arms. Drives
 * v4's REAL `GET/PUT /api/v1/characters/[id]` over a fresh copy of the
 * committed characters fixture and records status + body verbatim, so the v5
 * web-edge test (`crates/quilltap-web/tests/store_unavailable_envelope.rs`)
 * can pin both directions:
 *
 *   - `character_get_vault_absent` — the vault's `properties.json` keystone is
 *     DELETED (the REAL deleteDatabaseDocument), then the plain GET runs. The
 *     route's ownership `findById` hydrates and throws
 *     `CharacterVaultUnavailableError` OUTSIDE any local try/catch, so the
 *     middleware answers the deliberate contextful 503
 *     (`{error: 'Character vault unavailable', characterId}` —
 *     context.ts:176-205). An EQUALITY arm: v5 answers the same.
 *
 *   - `character_update_vault_corrupt` — malformed bytes are PLANTED
 *     (`'{'`, the dogfood-#47 repro), then a `{title}` PUT runs. **v4 TODAY
 *     silently accepts and CLOBBERS the bag** (finding #47 — its
 *     `readCharacterVaultProperties` collapses the parse failure to null);
 *     recorded so the v5 side can assert the DIVERGENCE both ways: v5 refuses
 *     with the vault 503 envelope, and this arm's recorded 200 goes red the
 *     moment v4 lands its own #47 fix (reclassify to a drift re-port then —
 *     the sibling pin lives in the characters-update tier-2 corpus).
 *
 * Only the seams the Rust side also neutralizes are mocked: the auth session
 * (the fixture user) and the startup gate. The DB stack is doMocked to the
 * REAL modules + the real cipher binding.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-store-unavailable-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/store-unavailable-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"               "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-store-unavailable.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- store-unavailable-routes
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
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
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
  run: () => Promise<{ status: number; body: unknown; characterId: string }>;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'characters.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CHARACTERS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CHARACTERS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-store-unavailable-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1/characters';

  /** The first two vaulted characters, ORDER BY name — the SAME deterministic
   *  pick the v5 web test makes (the fixture bakes Aria..Echo, all vaulted). */
  const pickVaulted = async (): Promise<Array<{ id: string; mp: string }>> => {
    const { rawQuery } = await import('@/lib/database/manager');
    const rows = (await rawQuery(
      "SELECT id, characterDocumentMountPointId AS mp FROM characters \
       WHERE characterDocumentMountPointId IS NOT NULL ORDER BY name LIMIT 2",
    )) as Array<{ id: string; mp: string }>;
    if (rows.length < 2) throw new Error('fixture has fewer than two vaulted characters');
    return rows;
  };

  const cases: CaseSpec[] = [
    {
      name: 'character_get_vault_absent',
      run: async () => {
        const [{ id, mp }] = await pickVaulted();
        const { deleteDatabaseDocument } = await import('@/lib/mount-index/database-store');
        await deleteDatabaseDocument(mp, 'properties.json');
        const route = (await import('@/app/api/v1/characters/[id]/route')) as never as Record<
          string,
          (...a: unknown[]) => Promise<unknown>
        >;
        const { status, body } = await respond(
          await route.GET(mockRequest(`${B}/${id}`), { params: Promise.resolve({ id }) }),
        );
        return { status, body, characterId: id };
      },
    },
    {
      name: 'character_update_vault_corrupt',
      run: async () => {
        const rows = await pickVaulted();
        const { id, mp } = rows[1];
        const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
        await writeDatabaseDocument(mp, 'properties.json', '{');
        const route = (await import('@/app/api/v1/characters/[id]/route')) as never as Record<
          string,
          (...a: unknown[]) => Promise<unknown>
        >;
        const { status, body } = await respond(
          await route.PUT(
            mockRequest(`${B}/${id}`, 'PUT', { title: 'clobber probe' }),
            { params: Promise.resolve({ id }) },
          ),
        );
        return { status, body, characterId: id };
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    jest.resetModules();
    applyMocks(spec);

    const work = mkdtempSync(join(scratch, 'su-'));
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
    try {
      const out = await c.run();
      outLines.push(JSON.stringify({ name: c.name, ...out }));
    } finally {
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(work, { recursive: true, force: true });
    }
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`store-unavailable oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('store-unavailable-routes oracle', async () => {
  await main();
});
