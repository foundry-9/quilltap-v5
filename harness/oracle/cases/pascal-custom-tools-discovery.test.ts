/**
 * @jest-environment node
 *
 * P4.6ay unit 2 ORACLE: Pascal custom-tool DISCOVERY / tier shadowing / roster.
 * Drives v4's REAL `resolveCustomToolRoster` (`lib/pascal/custom-tools.ts`).
 *
 * ## Why this mocks the pool + store, and emits a scenario corpus
 *
 * v4's own `__tests__/unit/lib/pascal/custom-tools-discovery.test.ts` mocks
 * `resolveTieredMountPool`, `getRepositories`, and the two database-store reads
 * — because none of those is what discovery is under test for. The tiered pool
 * resolver is already differentially proven elsewhere; the NEW logic here is
 * `isRootToolFile`, the load/parse/dedup sequence, tier shadowing, the `disabled`
 * tombstone, and the `MAX_ROSTER_SIZE` cap. So this oracle follows that template:
 * each row is a scenario `{ pool, mounts }` fed through the real function with the
 * same mocks, and the v5 side replays the scenario through
 * `resolve_roster_from_pool` + `load_definitions` (the same real parse path, via
 * the same `safe_parse`). The DB/disk IO wrappers (`load_tools_from_mount`,
 * `list_tool_files_from_disk`) are thin and are what v4 mocks here too.
 *
 * `content` in a mount's file list is EXACTLY what `readDatabaseDocument` returns
 * (a string), so both sides parse the same bytes — a malformed-JSON case ships
 * the broken text verbatim.
 *
 * Run (v4 @ d68638b4, Node 24; cp to a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-pascal-discovery-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/pascal-custom-tools-discovery.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-discovery.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- pascal-custom-tools-discovery
 */

import * as fs from 'fs';

jest.mock('@/lib/repositories/factory', () => ({ getRepositories: jest.fn() }));
jest.mock('@/lib/mount-index/tiered-mount-pool', () => ({ resolveTieredMountPool: jest.fn() }));
jest.mock('@/lib/mount-index/database-store', () => ({
  listDatabaseFiles: jest.fn(),
  readDatabaseDocument: jest.fn(),
  DatabaseStoreError: class DatabaseStoreError extends Error {
    code: string;
    constructor(message: string, code: string) {
      super(message);
      this.code = code;
    }
  },
}));

import { resolveCustomToolRoster } from '@/lib/pascal/custom-tools';
import { MAX_ROSTER_SIZE } from '@/lib/pascal/custom-tool.types';
import { getRepositories } from '@/lib/repositories/factory';
import { resolveTieredMountPool } from '@/lib/mount-index/tiered-mount-pool';
import { listDatabaseFiles, readDatabaseDocument } from '@/lib/mount-index/database-store';

const mockGetRepositories = getRepositories as jest.Mock;
const mockResolvePool = resolveTieredMountPool as jest.Mock;
const mockListDatabaseFiles = listDatabaseFiles as jest.Mock;
const mockReadDatabaseDocument = readDatabaseDocument as jest.Mock;

const OUT = process.env.QT_ORACLE_OUT;
const rows: unknown[] = [];

/** A mount is a store id → { enabled, files: [{ relativePath, kind, content }] }. */
interface MountSpec {
  enabled: boolean;
  files: Array<{ relativePath: string; kind: 'file' | 'folder'; content: string }>;
}
type Mounts = Record<string, MountSpec>;
interface Pool {
  characterMountPointId?: string | null;
  participantMountPointIds?: string[];
  groupMountPointIds?: string[];
  projectMountPointIds?: string[];
  globalMountPointId?: string | null;
}

function tool(name: string, extra: Record<string, unknown> = {}) {
  return { name, description: `The ${name} tool.`, outcomes: [{ when: true, message: 'done', state: 'info' }], ...extra };
}

