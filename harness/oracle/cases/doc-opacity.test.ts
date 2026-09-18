/**
 * @jest-environment node
 *
 * ORACLE for the doc-edit OPACITY COVENANT (P4.D200 — v4 bugs 152 `1065a1f53`
 * and 153 `89fcc3c0d`), ported to
 * quilltap_core::{doc_edit::path_resolver, tools::doc_edit::{shared, text, blob}}.
 *
 * Drives v4's REAL `buildReadResolutionContext` / `buildWriteResolutionContext` /
 * `resolveDocEditPath` / `getAccessibleMountPoints` / `executeDocEditTool` /
 * `flattenTierPool` against a REAL two-partition fixture. v4's own two regression
 * suites mock the repositories and `getGeneralMountPointId`; this family does not —
 * the real pool over real repositories over a real DB is the composition the bug
 * lived in, and the only place where "an opaque character keeps her group stores"
 * and "and is not shown a vault she cannot open" can be proven together.
 *
 * The whole DB stack is doMocked to the REAL modules (past jest.setup's global
 * mocks) plus the real better-sqlite3-multiple-ciphers cipher binding. Only the
 * fire-and-forget side-effect writers are no-op'd — the doc-enum mock set, kept
 * exactly as wide as v5's standing deferrals.
 *
 * Ops run in a SINGLE module graph on ONE fixture copy, IN ORDER. The read-only
 * ops come first; the blob ops MUTATE (Abigail seeds a blob she then reads back,
 * which is what keeps Leilani's four blob refusals non-vacuous — they fail at
 * MOUNT resolution, not for want of a blob); the `flatten` ops are pure.
 *
 * Emits ONE NDJSON line: { case, ops: [{ name, kind, actor, result }, ...] }.
 *
 * ⚠ The `{{...}}` placeholders in the op matrix are substituted from ids MINTED by
 * the builder (both vaults, their names, the project's official store). Both sides
 * read them back from the same fixture, so nothing is transcribed.
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree). STAGE this case
 * OUTSIDE `.claude/` — v4's jest ignores those paths in BOTH testPathIgnorePatterns
 * and modulePathIgnorePatterns, so `--roots` into a worktree matches ZERO tests,
 * leaves the previous NDJSON in place, and the Rust family then passes against a
 * stale oracle. The jest filter is ANCHORED for the same reason.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-doc-opacity
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/doc-opacity.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/doc-opacity.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
 *   QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-doc-opacity.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "doc-opacity\.test\.ts$"
 *
 * The fixture pair is NOT committed — the builder MINTS it — so rebuild,
 * regenerate, then `cargo test` against that SAME build, in that order.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  name: string;
  kind: string;
  actor?: string;
  mountPoint?: string;
  path?: string;
  tool?: string;
  args?: Record<string, unknown>;
  opts?: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  leilaniId: string;
  abigailId: string;
  projectId: string;
  chatId: string;
  groupOfficialMountPointId: string;
  flattenPool: {
    characterMountPointId: string;
    participantMountPointIds: string[];
    groupMountPointIds: string[];
    projectMountPointIds: string[];
    globalMountPointId: string;
  };
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'doc-opacity.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_DOPA_MAIN;
  const mountFixture = process.env.QT_FIXTURE_DOPA_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_DOPA_MAIN and QT_FIXTURE_DOPA_MOUNT must point at the seed fixtures from build-doc-opacity-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dopa-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

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

  // Seamed side effects → no-ops (the doc-enum set; documented Rust seams).
  jest.doMock('@/lib/services/librarian-notifications/writer', () => {
    const actual = jest.requireActual('@/lib/services/librarian-notifications/writer');
    return {
      __esModule: true,
      ...actual,
      postLibrarianWriteAnnouncement: async () => undefined,
      postLibrarianDeleteAnnouncement: async () => undefined,
      postLibrarianMoveAnnouncement: async () => undefined,
      postLibrarianFolderAnnouncement: async () => undefined,
      contentHiddenFromCharacters: () => false,
      documentHiddenFromCharacters: () => false,
    };
  });
  // P4.32: the reindex module runs for REAL — it IS v4's database-store chunk pass.
  jest.doMock('@/lib/doc-edit/reindex-file', () =>
    jest.requireActual('@/lib/doc-edit/reindex-file'),
  );
  // …but the TOOL-level fire-and-forget trigger stays seamed (a standing v5
  // deferral, and un-awaited, so its writes would race the dump).
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
  const { buildReadResolutionContext, buildWriteResolutionContext } = await import(
    '@/lib/tools/handlers/doc-edit/shared'
  );
  const { resolveDocEditPath, getAccessibleMountPoints } = await import(
    '@/lib/doc-edit/path-resolver'
  );
  const { flattenTierPool } = await import('@/lib/mount-index/tiered-mount-pool');

  await initializeDatabase();

  const repos = getRepositories();

  // ---- resolve the MINTED placeholders (read back, never transcribed) ----
  const leilani = await repos.characters.findByIdRaw(spec.leilaniId);
  const abigail = await repos.characters.findByIdRaw(spec.abigailId);
  const leilaniVaultId = leilani?.characterDocumentMountPointId as string;
  const abigailVaultId = abigail?.characterDocumentMountPointId as string;
  const leilaniVault = await repos.docMountPoints.findById(leilaniVaultId);
  const abigailVault = await repos.docMountPoints.findById(abigailVaultId);
  const subs: Record<string, string> = {
    '{{leilaniVaultId}}': leilaniVaultId,
    '{{abigailVaultId}}': abigailVaultId,
    '{{leilaniVaultName}}': leilaniVault!.name,
    '{{abigailVaultName}}': abigailVault!.name,
    '{{groupStoreId}}': spec.groupOfficialMountPointId,
  };
  const sub = (v: unknown): unknown =>
    typeof v === 'string' && subs[v] !== undefined ? subs[v] : v;
  const subArgs = (a: Record<string, unknown>): Record<string, unknown> => {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(a)) out[k] = sub(v);
    return out;
  };

  const ctxFor = (actor: string) => ({
    chatId: spec.chatId,
    userId: spec.userId,
    projectId: spec.projectId,
    characterId: actor === 'abigail' ? spec.abigailId : spec.leilaniId,
  });

  /**
   * Canonical context shape. v4 OMITS `characterIds` / `hideCharacterVaults` /
   * `operatorOverride` when they are undefined while the Rust port carries
   * `Vec::new()` / `false` / `false`; the observable consequence is identical
   * (the collector treats an empty list as absent), so both sides are canonicalized
   * to the explicit form rather than compared on key presence. The DISCRIMINATING
   * fields are `characterId` (null before bug 152's fix, set after) and
   * `hideCharacterVaults` (absent before, true after) — neither is masked by this.
   */
  const shapeOf = (c: Record<string, unknown>) => ({
    projectId: (c.projectId as string) ?? null,
    characterId: (c.characterId as string) ?? null,
    characterIds: (c.characterIds as string[]) ?? [],
    hideCharacterVaults: c.hideCharacterVaults === true,
    mountPoint: (c.mountPoint as string) ?? null,
    operatorOverride: c.operatorOverride === true,
  });

  const resolveRow = async (
    ctx: Record<string, unknown>,
    path: string,
  ): Promise<Record<string, unknown>> => {
    try {
      const r = await resolveDocEditPath('document_store', path, ctx as never);
      return {
        ok: true,
        mountPointId: r.mountPointId ?? null,
        mountPointName: r.mountPointName ?? null,
        mountType: r.mountType ?? null,
        relativePath: r.relativePath,
      };
    } catch (e) {
      const err = e as { message?: string; code?: string };
      return { ok: false, code: err.code ?? null, message: err.message ?? String(e) };
    }
  };

  const outLines: string[] = [];
  try {
    const opResults: Array<Record<string, unknown>> = [];
    for (const op of spec.ops) {
      const actor = op.actor ?? 'leilani';
      const context = ctxFor(actor);
      const mountPoint = sub(op.mountPoint) as string | undefined;
      let result: unknown;

      switch (op.kind) {
        case 'resolve_read': {
          const rc = await buildReadResolutionContext(
            { mount_point: mountPoint } as never,
            context as never,
          );
          result = await resolveRow(rc as never, op.path!);
          break;
        }
        case 'resolve_write': {
          const rc = await buildWriteResolutionContext(
            { mount_point: mountPoint } as never,
            context as never,
          );
          result = await resolveRow(rc as never, op.path!);
          break;
        }
        case 'context_read': {
          const rc = await buildReadResolutionContext(
            { mount_point: mountPoint } as never,
            context as never,
          );
          result = shapeOf(rc as never);
          break;
        }
        case 'context_write': {
          const rc = await buildWriteResolutionContext(
            { mount_point: mountPoint } as never,
            context as never,
          );
          result = shapeOf(rc as never);
          break;
        }
        case 'accessible': {
          const { actingCharacterIsOpaqueToVaults, collectPeerCharacterIdsForReads } = await import(
            '@/lib/tools/handlers/doc-edit/shared'
          );
          const peers = await collectPeerCharacterIdsForReads(context as never);
          const hide = await actingCharacterIsOpaqueToVaults(context as never);
          const mps = await getAccessibleMountPoints({
            projectId: context.projectId,
            characterId: context.characterId,
            extraCharacterIds: peers,
            hideCharacterVaults: hide,
          });
          result = {
            hideCharacterVaults: hide,
            peers,
            stores: mps.map((m) => ({ id: m.id, name: m.name, mountType: m.mountType })),
          };
          break;
        }
        case 'agreement': {
          // Bug 153's covenant, stated as a property: EVERYTHING the enumeration
          // lists must also OPEN through the resolution side, and nothing else.
          const { actingCharacterIsOpaqueToVaults, collectPeerCharacterIdsForReads } = await import(
            '@/lib/tools/handlers/doc-edit/shared'
          );
          const peers = await collectPeerCharacterIdsForReads(context as never);
          const hide = await actingCharacterIsOpaqueToVaults(context as never);
          const mps = await getAccessibleMountPoints({
            projectId: context.projectId,
            characterId: context.characterId,
            extraCharacterIds: peers,
            hideCharacterVaults: hide,
          });
          const rows: Array<Record<string, unknown>> = [];
          for (const m of mps) {
            const rc = await buildReadResolutionContext(
              { mount_point: m.name } as never,
              context as never,
            );
            const r = await resolveRow(rc as never, 'notes.md');
            rows.push({ name: m.name, opens: r.ok === true, resolvedId: r.mountPointId ?? null });
          }
          result = rows;
          break;
        }
        case 'tool': {
          const args = subArgs(op.args ?? {});
          const r = await executeDocEditTool(op.tool!, args, context as never);
          result = { output: r, formatted: formatDocEditResults(op.tool!, r) };
          break;
        }
        case 'flatten': {
          const pool = {
            characterMountPointId: spec.flattenPool.characterMountPointId,
            participantMountPointIds: spec.flattenPool.participantMountPointIds,
            groupMountPointIds: spec.flattenPool.groupMountPointIds,
            projectMountPointIds: spec.flattenPool.projectMountPointIds,
            globalMountPointId: spec.flattenPool.globalMountPointId,
          };
          result = flattenTierPool(pool as never, (op.opts ?? {}) as never);
          break;
        }
        default:
          throw new Error(`unknown op kind: ${op.kind}`);
      }

      opResults.push({ name: op.name, kind: op.kind, actor, result });
    }
    outLines.push(JSON.stringify({ case: 'doc-opacity', ops: opResults }));
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`doc-opacity oracle wrote ${outPath} (${spec.ops.length} ops)\n`);
}

test('doc-opacity oracle', async () => {
  await main();
});
