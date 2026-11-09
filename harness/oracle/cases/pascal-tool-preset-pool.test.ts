/**
 * @jest-environment node
 *
 * P4.D379 ORACLE: Pascal custom-tool RUN PRESETS — the tier walk. Drives v4's
 * REAL `resolveToolPresetPool` (`lib/pascal/tool-preset-pool.ts`, `fd165fce0`
 * as re-optioned by `0c47f4539`, at the `624d0d25c` pin).
 *
 * ## Why this mocks the pool + the store reads
 *
 * v4's own `__tests__/unit/lib/pascal/tool-preset-pool.test.ts` mocks
 * `resolveTieredMountPool`, `getRepositories` (`docMountPoints.findById`) and
 * `listDatabaseFiles` — the pool resolver is proven elsewhere; what is under
 * test here is the walk: the order, the silent skip of a vanished mount,
 * `stores` pushed before the listing, the WARN + `unread` on a listing failure,
 * the parse / dedup / shadows, the `localeCompare` sort and the DEBUG. This
 * oracle uses the same mocks, so v4's five cases are its spine; the REAL
 * `listToolsFolderFiles`, `orderMounts` (`reach.ts`, loaded real) and
 * `parsePresetFromPath` run beneath it.
 *
 * Each row: `{ id, scenario: { pool, mounts, broken }, ctx, result, logs,
 * poolCallOptions }`. `mounts` maps a mount id to its listed paths (an id
 * absent from `mounts` is a vanished row); `broken` maps a mount id to the
 * message its listing throws. `logs` are the two pool lines, captured through
 * the singleton logger's transports (memory note
 * `v4-oracle-job-host-and-log-capture`), `service` / `environment` dropped.
 *
 * Run (v4 @ the `624d0d25c` pin, Node 24; cp to a /tmp mirror, jest ignores
 * .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-pascal-preset-pool-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/pascal-tool-preset-pool.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server     # (the pin: /tmp/qt-v4-pin-p4d379-624d0d25c)
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-tool-preset-pool.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- pascal-tool-preset-pool
 */

import * as fs from 'fs';

jest.mock('@/lib/repositories/factory', () => ({ getRepositories: jest.fn() }));
jest.mock('@/lib/mount-index/tiered-mount-pool', () => ({ resolveTieredMountPool: jest.fn() }));
jest.mock('@/lib/mount-index/database-store', () => ({
  listDatabaseFiles: jest.fn(),
  DatabaseStoreError: class DatabaseStoreError extends Error {},
}));
jest.mock('@/lib/mount-index/read-file', () => ({ readMountFileBytes: jest.fn() }));

import { resolveToolPresetPool } from '@/lib/pascal/tool-preset-pool';
import { getRepositories } from '@/lib/repositories/factory';
import { resolveTieredMountPool } from '@/lib/mount-index/tiered-mount-pool';
import { listDatabaseFiles } from '@/lib/mount-index/database-store';
import { logger } from '@/lib/logger';

const mockGetRepositories = getRepositories as jest.Mock;
const mockResolvePool = resolveTieredMountPool as jest.Mock;
const mockListDatabaseFiles = listDatabaseFiles as jest.Mock;

const OUT = process.env.QT_ORACLE_OUT;
const rows: unknown[] = [];

const PINNED = new Set(['Preset listing failed for store', 'Tool presets resolved']);
let lines: Array<{ level: string; message: string; bag: Record<string, unknown> }> = [];
function captureLines(): void {
  const internal = logger as unknown as { minLevel: number; transports: Array<{ write: (d: unknown) => void }> };
  internal.minLevel = Number.MAX_SAFE_INTEGER;
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!PINNED.has(data.message)) return;
      const { service: _s, environment: _e, ...bag } = (data.context ?? {}) as Record<string, unknown>;
      lines.push({ level: data.level, message: data.message, bag: JSON.parse(JSON.stringify(bag)) });
    },
  });
}

interface Pool {
  characterMountPointId?: string | null;
  groupMountPointIds?: string[];
  projectMountPointIds?: string[];
  globalMountPointId?: string | null;
}
interface Scenario {
  id: string;
  pool: Pool;
  /** mount id → listed paths. An id absent here is a vanished row. */
  mounts: Record<string, string[]>;
  /** mount id → the message its listing throws. */
  broken?: Record<string, string>;
  ctx?: Partial<{ characterId: string | null; characterMountPointId: string | null; toolName: string }>;
}

function prime(s: Scenario): void {
  mockGetRepositories.mockReturnValue({
    docMountPoints: {
      findById: jest.fn(async (id: string) =>
        s.mounts[id] ? { id, name: `store-${id}`, enabled: true, mountType: 'database', basePath: '' } : null,
      ),
    },
  });
  mockListDatabaseFiles.mockImplementation(async (mountId: string) => {
    if (s.broken?.[mountId] !== undefined) throw new Error(s.broken[mountId]);
    return (s.mounts[mountId] ?? []).map((relativePath) => ({ kind: 'file', relativePath }));
  });
  mockResolvePool.mockResolvedValue({
    characterMountPointId: null,
    participantMountPointIds: [],
    groupMountPointIds: [],
    projectMountPointIds: [],
    globalMountPointId: null,
    ...s.pool,
  });
}

