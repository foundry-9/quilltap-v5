/**
 * @jest-environment node
 *
 * Tier-3 (mocked-engine) ORACLE for `POST /api/v1/mount-points/[id]?action=sync`
 * — v4's REAL `handleSync` (`app/api/v1/mount-points/[id]/route.ts`, added at
 * `23da0b322`).
 *
 * What is real here and what is not: the route module, its `syncSchema`, its
 * store lookup and its whole `catch` ladder are v4's own code. `syncMountPoint`
 * is mocked — as v4's own `sync-action.test.ts` mocks it — because the boundary
 * is the thing under test: the body schema (which is the single source of truth
 * for the CLI's flags, since the CLI does not re-validate), the lookup, and the
 * mapping from the engine's refusals onto HTTP status codes. A refusal the
 * operator can act on must not arrive as a 500.
 *
 * Each row records the observable outputs: the status, the response body, and —
 * for an accepted request — the OPTIONS BAG `syncMountPoint` was handed, which
 * is where the five defaults live.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * `.claude/` paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-mount-sync-action-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/mount-sync-action.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-mount-sync-action.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- mount-sync-action
 */

import fs from 'fs';

jest.mock('@/lib/logger', () => {
  const logger: Record<string, unknown> = {
    info: jest.fn(),
    warn: jest.fn(),
    error: jest.fn(),
    debug: jest.fn(),
  };
  logger.child = jest.fn(() => logger);
  return { logger };
});

// The mock functions are minted inside the factories — a `const` at module
// scope would be read before the hoisted factories run.
jest.mock('@/lib/api/middleware', () => {
  const findById = jest.fn();
  return {
    __findById: findById,
    createContextParamsHandler:
      (handler: (req: any, ctx: any, params: any) => Promise<any>) =>
      async (req: any, params: any) =>
        handler(
          req,
          { user: { id: 'user-1' }, repos: { docMountPoints: { findById } } },
          params,
        ),
  };
});

jest.mock('@/lib/api/middleware/actions', () => ({
  withActionDispatch:
    (handlers: Record<string, (req: any, ctx: any, params: any) => Promise<any>>) =>
    async (req: any, ctx: any, params: any) => {
      const action = new URL(req.url).searchParams.get('action') ?? '';
      return handlers[action](req, ctx, params);
    },
}));

// The route module reaches the filesystem watcher (and chokidar, an ESM
// package) on the way in; none of it is on the sync path.
jest.mock('@/lib/mount-index/watcher', () => ({
  detachMountPoint: jest.fn(),
  refreshMountPoint: jest.fn().mockResolvedValue(undefined),
}));

jest.mock('@/lib/mount-index/sync', () => {
  class SyncRefusedError extends Error {
    constructor(
      message: string,
      public code: string,
    ) {
      super(message);
      this.name = 'SyncRefusedError';
    }
  }
  return { syncMountPoint: jest.fn(), SyncRefusedError };
});

import { POST } from '@/app/api/v1/mount-points/[id]/route';
import { SyncRefusedError } from '@/lib/mount-index/sync';
import { ManifestMismatchError } from '@/lib/mount-index/sync/manifest';

const findByIdMock = jest.requireMock('@/lib/api/middleware').__findById as jest.Mock;
const syncMountPointMock = jest.requireMock('@/lib/mount-index/sync')
  .syncMountPoint as jest.Mock;

const STORE = {
  id: 'store-1',
  name: 'Lore',
  mountType: 'database',
  storeType: 'documents',
  conversionStatus: 'idle',
  scanStatus: 'idle',
};

const REPORT = {
  storeId: 'store-1',
  storeName: 'Lore',
  targetPath: '/tmp/lore',
  dryRun: false,
  actions: [],
  warnings: [],
  elapsedMs: 4,
  summary: {
    created: 0,
    modified: 0,
    deleted: 0,
    touched: 0,
    described: 0,
    conflicts: 0,
    skipped: 0,
    failed: 0,
  },
};

function req(body: unknown) {
  return {
    url: 'http://localhost/api/v1/mount-points/store-1?action=sync',
    json: async () => body,
  } as never;
}

const params = Promise.resolve({ id: 'store-1' }) as never;

/** The bodies the schema half is measured over. */
const BODIES: Array<{ id: string; body: unknown }> = [
  // --- accepted -----------------------------------------------------------
  { id: 'bare-targetPath-fills-every-default', body: { targetPath: '/tmp/lore' } },
  {
    id: 'every-flag-threaded-verbatim',
    body: {
      targetPath: '/tmp/lore',
      dryRun: true,
      direction: 'to-store',
      prefer: 'disk',
      propagateDeletes: false,
      useManifest: false,
    },
  },
  {
    id: 'the-other-direction-and-prefer',
    body: { targetPath: '/tmp/lore', direction: 'to-disk', prefer: 'store' },
  },
  { id: 'direction-both-explicitly', body: { targetPath: '/tmp/lore', direction: 'both' } },
  { id: 'prefer-newer-explicitly', body: { targetPath: '/tmp/lore', prefer: 'newer' } },
  {
    id: 'an-unknown-key-is-stripped-not-refused',
    body: { targetPath: '/tmp/lore', wibble: 1 },
  },
  {
    // `undefined` does not survive JSON, but the handler's `req.json()` is a
    // plain function here, so the schema's "absent and undefined agree" arm is
    // reachable.
    id: 'explicit-undefined-takes-the-default',
    body: { targetPath: '/tmp/lore', dryRun: undefined, direction: undefined },
  },

  // --- refused ------------------------------------------------------------
  { id: 'missing-targetPath', body: {} },
  { id: 'empty-targetPath', body: { targetPath: '' } },
  { id: 'targetPath-null', body: { targetPath: null } },
  { id: 'targetPath-wrong-type', body: { targetPath: 42 } },
  { id: 'body-is-not-an-object', body: [] },
  { id: 'body-is-null', body: null },
  {
    id: 'unknown-direction',
    body: { targetPath: '/tmp/lore', direction: 'sideways' },
  },
  { id: 'unknown-prefer', body: { targetPath: '/tmp/lore', prefer: 'whichever' } },
  { id: 'direction-null', body: { targetPath: '/tmp/lore', direction: null } },
  { id: 'prefer-null', body: { targetPath: '/tmp/lore', prefer: null } },
  { id: 'dryRun-null', body: { targetPath: '/tmp/lore', dryRun: null } },
  { id: 'dryRun-wrong-type', body: { targetPath: '/tmp/lore', dryRun: 'yes' } },
  {
    id: 'propagateDeletes-null',
    body: { targetPath: '/tmp/lore', propagateDeletes: null },
  },
  { id: 'useManifest-null', body: { targetPath: '/tmp/lore', useManifest: null } },
  {
    id: 'useManifest-wrong-type',
    body: { targetPath: '/tmp/lore', useManifest: 1 },
  },
  {
    // Several issues at once: the message lists them ALL, joined by '; ', in
    // the schema's key order.
    id: 'three-issues-at-once',
    body: { targetPath: '', direction: 'sideways', prefer: 'whichever' },
  },
];

