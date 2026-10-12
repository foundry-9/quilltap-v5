/**
 * Tier-2 oracle — the Wearable Pool LOADER over a REAL four-tier fixture
 * (P4.D279, keystone C1 §1; v4 `lib/wardrobe/pool.ts` at `4a38222e6`).
 *
 * Opens the pre-seeded main + mount-index fixtures (build-wearable-pool-fixture.ts)
 * and drives v4's REAL `loadWearablePool`, `resolveProjectTierForChat`,
 * `loadCastPools`, `componentGraph` and `repos.wardrobe.readSharedTiers`,
 * recording per op the pool's ORDERED `byId`, `wearable()`, `tiers` and the
 * wardrobe-family log lines (the singleton logger's transports spliced;
 * `LOG_LEVEL=debug` set before the logger loads so the DEBUG lines pass).
 *
 * Ops (Ada is a Guild member, rostered on ADMIT, not on EXCLUDE):
 *   explicit-project       loadWearablePool(repos, ada, [admitStore])
 *   explicit-empty-project loadWearablePool(repos, ada, [], { chatId: excludeChat }) — the chat is NOT consulted
 *   chat-admit             loadWearablePool(repos, ada, undefined, { chatId: admitChat })
 *   chat-exclude           … { chatId: excludeChat }   → the project tier WITHHELD (the DEBUG line)
 *   chat-exclude-operator  … { chatId: excludeChat, operator: true } → present
 *   no-chat                loadWearablePool(repos, ada)
 *   chat-no-project        … { chatId: noProjectChat }
 *   chat-missing           … { chatId: missingChat }   → no row → `[]`
 *   own-items-handed       loadWearablePool(repos, ada, [admitStore], { ownItems: [handed] }) — the vault is NOT read
 *   bea-no-groups          loadWearablePool(repos, bea, [admitStore])
 *   cast-pools             loadCastPools(repos, admitProject, [ada, bea]) — one shared read
 *   resolve-project-tier   resolveProjectTierForChat × every chat × operator
 *   read-shared-tiers      repos.wardrobe.readSharedTiers([general, admitStore, BOGUS], archived?, originOf)
 *   read-shared-tiers-empty readSharedTiers([], …) — no line
 *
 * Every pool op also records `componentGraph(pool, [generalEnsemble])` (the
 * cross-tier walk: General composite → GROUP sash) and `get` of the probe ids.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WPOOL_MAIN=/tmp/qt-wpool-main.db QT_FIXTURE_WPOOL_MOUNT=/tmp/qt-wpool-mount.db \
 *     $N/node --import tsx <v5>/harness/oracle/cases/wearable-pool-tier2.ts \
 *     > /tmp/oracle-wearable-pool-t2.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  adaId: string;
  beaId: string;
  groupId: string;
  groupName: string;
  admitProjectId: string;
  admitProjectName: string;
  excludeProjectId: string;
  excludeProjectName: string;
  generalMountPointId: string;
  bogusMountPointId: string;
  admitChatId: string;
  excludeChatId: string;
  noProjectChatId: string;
  missingChatId: string;
  items: Record<string, string>;
}

interface RecordedLine {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

/** The wardrobe-family lines (v4's `pool.ts` + `WardrobeRepository.readSharedTiers`). */
const CAPTURED = new Set([
  '[WearablePool] Loaded wearable pool',
  '[WearablePool] Character off project roster — project wardrobe withheld',
  '[WearablePool] Project lookup for chat failed',
  '[WearablePool] Failed to read shared wardrobe tiers; continuing without them',
  '[WearablePool] Failed to read the character wardrobe; using shared tiers alone',
  '[WearablePool] Failed to read the group wardrobe tier; skipping it',
  'Read shared wardrobe tiers',
  'Failed to read shared wardrobe tier; skipping',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'wearable-pool-tier2.json'), 'utf8')) as Spec;
  const I = spec.items;

  const mainFixture = process.env.QT_FIXTURE_WPOOL_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WPOOL_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WPOOL_MAIN and QT_FIXTURE_WPOOL_MOUNT must point at the seeded fixture .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wpool-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'wpool-main-work.db');
  const mountWork = join(scratch, 'wpool-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // DEBUG must pass the logger's level gate — set before the logger loads.
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  let lines: RecordedLine[] = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      lines.push({ level: data.level, message: data.message, fields });
    },
  });
  const takeLines = (): RecordedLine[] => {
    const out = lines;
    lines = [];
    return out;
  };

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { loadWearablePool, loadCastPools, resolveProjectTierForChat, componentGraph } = await import(
    '@/lib/wardrobe/pool'
  );
  type Pool = Awaited<ReturnType<typeof loadWearablePool>>;

  await initializeDatabase();
  const repos = getRepositories();

  // The minted stores, read back from the owner rows.
  const storeOf = async (kind: 'project' | 'group', id: string): Promise<string> => {
    const row =
      kind === 'project' ? await repos.projects.findByIdRaw(id) : await repos.groups.findByIdRaw(id);
    const mp = (row as { officialMountPointId?: string } | null)?.officialMountPointId;
    if (!mp) throw new Error(`${kind} ${id} has no official store`);
    return mp;
  };
  const admitStore = await storeOf('project', spec.admitProjectId);
  const excludeStore = await storeOf('project', spec.excludeProjectId);
  const groupStore = await storeOf('group', spec.groupId);

  const probeIds = [
    I.generalTopper,
    I.duplicateProjectGeneral,
    I.generalEnsemble,
    I.generalArchivedBreeches,
    I.projectApron,
    I.excludeVeil,
    I.groupSash,
    I.unmaskLivery,
    I.adaBoots,
    I.adaEveningGown,
    I.beaCoat,
    I.handedCloak,
    I.noSuch,
  ];

  const summarize = (pool: Pool) => ({
    characterId: pool.characterId,
    tiers: pool.tiers,
    byId: Array.from(pool.byId.values()),
    wearable: pool.wearable(),
    get: probeIds.map((id) => ({ id, item: pool.get(id) ?? null })),
    getMany: pool.getMany(probeIds),
    ensembleGraph: Array.from(componentGraph(pool, [I.generalEnsemble]).values()),
    owns: Array.from(pool.byId.values()).map((it) => ({ id: it.id, owns: pool.owns(it) })),
    findByTitle: ['Guild livery', 'my livery', 'Project badge', 'Retired breeches', 'Evening gown'].map(
      (q) => ({ query: q, item: pool.findByTitle(q) ?? null }),
    ),
  });

  const out: string[] = [];
  const emit = (row: Record<string, unknown>) => out.push(JSON.stringify(row));

  emit({
    kind: 'meta',
    baseline: 'p4.d279-wearable-pool-tier2',
    stores: { admit: admitStore, exclude: excludeStore, group: groupStore },
  });

  const poolOp = async (name: string, run: () => Promise<Pool>) => {
    takeLines();
    const pool = await run();
    emit({ kind: 'pool', name, pool: summarize(pool), lines: takeLines() });
  };

  await poolOp('explicit-project', () => loadWearablePool(repos, spec.adaId, [admitStore]));
  await poolOp('explicit-empty-project', () =>
    loadWearablePool(repos, spec.adaId, [], { chatId: spec.excludeChatId }),
  );
  await poolOp('chat-admit', () => loadWearablePool(repos, spec.adaId, undefined, { chatId: spec.admitChatId }));
  await poolOp('chat-exclude', () =>
    loadWearablePool(repos, spec.adaId, undefined, { chatId: spec.excludeChatId }),
  );
  await poolOp('chat-exclude-operator', () =>
    loadWearablePool(repos, spec.adaId, undefined, { chatId: spec.excludeChatId, operator: true }),
  );
  await poolOp('no-chat', () => loadWearablePool(repos, spec.adaId));
  await poolOp('chat-no-project', () =>
    loadWearablePool(repos, spec.adaId, undefined, { chatId: spec.noProjectChatId }),
  );
  await poolOp('chat-missing', () =>
    loadWearablePool(repos, spec.adaId, undefined, { chatId: spec.missingChatId }),
  );
  const handed = {
    id: I.handedCloak,
    characterId: null,
    title: 'Handed cloak',
    description: null,
    imagePrompt: null,
    types: ['top'],
    componentItemIds: [I.groupSash],
    appropriateness: null,
    isDefault: true,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
    folder: '',
    createdAt: spec.seedTimestamp,
    updatedAt: spec.seedTimestamp,
  };
  await poolOp('own-items-handed', () =>
    loadWearablePool(repos, spec.adaId, [admitStore], { ownItems: [handed as never] }),
  );
  await poolOp('bea-no-groups', () => loadWearablePool(repos, spec.beaId, [admitStore]));

  // The cast: one shared read for both.
  takeLines();
  const cast = await loadCastPools(repos, spec.admitProjectId, [spec.adaId, spec.beaId]);
  emit({
    kind: 'cast',
    name: 'cast-pools',
    pools: Array.from(cast.entries()).map(([id, pool]) => ({ characterId: id, pool: summarize(pool) })),
    lines: takeLines(),
  });

  // resolveProjectTierForChat — every chat, both roles.
  const chats: Array<[string, string | null]> = [
    ['admit', spec.admitChatId],
    ['exclude', spec.excludeChatId],
    ['no-project', spec.noProjectChatId],
    ['missing', spec.missingChatId],
    ['null', null],
    ['empty', ''],
  ];
  for (const [label, chatId] of chats) {
    for (const operator of [false, true]) {
      takeLines();
      const ids = await resolveProjectTierForChat(repos, chatId, spec.adaId, operator ? { operator: true } : {});
      emit({
        kind: 'project_tier',
        name: `resolve-project-tier-${label}${operator ? '-operator' : ''}`,
        chatId,
        operator,
        projectMountPointIds: ids,
        lines: takeLines(),
      });
    }
  }

  // readSharedTiers — a BOGUS mount in the list, both archived flags.
  const originOf = (mp: string) =>
    mp === spec.generalMountPointId
      ? { scope: 'general' as const, id: null, name: 'Quilltap General' }
      : mp === admitStore
        ? { scope: 'project' as const, id: spec.admitProjectId, name: spec.admitProjectName }
        : { scope: 'project' as const, id: 'bogus', name: 'Bogus' };
  for (const includeArchived of [true, false]) {
    takeLines();
    const items = await repos.wardrobe.readSharedTiers(
      [spec.generalMountPointId, admitStore, spec.bogusMountPointId],
      includeArchived,
      originOf,
    );
    emit({
      kind: 'shared_tiers',
      name: `read-shared-tiers-${includeArchived ? 'archived' : 'live'}`,
      mountPointIds: [spec.generalMountPointId, admitStore, spec.bogusMountPointId],
      includeArchived,
      items,
      lines: takeLines(),
    });
  }
  takeLines();
  const none = await repos.wardrobe.readSharedTiers([], true, originOf);
  emit({
    kind: 'shared_tiers',
    name: 'read-shared-tiers-empty',
    mountPointIds: [],
    includeArchived: true,
    items: none,
    lines: takeLines(),
  });

  await closeDatabase();
  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