const baseCtx = { userId: 'u1', characterId: 'c1', characterMountPointId: 'vault', projectId: 'p1', toolName: 'unlock' };
const p = (name: string, tool = 'unlock') => `Tools/${tool}.${name}.settings.json`;

const scenarios: Scenario[] = [
  // ---- v4's own five (`tool-preset-pool.test.ts`) ----
  {
    id: 'v4-every-tier-nearest-wins',
    pool: { characterMountPointId: 'vault', groupMountPointIds: ['group'], projectMountPointIds: ['project'], globalMountPointId: 'general' },
    mounts: {
      vault: [p('aggressive')],
      group: [p('aggressive'), p('careful')],
      project: [p('quiet')],
      general: [p('careful'), p('house')],
    },
  },
  {
    id: 'v4-every-enabled-store-a-destination',
    pool: { characterMountPointId: 'vault', groupMountPointIds: ['group'], projectMountPointIds: [], globalMountPointId: 'general' },
    mounts: { vault: [], group: [], general: [] },
  },
  {
    id: 'v4-ignores-others-definitions-nested-malformed',
    pool: { characterMountPointId: 'vault' },
    mounts: {
      vault: [
        'Tools/unlock.tool.json',
        'Tools/lockpick.aggressive.settings.json',
        'Tools/old/unlock.archived.settings.json',
        'Tools/unlock.Bad Name.settings.json',
        'Tools/unlock.fine.settings.json',
      ],
    },
  },
  {
    id: 'v4-no-vault-still-reads-shared-tiers',
    pool: { globalMountPointId: 'general' },
    mounts: { general: [p('house')] },
    ctx: { characterMountPointId: null },
  },
  {
    id: 'v4-broken-store-reported',
    pool: { groupMountPointIds: ['broken'], globalMountPointId: 'general' },
    mounts: { broken: [], general: [p('house')] },
    broken: { broken: 'index unavailable' },
  },
  // ---- the walk's edges ----
  {
    id: 'vanished-mount-skipped-silently',
    pool: { characterMountPointId: 'vault', groupMountPointIds: ['ghost'], globalMountPointId: 'general' },
    mounts: { vault: [p('mine')], general: [p('house')] },
  },
  {
    id: 'broken-near-store-far-name-wins',
    pool: { characterMountPointId: 'vault', groupMountPointIds: ['broken'], globalMountPointId: 'general' },
    mounts: { vault: [], broken: [p('aggressive')], general: [p('aggressive')] },
    broken: { broken: 'Database store unavailable' },
  },
  {
    id: 'three-tiers-one-name-two-shadows',
    pool: { characterMountPointId: 'vault', groupMountPointIds: ['g1', 'g2'], projectMountPointIds: ['proj'], globalMountPointId: 'general' },
    mounts: { vault: [p('x')], g1: [], g2: [p('x')], proj: [p('y')], general: [p('x'), p('y')] },
  },
  {
    id: 'collation-order',
    pool: { characterMountPointId: 'vault' },
    mounts: { vault: [p('ab'), p('a1'), p('a-b'), p('0x'), p('a_b'), p('a'), p('b'), p('a-a'), p('a_a'), p('9')] },
  },
  {
    id: 'duplicate-mount-across-tiers',
    pool: { groupMountPointIds: ['m'], projectMountPointIds: ['m', 'p'], globalMountPointId: 'm' },
    mounts: { m: [p('x')], p: [p('x'), p('y')] },
  },
  {
    id: 'same-tier-pool-order-no-sort',
    pool: { projectMountPointIds: ['zzz', 'aaa'] },
    mounts: { zzz: [p('x')], aaa: [p('x')] },
  },
  { id: 'empty-pool', pool: {}, mounts: {} },
  {
    id: 'character-id-null',
    pool: { globalMountPointId: 'general' },
    mounts: { general: [p('house')] },
    ctx: { characterId: null, characterMountPointId: null },
  },
  {
    id: 'two-broken-stores',
    pool: { characterMountPointId: 'vault', projectMountPointIds: ['b1', 'b2'] },
    mounts: { vault: [p('one')], b1: [], b2: [] },
    broken: { b1: 'first failure', b2: 'second failure' },
  },
  {
    id: 'other-tool-name',
    pool: { characterMountPointId: 'vault' },
    mounts: { vault: [p('fine', 'coin'), p('fine'), 'Tools/coin.settings.json'] },
    ctx: { toolName: 'coin' },
  },
];

describe('preset pool oracle', () => {
  it('emits', async () => {
    captureLines();
    for (const s of scenarios) {
      jest.clearAllMocks();
      lines = [];
      prime(s);
      const ctx = { ...baseCtx, ...(s.ctx ?? {}) };
      const result = await resolveToolPresetPool(ctx);
      rows.push({
        id: s.id,
        scenario: { pool: s.pool, mounts: s.mounts, broken: s.broken ?? {} },
        ctx,
        result: JSON.parse(JSON.stringify(result)),
        logs: lines,
        poolCallOptions: mockResolvePool.mock.calls[0]?.[1] ?? null,
      });
    }
  });
});

afterAll(() => {
  if (!OUT) throw new Error('set QT_ORACLE_OUT');
  fs.writeFileSync(OUT, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
});
