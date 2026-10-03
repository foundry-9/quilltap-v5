/**
 * @jest-environment node
 *
 * ORACLE for the PROJECT ROSTER ACCESS gate (P4.D245 — v4 `9753d0eb2`,
 * `lib/projects/roster-access.ts`), ported to
 * quilltap_core::{project_roster_access, doc_edit::path_resolver,
 * tools::doc_edit::{shared, document_ui, text}, tools::search, wardrobe_tiers}.
 *
 * Drives v4's REAL `projectRosterAdmits` / `rosterGatedProjectId` /
 * `getAccessibleMountPoints` / `resolveDocEditPath` / `executeDocEditTool` /
 * `resolveSharedWardrobeTiersForChat` / `executeWardrobeListTool` against a REAL
 * two-partition fixture whose projects set `allowAnyCharacter` EXPLICITLY (see
 * `build-project-roster-access-fixture.ts`). v4's own `roster-access.test.ts` mocks
 * the repositories; this family does not — the real policy over real stores is
 * the composition the gate lives in.
 *
 * The whole DB stack is doMocked to the REAL modules (the doc-opacity mock set),
 * PLUS the ROOT logger (`@/lib/logger`) is a recorder: every `DocEdit:PathResolver`
 * line (`createServiceLogger` is `logger.child({ service })`) is tagged `resolver`,
 * and every root-logger line whose message is one the gate emits —
 * `[ProjectRoster] Tool access check`, the two `[Wardrobe]` lines, and the
 * repository's two fail-closed `safeQuery` lines — is tagged `root`. Every op
 * records its lines; the Rust side compares them by level, message and field name.
 *
 * The `admits` / `gated_id` ops import `@/lib/projects/roster-access` DYNAMICALLY:
 * at a pin that predates the module they emit `{ absent: true }` and the Rust side
 * skips them, so the case runs at BOTH pins and the composition arms are the
 * both-directions proof (v4 admits everyone at the baseline; the ported v5 then
 * reds every off-roster arm against a baseline oracle).
 *
 * Emits ONE NDJSON line: { case, ops: [{ name, kind, result, logs }, ...] }.
 *
 * Run (Node 24, from a PINNED worktree). STAGE this case OUTSIDE `.claude/` — v4's
 * jest ignores those paths; the filter is ANCHORED.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-project-roster-access
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $V5W/harness/oracle/cases/project-roster-access.test.ts $STAGE/harness/oracle/cases/
 *   cp $V5W/harness/oracle/fixtures/project-roster-access.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PRA_MAIN=/tmp/qt-pra-main.db QT_FIXTURE_PRA_MOUNT=/tmp/qt-pra-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-project-roster-access-fixture.ts
 *   QT_FIXTURE_PRA_MAIN=/tmp/qt-pra-main.db QT_FIXTURE_PRA_MOUNT=/tmp/qt-pra-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-project-roster-access.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "project-roster-access\.test\.ts$"
 *
 * The fixture pair is NOT committed — the builder MINTS it — so rebuild,
 * regenerate, then `cargo test` against that SAME build, in that order.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  name: string;
  kind: string;
  project?: string;
  actor?: string;
  chat?: string;
  operator?: boolean;
  scope?: string;
  mountPoint?: string;
  path?: string;
  tool?: string;
  args?: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  adaId: string;
  beaId: string;
  closedProjectId: string;
  openProjectId: string;
  brokenProjectId: string;
  missingProjectId: string;
  closedChatId: string;
  openChatId: string;
  noProjectChatId: string;
  missingChatId: string;
  ops: Op[];
}
interface RecordedLog { src: 'resolver' | 'root'; level: string; message: string; context: unknown }

/** The ROOT-logger lines the gate emits (v4 `logger.*` with no service). */
const ROOT_MESSAGES = new Set([
  '[ProjectRoster] Tool access check',
  '[Wardrobe] Character off project roster — project wardrobe withheld',
  '[Wardrobe] Project lookup for chat failed',
  'Error finding entity by ID',
  'Error checking character participation',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'project-roster-access.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_PRA_MAIN;
  const mountFixture = process.env.QT_FIXTURE_PRA_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_PRA_MAIN and QT_FIXTURE_PRA_MOUNT must point at the seed fixtures from build-project-roster-access-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // CANONICAL scratch root (the doc-fs idiom): the new-blank General document
  // lands on disk under `<dataDir>/files`, and its realpath must share the
  // prefix the sentinel replaces.
  const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'qt-pra-oracle-')));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  // `files/_general` exists up front (the doc-fs tree): with TWO missing levels
  // v4's `safeRealpath` re-attaches the segments in the wrong order and the
  // General landing refuses as a boundary escape — a v4 quirk this family does
  // not set out to measure.
  mkdirSync(join(scratch, 'files', '_general'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
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
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  // Seamed side effects → no-ops (the doc-opacity set).
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postLibrarianWriteAnnouncement: async () => undefined,
      postLibrarianDeleteAnnouncement: async () => undefined,
      postLibrarianMoveAnnouncement: async () => undefined,
      postLibrarianFolderAnnouncement: async () => undefined,
      postLibrarianOpenAnnouncement: async () => undefined,
      contentHiddenFromCharacters: () => false,
      documentHiddenFromCharacters: () => false,
    };
  });
  jest.doMock('@/lib/doc-edit/reindex-file', () =>
    jest.requireActual('@/lib/doc-edit/reindex-file'),
  );
  jest.doMock('@/lib/tools/handlers/doc-edit/shared', () => {
    const actual = jest.requireActual('@/lib/tools/handlers/doc-edit/shared');
    return {
      __esModule: true,
      ...actual,
      triggerReindexIfNeeded: async () => undefined,
    };
  });
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    __esModule: true,
    enqueueEmbeddingJobsForMountPoint: () => undefined,
  }));

  // The ROOT logger as a recorder (the danger-routing idiom). `createServiceLogger`
  // is `logger.child({ service, module: 'service' })`, so the resolver's lines
  // arrive through `child` tagged with their service; the gate's own lines and
  // the repository's `safeQuery` lines arrive on the root (no service).
  const logs: RecordedLog[] = [];
  // No `requireActual` here: the real module's own initialization re-enters
  // `@/lib/logger` through its imports and trips "Cannot access 'Logger' before
  // initialization" — the danger-routing recorder's shape, with the module's
  // other exports stubbed.
  jest.doMock('@/lib/logger', () => {
    const recorder = (service: string | null): Record<string, unknown> => {
      const record = (level: string) => (message: string, bag?: Record<string, unknown>) => {
        const context = JSON.parse(JSON.stringify(bag ?? {}));
        if (service === 'DocEdit:PathResolver') {
          logs.push({ src: 'resolver', level, message, context });
        } else if (!service && ROOT_MESSAGES.has(message)) {
          logs.push({ src: 'root', level, message, context });
        }
      };
      const self: Record<string, unknown> = {
        debug: record('debug'),
        info: record('info'),
        warn: record('warn'),
        error: record('error'),
        trace: record('trace'),
      };
      self.child = (ctx: Record<string, unknown>) =>
        recorder(typeof ctx?.service === 'string' ? ctx.service : service);
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(null),
      Logger: class {},
      installChildLoggerTransport: () => undefined,
    };
  });

  const work = mkdtempSync(join(scratch, 'run-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { executeDocEditTool, formatDocEditResults } = await import(
    '@/lib/tools/handlers/doc-edit-handler'
  );
  const { resolveDocEditPath, getAccessibleMountPoints } = await import(
    '@/lib/doc-edit/path-resolver'
  );
  const { resolveSharedWardrobeTiersForChat } = await import('@/lib/wardrobe/shared-tiers');
  const { executeWardrobeListTool, formatWardrobeListResults } = await import(
    '@/lib/tools/handlers/wardrobe-list-handler'
  );
  // The chokepoint — ABSENT at a pin before `9753d0eb2`.
  let rosterAccess: {
    projectRosterAdmits: (p: unknown, c: unknown) => Promise<boolean>;
    rosterGatedProjectId: (p: unknown, c: unknown) => Promise<string | undefined>;
  } | null = null;
  try {
    rosterAccess = await import('@/lib/projects/roster-access');
  } catch {
    rosterAccess = null;
  }

  await initializeDatabase();
  const repos = getRepositories();

  // ---- the MINTED placeholders (read back, never transcribed) ----
  const officialOf = async (projectId: string): Promise<string> => {
    const raw = await repos.projects.findByIdRaw(projectId);
    return raw?.officialMountPointId as string;
  };
  const closedStoreId = await officialOf(spec.closedProjectId);
  const openStoreId = await officialOf(spec.openProjectId);
  const closedStoreName = (await repos.docMountPoints.findById(closedStoreId))!.name;
  const subs: Record<string, string> = {
    '{{closedStoreId}}': closedStoreId,
    '{{openStoreId}}': openStoreId,
    '{{closedStoreName}}': closedStoreName,
  };
  const sub = (v: string | undefined): string | undefined =>
    v !== undefined && subs[v] !== undefined ? subs[v] : v;

  const projectIdOf = (p: string | undefined): string | null | undefined => {
    switch (p) {
      case 'closed': return spec.closedProjectId;
      case 'open': return spec.openProjectId;
      case 'broken': return spec.brokenProjectId;
      case 'missing': return spec.missingProjectId;
      case 'empty': return '';
      case 'none': case undefined: return undefined;
      default: throw new Error(`unknown project ${p}`);
    }
  };
  const characterIdOf = (a: string | undefined): string | null | undefined => {
    switch (a) {
      case 'ada': return spec.adaId;
      case 'bea': return spec.beaId;
      case 'empty': return '';
      case 'none': case undefined: return undefined;
      default: throw new Error(`unknown actor ${a}`);
    }
  };
  const chatIdOf = (c: string | undefined): string | undefined => {
    switch (c) {
      case 'closed': return spec.closedChatId;
      case 'open': return spec.openChatId;
      case 'noproject': return spec.noProjectChatId;
      case 'missing': return spec.missingChatId;
      case 'empty': return '';
      case undefined: return undefined;
      default: throw new Error(`unknown chat ${c}`);
    }
  };
  // v4 omits an undefined key; an EMPTY string is kept (JS-falsy, present).
  const ctxWith = (o: Record<string, unknown>): Record<string, unknown> =>
    Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined));

  const sentinelize = (v: unknown): unknown =>
    JSON.parse(JSON.stringify(v).split(scratch).join('__ROOT__'));

  const outLines: string[] = [];
  try {
    const opResults: Array<Record<string, unknown>> = [];
    for (const op of spec.ops) {
      const projectId = projectIdOf(op.project);
      const characterId = characterIdOf(op.actor);
      let result: unknown;
      logs.splice(0);
      switch (op.kind) {
        case 'admits': {
          result = rosterAccess
            ? { allowed: await rosterAccess.projectRosterAdmits(projectId ?? null, characterId ?? null) }
            : { absent: true };
          break;
        }
        case 'gated_id': {
          result = rosterAccess
            ? { projectId: (await rosterAccess.rosterGatedProjectId(projectId ?? null, characterId ?? null)) ?? null }
            : { absent: true };
          break;
        }
        case 'accessible': {
          const mps = await getAccessibleMountPoints(
            ctxWith({ projectId, characterId }) as never,
          );
          result = { stores: mps.map((m) => ({ id: m.id, name: m.name, mountType: m.mountType })) };
          break;
        }
        case 'resolve': {
          try {
            const r = await resolveDocEditPath(
              op.scope as never,
              op.path!,
              ctxWith({ projectId, characterId, mountPoint: sub(op.mountPoint) }) as never,
            );
            result = {
              ok: true,
              scope: r.scope,
              mountPointId: r.mountPointId ?? null,
              mountPointName: r.mountPointName ?? null,
              mountType: r.mountType ?? null,
              relativePath: r.relativePath,
            };
          } catch (e) {
            const err = e as { message?: string; code?: string };
            result = { ok: false, code: err.code ?? null, message: err.message ?? String(e) };
          }
          break;
        }
        case 'tool': {
          const context = ctxWith({
            chatId: chatIdOf(op.chat),
            userId: spec.userId,
            projectId,
            characterId,
          });
          const r = await executeDocEditTool(op.tool!, op.args ?? {}, context as never);
          result = { output: r, formatted: formatDocEditResults(op.tool!, r) };
          break;
        }
        case 'wardrobe_tiers': {
          const tiers = await resolveSharedWardrobeTiersForChat(
            chatIdOf(op.chat) ?? null,
            characterId ?? null,
            op.operator ? { operator: true } : {},
          );
          result = {
            groupMountPointIds: tiers.groupMountPointIds,
            projectMountPointIds: tiers.projectMountPointIds,
          };
          break;
        }
        case 'wardrobe_list': {
          const out = await executeWardrobeListTool(op.args ?? {}, {
            userId: spec.userId,
            chatId: chatIdOf(op.chat) as string,
            characterId: characterId as string,
          });
          result = { output: out, formatted: formatWardrobeListResults(out as never) };
          break;
        }
        default:
          throw new Error(`unknown op kind: ${op.kind}`);
      }
      opResults.push({
        name: op.name,
        kind: op.kind,
        result: sentinelize(result),
        logs: sentinelize(logs.splice(0)),
      });
    }
    outLines.push(JSON.stringify({ case: 'project-roster-access', ops: opResults }));
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `project-roster-access oracle wrote ${outPath} (${spec.ops.length} ops; chokepoint ${rosterAccess ? 'present' : 'ABSENT'})\n`,
  );
}

test('project-roster-access oracle', async () => {
  await main();
});
