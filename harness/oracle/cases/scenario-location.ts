/**
 * Tier-2 oracle — THE SCENARIO LOCATION (`lib/scenarios/location.ts`) and the
 * instance-wide catalogue (`lib/scenarios/everywhere.ts`), v4 `545e299bb` at
 * the `83334b9a5` pin (P4.D295).
 *
 * Opens a COPY of the run-time `build-scenario-location-fixture.ts` pair and
 * replays ONE fixed op list against v4's REAL `resolveScenarioLocation`,
 * `classifyScenarioMount`, `lookupScenarioRef`, the locations' `list` / `read`
 * / `idForPath` and the store tiers' `create` / `update` / `rename` /
 * `delete` / `setDefault`, and `listScenariosEverywhere`, in order, on the
 * same working copy (so the writes are sequential state). Every row records
 * the op, its args, its result (a thrown error as `{ threw: { name, message }
 * }`), the `doc_mount_points` count, which mounts hold a `Scenarios/` folder
 * row, and the captured scenario lines (L1–L15). The Rust twin replays the
 * SAME op list from these rows.
 *
 * Normalisation (both sides): a mount id minted during the run (BARE's
 * `ensure`) is the token `<minted>`; a document timestamp not present in the
 * fixture before the run is `<fresh>`; an excerpt is recorded as UTF-16 code
 * units (`excerptUnits`) so a lone surrogate survives NDJSON (R-AA).
 *
 * Run (Node 24, from the v4 checkout or the round's pin), AFTER building the
 * fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_FIXTURE_SL_MAIN=/tmp/qt-scenario-location-main.db \
 *   QT_FIXTURE_SL_MOUNT=/tmp/qt-scenario-location-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/scenario-location.ts \
 *     > /tmp/oracle-scenario-location.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  seedTimestamp: string;
  ids: Record<string, string>;
}

/** The scenario lines this family pins (v4's exact wording). */
const PINNED_MESSAGES = [
  '[ScenarioLocation] Resolved scenario location',
  '[ScenarioLocation] Failed to ensure Scenarios folder',
  '[ScenarioLocation] Created scenario',
  '[ScenarioLocation] Updated scenario',
  '[ScenarioLocation] Renamed scenario',
  '[ScenarioLocation] Deleted scenario',
  '[ScenarioLocation] Mount belongs to an unavailable character',
  '[ScenarioLocation] Refused scenario ref: bad path',
  '[ScenarioLocation] Refused scenario ref: mount is not a scenario owner',
  '[ScenarioLocation] Refused scenario ref: owner did not resolve',
  '[ScenarioLocation] Scenario ref points at a missing file',
  '[ScenariosEverywhere] Listed scenario shelves',
  'Scenario file body is empty; skipping',
  'Scenarios: multiple isDefault: true entries',
  'Failed to demote a previously-default scenario; alphabetical tiebreaker will compensate',
];

/** One op of the replayed list. `scope`/`id` use the spec's id KEYS
 *  (`@alpha`), resolved on both sides; `mount` is a mount KEY. */
type Op =
  | { op: 'resolve'; scope: string; id: string | null; ensure: boolean }
  | { op: 'classify'; mount: string }
  | { op: 'lookup'; mount: string; path: string }
  | { op: 'list'; scope: string; id: string | null; includeArchived: boolean }
  | { op: 'read'; scope: string; id: string | null; path: string }
  | { op: 'idForPath'; scope: string; id: string | null; path: string }
  | { op: 'create'; scope: string; id: string | null; input: Record<string, unknown> }
  | { op: 'update'; scope: string; id: string | null; path: string; patch: Record<string, unknown> }
  | { op: 'rename'; scope: string; id: string | null; path: string; filename: string }
  | { op: 'delete'; scope: string; id: string | null; path: string }
  | { op: 'setDefault'; scope: string; id: string | null; path: string | null }
  | { op: 'everywhere'; includeArchived: boolean }
  | { op: 'breakFolders' };

