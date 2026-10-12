/**
 * Tier-2 oracle — WARDROBE LOCATIONS (`lib/wardrobe/location.ts`, P4.D279 C1 §2).
 *
 * Opens COPIES of the keystone fixture pair and drives v4's REAL
 * `resolveWardrobeLocation(scope, id, repos, userId, { ensure })` over the four
 * scopes × ensure on/off, plus the location's read / folder / writable-mount
 * operations, `groupLocationsForCharacter` and `locationKey`. Every probe records
 * the location's `{ scope, id, mountPointId, characterId, origin }` (or `null`,
 * or the thrown message), what it read, the `doc_mount_points` row count (the
 * R-C "a probe never provisions" proof) and the captured
 * `[WardrobeLocation] Resolved wardrobe location` lines — v4 logs one per
 * resolve that RETURNS; a throw escapes before it.
 *
 * The real-repository shape of the R-C probe: a project whose
 * `officialMountPointId` is NULL makes `repos.projects.findById` THROW
 * `ProjectStoreUnavailableError` (the overlay has no store to hydrate from), so
 * BOTH ensure arms throw and nothing is provisioned — the unit test's
 * `officialMountPointId: null` owner is unreachable through real repos. The
 * ensure half is therefore proven on Bare Manor: a provisioned store with NO
 * `Wardrobe/` folder — `ensure:false` finds it and reads `[]` without writing,
 * `ensure:true` creates exactly the folder.
 *
 * Minted mount ids are replaced by owner-derived tokens (`<ada.mount>`, …) so
 * the Rust twin compares them by relationship.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-location-tier2.ts \
 *     > /tmp/oracle-wardrobe-location.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userAId: string;
  userBId: string;
  characterAId: string;
  characterBId: string;
  archivedCharacterId: string;
  generalMountPointId: string;
  bareProjectId: string;
  nullProjectId: string;
  fullProjectId: string;
  groupId: string;
  missingId: string;
  items: Record<string, { id: string; title: string; types: string[]; folder: string }>;
}

/** The messages this family pins (v4's exact wording). */
const PINNED_MESSAGES = ['[WardrobeLocation] Resolved wardrobe location', '[SharedWardrobe] Failed to ensure Wardrobe folder'];

