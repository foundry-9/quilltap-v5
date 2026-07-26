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

/** The sheet `characters.findById` hands back on a lazy read, and how often. */
interface SheetSpec {
  /** Passed on the RosterContext — v4's truthy short-circuit. */
  ctxMetadata?: Record<string, unknown> | null;
  /** What the vault read returns when the roster has to go looking. */
  vaultMetadata?: Record<string, unknown> | null;
  /** `findById` rejects — the gate treats the sheet as empty. */
  vaultThrows?: boolean;
  /** No `characterId` on the context at all. */
  noCharacter?: boolean;
}

/** Counts `characters.findById` calls, so laziness is a claim and not a hope. */
let sheetReads = 0;

function prime(pool: Pool, mounts: Mounts, sheet: SheetSpec = {}) {
  mockResolvePool.mockResolvedValue({
    characterMountPointId: pool.characterMountPointId ?? null,
    participantMountPointIds: pool.participantMountPointIds ?? [],
    groupMountPointIds: pool.groupMountPointIds ?? [],
    projectMountPointIds: pool.projectMountPointIds ?? [],
    globalMountPointId: pool.globalMountPointId ?? null,
  });
  sheetReads = 0;
  mockGetRepositories.mockReturnValue({
    docMountPoints: {
      findById: jest.fn(async (id: string) =>
        mounts[id] ? { id, name: `store-${id}`, enabled: mounts[id].enabled, mountType: 'database', basePath: '' } : null
      ),
    },
    // The lazy invoker-sheet read. `loadInvokerMetadata` reaches for this ONLY
    // when a gated definition turns up and the context carried no sheet.
    characters: {
      findById: jest.fn(async () => {
        sheetReads += 1;
        if (sheet.vaultThrows) throw new Error('that vault is unreachable');
        return sheet.vaultMetadata === undefined ? null : { id: 'char1', metadata: sheet.vaultMetadata };
      }),
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

/** A gated definition, for the P4.d19 scenarios. */
function gated(name: string, clause: 'availableWhen' | 'withheldWhen', metadata: Record<string, unknown>, extra: Record<string, unknown> = {}) {
  return tool(name, { [clause]: { metadata }, ...extra });
}

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

const scenarios: Array<{ id: string; pool: Pool; mounts: Mounts; sheet?: SheetSpec }> = [
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

  // ---- the P4.d19 availability gates (v4 6864bf0e) -----------------------
  {
    id: 'gate-available-holds-deals',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { toolAbilities: { contains: 'programmable' } }))] } },
    sheet: { ctxMetadata: { toolAbilities: 'programmable, ambulatory' } },
  },
  {
    id: 'gate-available-misses-withholds',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { toolAbilities: { contains: 'programmable' } }))] } },
    sheet: { ctxMetadata: { toolAbilities: 'ambulatory' } },
  },
  {
    id: 'gate-empty-sheet-fails-closed',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { toolAbilities: { contains: 'programmable' } }))] } },
    sheet: { ctxMetadata: {} },
  },
  {
    id: 'gate-empty-sheet-withheld-when-offers',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'withheldWhen', { novice: { eq: true } }))] } },
    sheet: { ctxMetadata: {} },
  },
  {
    id: 'gate-withheld-when-holds',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'withheldWhen', { novice: { eq: true } }))] } },
    sheet: { ctxMetadata: { novice: true } },
  },
  {
    id: 'gate-key-holds-an-array',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { clearance: { gte: 3 } }))] } },
    sheet: { ctxMetadata: { clearance: [3, 4] } },
  },
  {
    id: 'gate-key-holds-null',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'withheldWhen', { clearance: { gte: 3 } }))] } },
    sheet: { ctxMetadata: { clearance: null } },
  },
  // THE ordering claim: the gate runs BEFORE `disabled`, so a gated-out
  // definition makes no claim on its name and a farther tier still deals one.
  {
    id: 'gate-before-disabled-farther-tier-still-deals',
    pool: { characterMountPointId: 'charMount', globalMountPointId: 'gl' },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { toolAbilities: { contains: 'programmable' } }, { description: "The android's own." }))] },
      gl: { enabled: true, files: [file('Tools/hack.tool.json', tool('hack', { description: 'The plain one everybody else gets.' }))] },
    },
    sheet: { ctxMetadata: { toolAbilities: 'ambulatory' } },
  },
  {
    id: 'gate-before-disabled-nearer-variant-wins-when-it-holds',
    pool: { characterMountPointId: 'charMount', globalMountPointId: 'gl' },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { toolAbilities: { contains: 'programmable' } }, { description: "The android's own." }))] },
      gl: { enabled: true, files: [file('Tools/hack.tool.json', tool('hack', { description: 'The plain one everybody else gets.' }))] },
    },
    sheet: { ctxMetadata: { toolAbilities: 'programmable' } },
  },
  // A gated TOMBSTONE is both keys at once: the gate answers first, so a sheet
  // that fails the gate never reaches `disabled` and the farther tier survives.
  {
    id: 'gate-tombstone-gated-out-does-not-suppress',
    pool: { characterMountPointId: 'charMount', globalMountPointId: 'gl' },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'withheldWhen', { novice: { eq: true } }, { disabled: true }))] },
      gl: { enabled: true, files: [file('Tools/hack.tool.json', tool('hack'))] },
    },
    sheet: { ctxMetadata: { novice: true } },
  },
  {
    id: 'gate-tombstone-gate-passes-then-suppresses',
    pool: { characterMountPointId: 'charMount', globalMountPointId: 'gl' },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'withheldWhen', { novice: { eq: true } }, { disabled: true }))] },
      gl: { enabled: true, files: [file('Tools/hack.tool.json', tool('hack'))] },
    },
    sheet: { ctxMetadata: { novice: false } },
  },
  // ---- the LAZY sheet read: only when a gated definition turns up --------
  {
    id: 'gate-lazy-read-when-context-has-no-sheet',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { clearance: { gte: 3 } }))] } },
    sheet: { vaultMetadata: { clearance: 5 } },
  },
  {
    id: 'gate-lazy-read-happens-once-across-tiers',
    pool: { characterMountPointId: 'charMount', globalMountPointId: 'gl' },
    mounts: {
      charMount: { enabled: true, files: [file('Tools/a.tool.json', gated('a', 'availableWhen', { clearance: { gte: 3 } }))] },
      gl: { enabled: true, files: [file('Tools/b.tool.json', gated('b', 'availableWhen', { clearance: { gte: 9 } }))] },
    },
    sheet: { vaultMetadata: { clearance: 5 } },
  },
  {
    id: 'no-gate-means-no-sheet-read',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/plain.tool.json', tool('plain'))] } },
    sheet: { vaultMetadata: { clearance: 5 } },
  },
  {
    id: 'gate-vault-read-fails-treated-as-empty',
    pool: { projectMountPointIds: ['m1'] },
    mounts: {
      m1: { enabled: true, files: [
        file('Tools/hack.tool.json', gated('hack', 'availableWhen', { clearance: { gte: 3 } })),
        file('Tools/listen.tool.json', gated('listen', 'withheldWhen', { novice: { eq: true } })),
      ] },
    },
    sheet: { vaultThrows: true },
  },
  {
    id: 'gate-character-absent-from-vault-treated-as-empty',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { clearance: { gte: 3 } }))] } },
    sheet: {},
  },
  {
    id: 'gate-no-character-id-treated-as-empty',
    pool: { projectMountPointIds: ['m1'] },
    mounts: {
      m1: { enabled: true, files: [
        file('Tools/hack.tool.json', gated('hack', 'availableWhen', { clearance: { gte: 3 } })),
        file('Tools/listen.tool.json', gated('listen', 'withheldWhen', { novice: { eq: true } })),
      ] },
    },
    sheet: { noCharacter: true },
  },
  {
    id: 'gate-context-sheet-beats-the-vault',
    pool: { projectMountPointIds: ['m1'] },
    mounts: { m1: { enabled: true, files: [file('Tools/hack.tool.json', gated('hack', 'availableWhen', { clearance: { gte: 3 } }))] } },
    sheet: { ctxMetadata: { clearance: 5 }, vaultMetadata: { clearance: 0 } },
  },
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
      const sheet = scenario.sheet ?? {};
      prime(scenario.pool, scenario.mounts, sheet);
      const scenarioCtx = {
        ...ctx,
        ...(sheet.noCharacter ? { characterId: undefined } : {}),
        ...('ctxMetadata' in sheet ? { metadata: sheet.ctxMetadata } : {}),
      };
      const roster = await resolveCustomToolRoster(scenarioCtx);
      rows.push({
        kind: 'roster',
        id: scenario.id,
        input: { pool: scenario.pool, mounts: scenario.mounts, sheet },
        output: serializeRoster(roster),
        // The lazy-read claim: no vault read unless a gated definition turns up,
        // and never more than one across every tier.
        sheetReads,
      });
    }
  });
});

afterAll(() => {
  if (!OUT) throw new Error('set QT_ORACLE_OUT');
  fs.writeFileSync(OUT, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
});