/** A well-formed tool file entry, content pre-stringified as the store returns it. */
function file(relativePath: string, doc: unknown, kind: 'file' | 'folder' = 'file') {
  return { relativePath, kind, content: typeof doc === 'string' ? doc : JSON.stringify(doc) };
}

function prime(pool: Pool, mounts: Mounts) {
  mockResolvePool.mockResolvedValue({
    characterMountPointId: pool.characterMountPointId ?? null,
    participantMountPointIds: pool.participantMountPointIds ?? [],
    groupMountPointIds: pool.groupMountPointIds ?? [],
    projectMountPointIds: pool.projectMountPointIds ?? [],
    globalMountPointId: pool.globalMountPointId ?? null,
  });
  mockGetRepositories.mockReturnValue({
    docMountPoints: {
      findById: jest.fn(async (id: string) =>
        mounts[id] ? { id, name: `store-${id}`, enabled: mounts[id].enabled, mountType: 'database', basePath: '' } : null
      ),
    },
  });
  mockListDatabaseFiles.mockImplementation(async (mountId: string) =>
    (mounts[mountId]?.files ?? []).map((f) => ({
      kind: f.kind,
      relativePath: f.relativePath,
      fileName: f.relativePath.split('/').pop(),
    }))
  );
  mockReadDatabaseDocument.mockImplementation(async (mountId: string, relativePath: string) => {
    const f = (mounts[mountId]?.files ?? []).find((x) => x.relativePath === relativePath);
    if (!f) throw new Error(`no such file: ${mountId}/${relativePath}`);
    return { content: f.content };
  });
}

const ctx = { userId: 'u1', chatId: 'c1', characterId: 'char1' };

// -------------------------------------------------------------- the corpus
const gappy = {
  name: 'gappy',
  description: 'Has a coverage gap.',
  outcomes: [{ when: { gt: 0.5 }, message: 'x', state: 'info' }],
};
const capFiles: MountSpec['files'] = [];
for (let i = 0; i < MAX_ROSTER_SIZE + 3; i++) {
  const n = `t${String(i).padStart(3, '0')}`;
  capFiles.push(file(`Tools/${n}.tool.json`, tool(n)));
}