const OPS: Array<{ name: string } & Op> = [
  // resolve — probes, the tombstones, the raw-schema drops
  { name: 'resolve_general_probe', op: 'resolve', scope: 'general', id: null, ensure: false },
  { name: 'resolve_general_ensure', op: 'resolve', scope: 'general', id: null, ensure: true },
  { name: 'resolve_project_probe', op: 'resolve', scope: 'project', id: 'alpha', ensure: false },
  { name: 'resolve_project_missing', op: 'resolve', scope: 'project', id: 'missing', ensure: false },
  { name: 'resolve_project_no_id', op: 'resolve', scope: 'project', id: null, ensure: false },
  { name: 'resolve_project_empty_id', op: 'resolve', scope: 'project', id: '', ensure: false },
  { name: 'resolve_project_nameless_dropped', op: 'resolve', scope: 'project', id: 'nameless', ensure: false },
  { name: 'resolve_project_bare_probe', op: 'resolve', scope: 'project', id: 'bare', ensure: false },
  { name: 'resolve_project_no_folder_probe_never_heals', op: 'resolve', scope: 'project', id: 'betaLower', ensure: false },
  { name: 'resolve_group_probe', op: 'resolve', scope: 'group', id: 'guild', ensure: false },
  { name: 'resolve_group_unnamed_dropped', op: 'resolve', scope: 'group', id: 'unnamedGroup', ensure: false },
  { name: 'resolve_character_probe', op: 'resolve', scope: 'character', id: 'ada', ensure: false },
  { name: 'resolve_character_ensure_ignored', op: 'resolve', scope: 'character', id: 'ada', ensure: true },
  { name: 'resolve_character_archived', op: 'resolve', scope: 'character', id: 'zed', ensure: false },
  { name: 'resolve_character_other_user', op: 'resolve', scope: 'character', id: 'otto', ensure: false },
  { name: 'resolve_character_vaultless', op: 'resolve', scope: 'character', id: 'nova', ensure: false },
  { name: 'resolve_character_missing', op: 'resolve', scope: 'character', id: 'missing', ensure: false },
  // classify
  { name: 'classify_general', op: 'classify', mount: 'general' },
  { name: 'classify_character', op: 'classify', mount: 'ada.vault' },
  { name: 'classify_archived_character', op: 'classify', mount: 'zed.vault' },
  { name: 'classify_other_users_character', op: 'classify', mount: 'otto.vault' },
  { name: 'classify_project', op: 'classify', mount: 'alpha.store' },
  { name: 'classify_group', op: 'classify', mount: 'guild.store' },
  { name: 'classify_linked_store', op: 'classify', mount: 'linked' },
  { name: 'classify_arbitrary_store', op: 'classify', mount: 'arbitrary' },
  { name: 'classify_unknown', op: 'classify', mount: 'unknown' },
  { name: 'classify_nameless_project_store', op: 'classify', mount: 'nameless.store' },
  // lookup
  { name: 'lookup_bad_path_before_any_read', op: 'lookup', mount: 'alpha.store', path: '../x.md' },
  { name: 'lookup_nested', op: 'lookup', mount: 'alpha.store', path: 'Scenarios/a/b.md' },
  { name: 'lookup_empty_path', op: 'lookup', mount: 'alpha.store', path: '' },
  { name: 'lookup_linked_store', op: 'lookup', mount: 'linked', path: 'Scenarios/x.md' },
  { name: 'lookup_arbitrary_store', op: 'lookup', mount: 'arbitrary', path: 'Scenarios/y.md' },
  { name: 'lookup_archived_character', op: 'lookup', mount: 'zed.vault', path: 'Scenarios/z.md' },
  { name: 'lookup_unknown_mount', op: 'lookup', mount: 'unknown', path: 'Scenarios/a.md' },
  { name: 'lookup_general_ok', op: 'lookup', mount: 'general', path: 'Scenarios/welcome.md' },
  { name: 'lookup_project_ok', op: 'lookup', mount: 'alpha.store', path: 'Scenarios/a.md' },
  { name: 'lookup_group_bare_name', op: 'lookup', mount: 'guild.store', path: 'g' },
  { name: 'lookup_character_default_from_column', op: 'lookup', mount: 'ada.vault', path: 'Scenarios/Arrival.md' },
  { name: 'lookup_character_frontmatter', op: 'lookup', mount: 'ada.vault', path: 'Scenarios/b-note.md' },
  { name: 'lookup_character_archived_file_reads', op: 'lookup', mount: 'ada.vault', path: 'Scenarios/archived.md' },
  { name: 'lookup_missing_file', op: 'lookup', mount: 'alpha.store', path: 'Scenarios/missing.md' },
  { name: 'lookup_blank_body_is_missing', op: 'lookup', mount: 'alpha.store', path: 'Scenarios/blank.md' },
  { name: 'lookup_percent_decoded_character', op: 'lookup', mount: 'ada.vault', path: 'Scenarios/a%20b.md' },
  { name: 'lookup_percent_decoded_project', op: 'lookup', mount: 'alpha.store', path: 'Scenarios/a%20b.md' },
  { name: 'lookup_malformed_percent_throws', op: 'lookup', mount: 'alpha.store', path: 'Scenarios/%E0.md' },
  { name: 'lookup_lowercase_folder_is_nested', op: 'lookup', mount: 'alpha.store', path: 'scenarios/A.md' },
  { name: 'lookup_case_insensitive_file', op: 'lookup', mount: 'alpha.store', path: 'Scenarios/b.md' },
  // list / read / idForPath
  { name: 'list_project', op: 'list', scope: 'project', id: 'alpha', includeArchived: false },
  { name: 'list_project_archived', op: 'list', scope: 'project', id: 'alpha', includeArchived: true },
  { name: 'list_character', op: 'list', scope: 'character', id: 'ada', includeArchived: false },
  { name: 'list_character_archived', op: 'list', scope: 'character', id: 'ada', includeArchived: true },
  { name: 'list_general', op: 'list', scope: 'general', id: null, includeArchived: false },
  { name: 'read_project_archived_file', op: 'read', scope: 'project', id: 'alpha', path: 'Scenarios/c.md' },
  { name: 'read_bad_path_is_not_found', op: 'read', scope: 'project', id: 'alpha', path: '../c.md' },
  { name: 'id_for_path_character', op: 'idForPath', scope: 'character', id: 'ada', path: 'Scenarios/Arrival.md' },
  { name: 'id_for_path_store', op: 'idForPath', scope: 'project', id: 'alpha', path: 'Scenarios/a.md' },
  // the catalogue, before any write
  { name: 'everywhere', op: 'everywhere', includeArchived: false },
  { name: 'everywhere_archived', op: 'everywhere', includeArchived: true },
  // store writes (sequential)
  { name: 'create_conflict', op: 'create', scope: 'project', id: 'alpha', input: { filename: 'a', body: 'x' } },
  { name: 'create_filename_empty_after_sanitise', op: 'create', scope: 'project', id: 'alpha', input: { filename: '.md', body: 'x' } },
  { name: 'create_default_demotes_the_rest', op: 'create', scope: 'project', id: 'alpha', input: { filename: 'new one', name: 'New', description: 'Fresh', isDefault: true, body: 'A fresh scene.' } },
  { name: 'create_whitespace_body_written_then_not_found', op: 'create', scope: 'project', id: 'alpha', input: { filename: 'ws', body: '   ' } },
  { name: 'update_keeps_archived_flag', op: 'update', scope: 'project', id: 'alpha', path: 'Scenarios/c.md', patch: { body: 'Updated, still archived.' } },
  { name: 'update_missing', op: 'update', scope: 'project', id: 'alpha', path: 'Scenarios/missing.md', patch: { body: 'x' } },
  { name: 'update_malformed_percent', op: 'update', scope: 'project', id: 'alpha', path: '%E0', patch: { body: 'x' } },
  { name: 'rename_same_name_missing_is_not_found', op: 'rename', scope: 'project', id: 'alpha', path: 'Scenarios/missing.md', filename: 'missing' },
  { name: 'rename_conflict', op: 'rename', scope: 'project', id: 'alpha', path: 'Scenarios/a.md', filename: 'B' },
  { name: 'rename_same_name_existing', op: 'rename', scope: 'project', id: 'alpha', path: 'Scenarios/a.md', filename: 'a' },
  { name: 'rename_moves', op: 'rename', scope: 'project', id: 'alpha', path: 'Scenarios/a.md', filename: 'renamed' },
  { name: 'rename_empty_after_sanitise', op: 'rename', scope: 'project', id: 'alpha', path: 'Scenarios/renamed.md', filename: '.md' },
  { name: 'delete_missing', op: 'delete', scope: 'project', id: 'alpha', path: 'Scenarios/missing.md' },
  { name: 'delete_ok', op: 'delete', scope: 'project', id: 'alpha', path: 'Scenarios/renamed.md' },
  { name: 'set_default_null_clears', op: 'setDefault', scope: 'project', id: 'alpha', path: null },
  { name: 'list_after_clear', op: 'list', scope: 'project', id: 'alpha', includeArchived: true },
  { name: 'set_default_one', op: 'setDefault', scope: 'project', id: 'alpha', path: 'Scenarios/B.md' },
  { name: 'set_default_unknown_throws', op: 'setDefault', scope: 'project', id: 'alpha', path: 'Scenarios/nope.md' },
  { name: 'list_after_set', op: 'list', scope: 'project', id: 'alpha', includeArchived: true },
  { name: 'create_general', op: 'create', scope: 'general', id: null, input: { filename: 'third', body: 'A third general scene.' } },
  // ensure — provisioning and healing
  { name: 'ensure_provisions_bare_project', op: 'resolve', scope: 'project', id: 'bare', ensure: true },
  { name: 'ensure_heals_folder', op: 'resolve', scope: 'project', id: 'betaLower', ensure: true },
  { name: 'ensure_unnamed_group_dropped', op: 'resolve', scope: 'group', id: 'unnamedGroup', ensure: true },
  { name: 'everywhere_after_writes', op: 'everywhere', includeArchived: false },
  // a broken folder table: the heal WARNs then RETHROWS; a probe is untouched
  { name: 'break_folders', op: 'breakFolders' },
  { name: 'ensure_folder_failure_rethrows', op: 'resolve', scope: 'group', id: 'guild', ensure: true },
  { name: 'probe_after_break', op: 'resolve', scope: 'group', id: 'guild', ensure: false },
];