/** The refusals the engine can raise, and what the ladder makes of each. */
const REFUSALS: Array<{ id: string; make: () => unknown }> = [
  {
    id: 'NOT_DATABASE_BACKED',
    make: () =>
      new SyncRefusedError('"Lore" is a filesystem store', 'NOT_DATABASE_BACKED'),
  },
  { id: 'CHARACTER_ARCHIVED', make: () => new SyncRefusedError('archived', 'CHARACTER_ARCHIVED') },
  { id: 'SYNC_IN_PROGRESS', make: () => new SyncRefusedError('already running', 'SYNC_IN_PROGRESS') },
  {
    id: 'CONVERSION_IN_PROGRESS',
    make: () => new SyncRefusedError('converting', 'CONVERSION_IN_PROGRESS'),
  },
  { id: 'SCAN_IN_PROGRESS', make: () => new SyncRefusedError('scanning', 'SCAN_IN_PROGRESS') },
  {
    // A code the ladder has never heard of falls to the `badRequest` arm, not
    // the 500 — the `SyncRefusedError` test comes first.
    id: 'an-unknown-refusal-code',
    make: () => new SyncRefusedError('something else', 'WIBBLE'),
  },
  { id: 'ManifestMismatchError', make: () => new ManifestMismatchError('other', 'store-1') },
  {
    id: 'ENOENT',
    make: () =>
      Object.assign(new Error('ENOENT: no such file or directory'), { code: 'ENOENT' }),
  },
  {
    // A non-ENOENT `fs` error takes the 500 arm, with the code carried in the
    // message rather than the sentence.
    id: 'EACCES',
    make: () => Object.assign(new Error('EACCES: permission denied'), { code: 'EACCES' }),
  },
  { id: 'a-plain-error', make: () => new Error('the disk caught fire') },
];

const rows: string[] = [];
const emit = (row: unknown) => rows.push(JSON.stringify(row));

async function readBody(res: any): Promise<unknown> {
  try {
    return await res.json();
  } catch {
    return null;
  }
}

test('mount-sync-action oracle', async () => {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  for (const { id, body } of BODIES) {
    findByIdMock.mockReset().mockResolvedValue(STORE);
    syncMountPointMock.mockReset().mockResolvedValue(REPORT);
    const res: any = await POST(req(body), params);
    emit({
      kind: 'schema',
      id,
      body: body === undefined ? null : body,
      status: res.status,
      responseBody: await readBody(res),
      called: syncMountPointMock.mock.calls.length,
      // The options bag the engine was handed, which is where the defaults are.
      options: syncMountPointMock.mock.calls.length
        ? syncMountPointMock.mock.calls[0][1]
        : null,
    });
  }

  // The store lookup runs AFTER the body parse, so a bad body on a missing
  // store is a 400 and not a 404.
  for (const [id, body] of [
    ['missing-store', { targetPath: '/tmp/lore' }],
    ['missing-store-and-bad-body', {}],
  ] as Array<[string, unknown]>) {
    findByIdMock.mockReset().mockResolvedValue(null);
    syncMountPointMock.mockReset().mockResolvedValue(REPORT);
    const res: any = await POST(req(body), params);
    emit({
      kind: 'store',
      id,
      status: res.status,
      responseBody: await readBody(res),
      called: syncMountPointMock.mock.calls.length,
    });
  }

  // The happy path's body IS the report, bare.
  findByIdMock.mockReset().mockResolvedValue(STORE);
  syncMountPointMock.mockReset().mockResolvedValue(REPORT);
  {
    const res: any = await POST(req({ targetPath: '/tmp/lore' }), params);
    emit({
      kind: 'store',
      id: 'the-report-is-the-body',
      status: res.status,
      responseBody: await readBody(res),
      called: syncMountPointMock.mock.calls.length,
    });
  }

  for (const { id, make } of REFUSALS) {
    findByIdMock.mockReset().mockResolvedValue(STORE);
    syncMountPointMock.mockReset().mockRejectedValue(make());
    const res: any = await POST(req({ targetPath: '/tmp/lore' }), params);
    emit({
      kind: 'refusal',
      id,
      status: res.status,
      responseBody: await readBody(res),
    });
  }

  fs.writeFileSync(outPath, rows.join('\n') + '\n');
  process.stderr.write(`mount-sync-action oracle wrote ${outPath} (${rows.length} rows)\n`);
});