const scenarios: Array<{ id: string; pool: Pool; mounts: Mounts }> = [
  {
    id: 'finds-in-database-store',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock'))] } },
  },
  {
    id: 'keys-by-name-not-filename',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/lockpicking.tool.json', tool('unlock'))] } },
  },
  {
    id: 'ignores-nested-file',
    pool: { projectMountPointIds: ['m1'] },
    mounts: {
      m1: { enabled: true, files: [file('Tools/live.tool.json', tool('live')), file('Tools/archive/old.tool.json', tool('old'))] },
    },
  },
  {
    id: 'ignores-non-suffix',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/notes.md', tool('notes'))] } },
  },
  {
    id: 'ignores-folder-entry',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/sub.tool.json', tool('sub'), 'folder'), file('Tools/real.tool.json', tool('real'))] } },
  },
  {
    id: 'skips-disabled-mount',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: false, files: [file('Tools/x.tool.json', tool('x'))] } },
  },
  { id: 'empty-pool', pool: {}, mounts: {} },
  {
    id: 'pool-references-missing-mount',
    pool: { projectMountPointIds: ['ghost'] },
    mounts: {},
  },
  {
    id: 'malformed-json',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/broken.tool.json', '{ not json at all'), file('Tools/good.tool.json', tool('good'))] } },
  },
  {
    id: 'schema-violation',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/gappy.tool.json', gappy)] } },
  },
  {
    id: 'duplicate-name-same-store',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/a.tool.json', tool('unlock')), file('Tools/b.tool.json', tool('unlock'))] } },
  },
  {
    id: 'nearer-tier-wins',
    pool: { characterMountPointId: 'charMount', projectMountPointIds: ['projMount'] },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { description: "The locksmith's own." }))] },
      projMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { description: 'The house rule.' }))] },
    },
  },
  {
    id: 'full-precedence-chain',
    pool: { characterMountPointId: 'c', participantMountPointIds: ['p'], groupMountPointIds: ['g'], projectMountPointIds: ['j'], globalMountPointId: 'gl' },
    mounts: {
      c: { enabled: true, files: [file('Tools/a.tool.json', tool('a'))] },
      p: { enabled: true, files: [file('Tools/a.tool.json', tool('a')), file('Tools/b.tool.json', tool('b'))] },
      g: { enabled: true, files: [file('Tools/b.tool.json', tool('b')), file('Tools/c.tool.json', tool('c'))] },
      j: { enabled: true, files: [file('Tools/c.tool.json', tool('c')), file('Tools/d.tool.json', tool('d'))] },
      gl: { enabled: true, files: [file('Tools/d.tool.json', tool('d')), file('Tools/e.tool.json', tool('e'))] },
    },
  },
  {
    id: 'disabled-suppresses-inherited',
    pool: { characterMountPointId: 'charMount', projectMountPointIds: ['projMount'] },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { disabled: true }))] },
      projMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock'))] },
    },
  },
  {
    id: 'farther-disabled-does-not-remove-nearer',
    pool: { characterMountPointId: 'charMount', projectMountPointIds: ['projMount'] },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock'))] },
      projMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { disabled: true }))] },
    },
  },
  {
    id: 'disabled-leaves-others',
    pool: { characterMountPointId: 'charMount', projectMountPointIds: ['projMount'] },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { disabled: true }))] },
      projMount: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock')), file('Tools/listen.tool.json', tool('listen'))] },
    },
  },
  {
    id: 'same-tier-tie-order-zzz-aaa',
    pool: { projectMountPointIds: ['zzz', 'aaa'] },
    mounts: {
      aaa: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { description: 'from aaa' }))] },
      zzz: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { description: 'from zzz' }))] },
    },
  },
  {
    id: 'same-tier-tie-order-aaa-zzz',
    pool: { projectMountPointIds: ['aaa', 'zzz'] },
    mounts: {
      aaa: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { description: 'from aaa' }))] },
      zzz: { enabled: true, files: [file('Tools/unlock.tool.json', tool('unlock', { description: 'from zzz' }))] },
    },
  },
  {
    id: 'metadata-definition-loads',
    pool: { projectMountPointIds: ['m1'] },
    mounts: {
      m1: {
        enabled: true,
        files: [
          file('Tools/gate.tool.json', {
            name: 'gate',
            description: 'A metadata-gated tool.',
            outcomes: [
              { when: { metadata: { clearance: { gte: 3 } } }, message: 'granted', state: 'success' },
              { when: true, message: 'denied', state: 'failure' },
            ],
          }),
        ],
      },
    },
  },
  { id: 'roster-cap', pool: { projectMountPointIds: ['m1'] }, mounts: { m1: { enabled: true, files: capFiles } } },
];

function serializeRoster(roster: Awaited<ReturnType<typeof resolveCustomToolRoster>>) {
  const tools: Record<string, unknown> = {};
  for (const [name, t] of roster.tools) {
    tools[name] = {
      tier: t.tier,
      definitionPath: t.definitionPath,
      mountPointId: t.mountPointId,
      mountName: t.mountName,
      description: t.definition.description,
    };
  }
  return {
    toolKeys: [...roster.tools.keys()],
    tools,
    errors: roster.errors,
    droppedForCap: roster.droppedForCap,
  };
}

describe('discovery oracle', () => {
  it('emits', async () => {
    for (const scenario of scenarios) {
      jest.clearAllMocks();
      prime(scenario.pool, scenario.mounts);
      const roster = await resolveCustomToolRoster(ctx);
      rows.push({ kind: 'roster', id: scenario.id, input: { pool: scenario.pool, mounts: scenario.mounts }, output: serializeRoster(roster) });
    }
  });
});

afterAll(() => {
  if (!OUT) throw new Error('set QT_ORACLE_OUT');
  fs.writeFileSync(OUT, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
});