function render(v: unknown, remap: (s: string) => string): string {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'string') return remap(v);
  if (typeof v === 'boolean' || typeof v === 'number') return String(v);
  return remap(JSON.stringify(v));
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'scenario-location.json'), 'utf8')) as Spec;
  const mainFixture = process.env.QT_FIXTURE_SL_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SL_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_SL_MAIN and QT_FIXTURE_SL_MOUNT must point at the built fixture .db files');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-sl-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'sl-main.db');
  const mountWork = join(scratch, 'sl-mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  let captured: string[] = [];
  let remap = (s: string) => s;
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!PINNED_MESSAGES.includes(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      const rendered = Object.entries(fields)
        .map(([k, v]) => (Array.isArray(v) || (v !== null && typeof v === 'object')
          ? ` ${k}Json=${render(v, remap)}`
          : ` ${k}=${render(v, remap)}`))
        .join('');
      captured.push(`${data.level.toUpperCase()} ${data.message}${rendered}`);
    },
  });

  const { initializeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const loc = await import('@/lib/scenarios/location');
  const { listScenariosEverywhere } = await import('@/lib/scenarios/everywhere');

  await initializeDatabase();
  const repos = getRepositories();
  const midb = getRawMountIndexDatabase();
  const maindb = getRawDatabase();
  if (!midb || !maindb) throw new Error('DB handles unavailable');
  const userId = spec.userId;
  const ID = spec.ids;

  // Mount keys → ids (read back from the rows — vault / store ids are minted).
  const col = (table: string, column: string, id: string): string | null =>
    ((maindb.prepare(`SELECT "${column}" AS v FROM "${table}" WHERE id = ?`).get(id) as { v: string | null } | undefined)?.v ?? null);
  const MOUNTS: Record<string, string> = {
    general: ID.general,
    linked: ID.linked,
    arbitrary: ID.arbitrary,
    unknown: ID.unknown,
    'ada.vault': col('characters', 'characterDocumentMountPointId', ID.ada)!,
    'zed.vault': col('characters', 'characterDocumentMountPointId', ID.zed)!,
    'otto.vault': col('characters', 'characterDocumentMountPointId', ID.otto)!,
    'alpha.store': col('projects', 'officialMountPointId', ID.alpha)!,
    'guild.store': col('groups', 'officialMountPointId', ID.guild)!,
    'nameless.store': col('projects', 'officialMountPointId', ID.nameless)!,
  };
  const knownMounts = new Set(
    (midb.prepare('SELECT id FROM doc_mount_points').all() as Array<{ id: string }>).map((r) => r.id),
  );
  const knownTs = new Set<string>();
  for (const r of midb.prepare('SELECT d.createdAt AS c, d.updatedAt AS u, l.lastModified AS m FROM doc_mount_documents d JOIN doc_mount_file_links l ON l.fileId = d.fileId').all() as Array<{ c: string; u: string; m: string }>) {
    knownTs.add(r.c); knownTs.add(r.u); knownTs.add(r.m);
  }
  remap = (s: string) => {
    let out = s;
    for (const m of midb.prepare('SELECT id FROM doc_mount_points').all() as Array<{ id: string }>) {
      if (!knownMounts.has(m.id)) out = out.split(m.id).join('<minted>');
    }
    return out;
  };
  const ts = (t: string) => (knownTs.has(t) ? t : '<fresh>');
  const idOf = (key: string | null) => (key === null ? null : key === '' ? '' : ID[key]);

  const scen = (s: Record<string, unknown> | null) =>
    s === null
      ? null
      : {
          path: s.path,
          filename: s.filename,
          name: s.name,
          ...(s.description !== undefined && { description: s.description }),
          isDefault: s.isDefault,
          rawIsDefault: s.rawIsDefault,
          archived: s.archived,
          body: s.body,
          lastModified: ts(s.lastModified as string),
          createdAt: ts(s.createdAt as string),
          updatedAt: ts(s.updatedAt as string),
        };
  const shape = (l: { scope: string; id: string | null; mountPointId: string; ownerName: string } | null) =>
    l === null ? null : { scope: l.scope, id: l.id, mountPointId: remap(l.mountPointId), ownerName: l.ownerName };
  const units = (s: string) => Array.from({ length: s.length }, (_, i) => s.charCodeAt(i));
  const pointCount = () => (midb.prepare('SELECT COUNT(*) AS n FROM doc_mount_points').get() as { n: number }).n;
  let foldersBroken = false;
  const scenarioFolders = (): string[] => {
    if (foldersBroken) return ['<broken>'];
    return (midb.prepare("SELECT mountPointId AS m FROM doc_mount_folders WHERE LOWER(path) = 'scenarios'").all() as Array<{ m: string }>)
      .map((r) => remap(r.m))
      .sort();
  };
  const threw = (e: unknown) => ({ threw: { name: e instanceof Error ? e.name : 'Error', message: remap(e instanceof Error ? e.message : String(e)) } });

  for (const o of OPS) {
    captured = [];
    let result: unknown;
    try {
      switch (o.op) {
        case 'resolve':
          result = shape(await loc.resolveScenarioLocation(o.scope as never, idOf(o.id), repos, userId, { ensure: o.ensure }) as never);
          break;
        case 'classify':
          result = await loc.classifyScenarioMount(MOUNTS[o.mount], repos, userId);
          break;
        case 'lookup': {
          const r = await loc.lookupScenarioRef({ mountPointId: MOUNTS[o.mount], path: o.path }, repos, userId);
          result = r.status === 'refused'
            ? { status: 'refused', reason: r.reason }
            : r.status === 'missing'
              ? { status: 'missing', location: shape(r.location as never), path: r.path }
              : { status: 'ok', location: shape(r.location as never), scenario: scen(r.scenario as never), scenarioId: r.location.idForPath(r.scenario.path) };
          break;
        }
        case 'list': {
          const l = await loc.resolveScenarioLocation(o.scope as never, idOf(o.id), repos, userId);
          const r = await l!.list({ includeArchived: o.includeArchived });
          result = { scenarios: r.scenarios.map((s) => scen(s as never)), warnings: r.warnings };
          break;
        }
        case 'read': {
          const l = await loc.resolveScenarioLocation(o.scope as never, idOf(o.id), repos, userId);
          result = scen((await l!.read(o.path)) as never);
          break;
        }
        case 'idForPath': {
          const l = await loc.resolveScenarioLocation(o.scope as never, idOf(o.id), repos, userId);
          result = l!.idForPath(o.path);
          break;
        }
        case 'create':
        case 'update':
        case 'rename':
        case 'delete':
        case 'setDefault': {
          const l = (await loc.resolveScenarioLocation(o.scope as never, idOf(o.id), repos, userId)) as Awaited<ReturnType<typeof loc.resolveScenarioLocation>> & Record<string, (...a: unknown[]) => Promise<unknown>>;
          const r =
            o.op === 'create' ? await l.create(o.input)
              : o.op === 'update' ? await l.update(o.path, o.patch)
                : o.op === 'rename' ? await l.rename(o.path, o.filename)
                  : o.op === 'delete' ? await l.delete(o.path)
                    : await l.setDefault(o.path);
          result = o.op === 'delete' || o.op === 'setDefault' ? null : scen(r as never);
          break;
        }
        case 'everywhere': {
          const sources = await listScenariosEverywhere(repos, userId, { includeArchived: o.includeArchived });
          result = sources.map((src) => ({
            ...src,
            mountPointId: remap(src.mountPointId),
            scenarios: src.scenarios.map((s) => {
              const { excerpt, updatedAt, ...rest } = s;
              return { ...rest, excerptUnits: units(excerpt), updatedAt: ts(updatedAt) };
            }),
          }));
          break;
        }
        case 'breakFolders':
          midb.exec('DROP TABLE doc_mount_folders');
          foldersBroken = true;
          result = null;
          break;
      }
    } catch (e) {
      result = threw(e);
    }
    process.stdout.write(
      `${JSON.stringify({ name: o.name, op: o, result, points: pointCount(), scenarioFolders: scenarioFolders(), lines: captured })}\n`,
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scenario-location oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