function renderValue(v: unknown, remap: (s: string) => string): string {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'string') return remap(v);
  if (typeof v === 'boolean' || typeof v === 'number') return String(v);
  return remap(JSON.stringify(v));
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-keystone-tier2.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_WKS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WKS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WKS_MAIN and QT_FIXTURE_WKS_MOUNT must point at the seeded fixture .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wks-location-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'wks-main-work.db');
  const mountWork = join(scratch, 'wks-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // The pinned lines are DEBUG; the logger's level is fixed at import.
  process.env.LOG_LEVEL = 'debug';

  // Capture v4's logger lines (splice the transports; the level gate is upstream).
  const { logger } = await import('@/lib/logger');
  let captured: string[] = [];
  const tokens = new Map<string, string>();
  const remap = (s: string): string => {
    let out = s;
    for (const [minted, token] of tokens) out = out.split(minted).join(token);
    return out;
  };
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!PINNED_MESSAGES.includes(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      const rendered = Object.entries(fields)
        .map(([k, v]) => ` ${k}=${renderValue(v, remap)}`)
        .join('');
      captured.push(`${data.level.toUpperCase()} ${data.message}${rendered}`);
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { resolveWardrobeLocation, groupLocationsForCharacter, locationKey } = await import(
    '@/lib/wardrobe/location'
  );
  type Location = NonNullable<Awaited<ReturnType<typeof resolveWardrobeLocation>>>;
  type Scope = Parameters<typeof resolveWardrobeLocation>[0];

  await initializeDatabase();
  const repos = getRepositories();
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');

  // The owner-derived token map (both sides build the same one).
  const mountOf = async (kind: 'character' | 'project' | 'group', id: string): Promise<string> => {
    const raw =
      kind === 'character'
        ? await repos.characters.findByIdRaw(id)
        : kind === 'project'
          ? await repos.projects.findByIdRaw(id)
          : await repos.groups.findByIdRaw(id);
    const mp = (raw as { characterDocumentMountPointId?: string | null; officialMountPointId?: string | null } | null);
    const value = kind === 'character' ? mp?.characterDocumentMountPointId : mp?.officialMountPointId;
    if (!value) throw new Error(`${kind} ${id} has no mount`);
    return value;
  };
  tokens.set(await mountOf('character', spec.characterAId), '<ada.mount>');
  tokens.set(await mountOf('character', spec.characterBId), '<bo.mount>');
  tokens.set(await mountOf('character', spec.archivedCharacterId), '<zed.mount>');
  tokens.set(await mountOf('project', spec.bareProjectId), '<bare.mount>');
  tokens.set(await mountOf('project', spec.fullProjectId), '<full.mount>');
  tokens.set(await mountOf('group', spec.groupId), '<group.mount>');
  const bareMount = await mountOf('project', spec.bareProjectId);

  const pointCount = (): number =>
    (midb.prepare('SELECT COUNT(*) AS n FROM doc_mount_points').get() as { n: number }).n;
  const folderPaths = (mountPointId: string): string[] =>
    (midb.prepare('SELECT path FROM doc_mount_folders WHERE mountPointId = ?').all(mountPointId) as Array<{ path: string }>)
      .map((r) => r.path.toLowerCase())
      .sort();

  const shape = (loc: Location | null) =>
    loc === null
      ? null
      : {
          scope: loc.scope,
          id: loc.id,
          mountPointId: remap(loc.mountPointId),
          characterId: loc.characterId,
          origin: loc.origin,
          key: remap(locationKey(loc)),
        };
  const itemShape = (items: Array<{ id: string; title: string; folder?: string; archivedAt?: string | null; characterId?: string | null; componentItemIds: string[] }>) =>
    items
      .map((i) => ({
        id: i.id,
        title: i.title,
        folder: i.folder ?? '',
        archivedAt: i.archivedAt ?? null,
        characterId: i.characterId ?? null,
        componentItemIds: i.componentItemIds,
      }))
      .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const threw = async <T,>(fn: () => Promise<T> | T): Promise<unknown> => {
    try {
      return { ok: await fn() };
    } catch (error) {
      return { threw: remap(error instanceof Error ? error.message : String(error)) };
    }
  };

  const probes: Array<Record<string, unknown>> = [];
  const probe = async (
    label: string,
    scope: Scope,
    id: string | null,
    userId: string,
    ensure: boolean,
    extra?: (loc: Location) => Promise<Record<string, unknown>>,
  ): Promise<void> => {
    captured = [];
    const before = pointCount();
    let result: unknown;
    let loc: Location | null = null;
    try {
      loc = await resolveWardrobeLocation(scope, id, repos, userId, ensure ? { ensure: true } : undefined);
      result = { location: shape(loc) };
    } catch (error) {
      result = { threw: remap(error instanceof Error ? error.message : String(error)) };
    }
    const record: Record<string, unknown> = {
      label,
      scope,
      id,
      ensure,
      result,
      pointsBefore: before,
      pointsAfter: pointCount(),
      bareFolders: folderPaths(bareMount),
    };
    if (loc && extra) record.extra = await extra(loc);
    record.lines = captured.slice();
    probes.push(record);
  };

  const A = spec.userAId;
  const B = spec.userBId;

  await probe('character A (read probe)', 'character', spec.characterAId, A, false, async (loc) => ({
    readItems: itemShape(await loc.readItems()),
    readItemsArchived: itemShape(await loc.readItems(true)),
    findArchived: (await loc.findItem(spec.items.archivedHat.id))?.id ?? null,
    findMissing: (await loc.findItem(spec.missingId))?.id ?? null,
    listFolders: await loc.listFolders(),
    resolveExisting: await threw(() => loc.resolveFolder('formal/evening')),
    resolveHandMade: await threw(() => loc.resolveFolder('hats: summer')),
    resolveNew: await threw(() => loc.resolveFolder('Formal/Winter Gear')),
    resolveBlank: await threw(() => loc.resolveFolder('   ')),
    resolveClimb: await threw(() => loc.resolveFolder('../x')),
    writable: await threw(async () => remap(await loc.writableMountPointId())),
  }));
  await probe('character A (ensure)', 'character', spec.characterAId, A, true);
  await probe("character B as user A (ownership gate)", 'character', spec.characterBId, A, false);
  await probe('character A as user B (ownership gate, reversed)', 'character', spec.characterAId, B, false);
  await probe('character B as user B', 'character', spec.characterBId, B, false, async (loc) => ({
    readItems: itemShape(await loc.readItems(true)),
  }));
  await probe('archived Zed (reads, refuses writes)', 'character', spec.archivedCharacterId, A, false, async (loc) => ({
    readItems: itemShape(await loc.readItems()),
    writable: await threw(async () => remap(await loc.writableMountPointId())),
    create: await threw(() =>
      loc.create({
        id: spec.missingId,
        characterId: spec.archivedCharacterId,
        title: 'Never Written',
        description: null,
        imagePrompt: null,
        types: ['top'],
        componentItemIds: [],
        appropriateness: null,
        isDefault: false,
        replace: false,
        migratedFromClothingRecordId: null,
        archivedAt: null,
        folder: '',
        createdAt: '2026-02-01T00:00:00.000Z',
        updatedAt: '2026-02-01T00:00:00.000Z',
      } as never),
    ),
    update: await threw(() => loc.update(spec.items.archivedCape.id, { title: 'x' })),
    delete: await threw(() => loc.delete(spec.items.archivedCape.id)),
    createFolder: await threw(() => loc.createFolder('Never')),
    readItemsAfter: itemShape(await loc.readItems(true)),
    folders: folderPaths(await mountOf('character', spec.archivedCharacterId)),
  }));
  await probe('general (read probe)', 'general', null, A, false, async (loc) => ({
    readItems: itemShape(await loc.readItems()),
  }));
  await probe('general (ensure)', 'general', null, A, true);
  await probe('project Bare Manor (read probe: store, no folder)', 'project', spec.bareProjectId, A, false, async (loc) => ({
    readItems: itemShape(await loc.readItems(true)),
    listFolders: await loc.listFolders(),
  }));
  await probe('project Bare Manor (ensure: the folder appears)', 'project', spec.bareProjectId, A, true, async (loc) => ({
    readItems: itemShape(await loc.readItems(true)),
  }));
  await probe('project Bare Manor (ensure again: idempotent)', 'project', spec.bareProjectId, A, true);
  await probe('project Null Abbey (read probe)', 'project', spec.nullProjectId, A, false);
  await probe('project Null Abbey (ensure)', 'project', spec.nullProjectId, A, true);
  await probe('project Full Abbey', 'project', spec.fullProjectId, A, false, async (loc) => ({
    readItems: itemShape(await loc.readItems(true)),
    listFolders: await loc.listFolders(),
  }));
  await probe('project Full Abbey (ensure)', 'project', spec.fullProjectId, A, true);
  await probe('group Guild', 'group', spec.groupId, A, false, async (loc) => ({
    readItems: itemShape(await loc.readItems(true)),
    findShadow: (await loc.findItem(spec.items.archivedHat.id))?.title ?? null,
  }));
  await probe('group Guild (ensure)', 'group', spec.groupId, A, true);
  await probe('project missing (ensure)', 'project', spec.missingId, A, true);
  await probe('group missing (ensure)', 'group', spec.missingId, A, true);
  await probe('character missing', 'character', spec.missingId, A, false);
  await probe("project '' (falsy id)", 'project', '', A, true);
  await probe("group '' (falsy id)", 'group', '', A, false);
  await probe("character '' (falsy id)", 'character', '', A, false);
  await probe('project null id', 'project', null, A, true);

  captured = [];
  const groupLocs = async (characterId: string) =>
    (await groupLocationsForCharacter(characterId)).map((l) => shape(l));
  const groups = {
    ada: await groupLocs(spec.characterAId),
    bo: await groupLocs(spec.characterBId),
    missing: await groupLocs(spec.missingId),
    lines: captured.slice(),
  };
  const keys = {
    project: locationKey({ scope: 'project', mountPointId: 'mp-1' }),
    group: locationKey({ scope: 'group', mountPointId: 'mp-1' }),
    general: locationKey({ scope: 'general', mountPointId: spec.generalMountPointId }),
  };

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'wardrobe-location-tier2', probes, groups, keys }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-location-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
