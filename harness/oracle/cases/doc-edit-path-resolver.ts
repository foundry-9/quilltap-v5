/**
 * Read-differential ORACLE for the doc-edit path resolver + URI producers
 * (W4.1d batch 3a). Drives v4's REAL `resolveDocEditPath` +
 * `docStoreUriFor`/`uriForResolvedPath`/`buildDocStoreUriResolver` over a
 * resolution matrix against the seeded fixtures. The Rust port reads the SAME
 * fixtures and must produce the same resolved paths / errors / URIs exactly
 * (every id pinned/shared — zero normalization). Every store is database-backed,
 * so the FS scopes are never touched (documented seam).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DPR_MAIN=/tmp/qt-dpr-main.db QT_FIXTURE_DPR_MOUNT=/tmp/qt-dpr-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/doc-edit-path-resolver.ts > /tmp/oracle-dpr.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  readFileSync,
  existsSync,
  mkdtempSync,
  mkdirSync,
  copyFileSync,
  realpathSync,
  writeFileSync,
  symlinkSync,
} from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  charAId: string;
  generalMountPointId: string;
  projectId: string;
  projectOfficialStore: string;
  normalStore: string;
  sharedStoreA: string;
  sharedStoreB: string;
  disabledStore: string;
  fsStore: string;
  fsStoreName: string;
  legacyProjectId: string;
  fsProjectId: string;
}

/**
 * Materialize the host-filesystem tree both differential sides build identically
 * under a CANONICAL scratch root, so `safeRealpath` + `verifyPathIsWithinBase`
 * see the same structure. (Contents are irrelevant to the resolver — only the
 * dir/symlink layout matters.) Returns `<root>/mount`, the fs-mount base.
 *   <root>/files/_general/existing.md        (general read target)
 *   <root>/files/_general/link-out -> <root>/outside   (general symlink escape)
 *   <root>/files/<legacyProjectId>/          (legacy-fallback base dir)
 *   <root>/mount/docs/note.md                (fs-mount read target)
 *   <root>/mount/escape -> <root>/outside    (fs-mount symlink escape)
 *   <root>/outside/secret.md                 (escape destination)
 */
function materializeTree(root: string, legacyProjectId: string): string {
  const general = join(root, 'files', '_general');
  const mount = join(root, 'mount');
  const outside = join(root, 'outside');
  mkdirSync(general, { recursive: true });
  mkdirSync(join(mount, 'docs'), { recursive: true });
  mkdirSync(outside, { recursive: true });
  mkdirSync(join(root, 'files', legacyProjectId), { recursive: true });
  writeFileSync(join(general, 'existing.md'), '# existing\n');
  writeFileSync(join(mount, 'docs', 'note.md'), '# note\n');
  writeFileSync(join(outside, 'secret.md'), 'secret\n');
  symlinkSync(outside, join(general, 'link-out'));
  symlinkSync(outside, join(mount, 'escape'));
  return mount;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'doc-edit-path-resolver.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_DPR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_DPR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_DPR_MAIN and QT_FIXTURE_DPR_MOUNT must point at the seeded fixtures');
  }

  // CANONICAL scratch root (macOS /var → /private/var) so the paths safeRealpath
  // returns share a stable prefix with the fs-mount basePath, and the sentinel
  // rewrite below is exact on both sides.
  const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'qt-dpr-oracle-')));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'dpr-main-work.db');
  const mountWork = join(scratch, 'dpr-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  const fsMountBase = materializeTree(scratch, spec.legacyProjectId);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { resolveDocEditPath, PathResolutionError } = await import('@/lib/doc-edit/path-resolver');
  const { docStoreUriFor, uriForResolvedPath, buildDocStoreUriResolver } = await import(
    '@/lib/doc-edit/uri-producers'
  );

  await initializeDatabase();

  // Rewrite every filesystem mount's sentinel basePath to this side's tree.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB unavailable');
  midb.prepare("UPDATE doc_mount_points SET basePath = ? WHERE mountType = 'filesystem'").run(fsMountBase);

  // Sentinel-rewrite: the canonical scratch root is per-side, so replace it with a
  // stable token in every emitted path before the diff (paths/errors else exact).
  const sentinelize = (value: unknown): unknown =>
    JSON.parse(JSON.stringify(value).split(scratch).join('__ROOT__'));

  const A = spec.charAId;
  const charA = await getRepositories().characters.findByIdRaw(A);
  const charAVault = charA?.characterDocumentMountPointId as string;

  const rows: unknown[] = [];

  const matrix: Array<{ id: string; scope: string; path: string | undefined; ctx: any }> = [
    { id: 'self-token', scope: 'document_store', path: 'Notes/a.md', ctx: { characterId: A, mountPoint: 'self' } },
    { id: 'self-token-upper', scope: 'document_store', path: 'x.md', ctx: { characterId: A, mountPoint: 'SELF' } },
    { id: 'name-match', scope: 'document_store', path: 'k.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: 'Project Docs' } },
    { id: 'id-match', scope: 'document_store', path: 'k.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: spec.normalStore } },
    { id: 'ambiguous-name', scope: 'document_store', path: 'a.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: 'Shared Name' } },
    { id: 'unknown', scope: 'document_store', path: 'a.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: 'no-such-store' } },
    { id: 'disabled', scope: 'document_store', path: 'a.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: 'Disabled Store' } },
    { id: 'traversal', scope: 'document_store', path: '../etc', ctx: { characterId: A, mountPoint: 'self' } },
    { id: 'absolute', scope: 'document_store', path: '/etc/passwd', ctx: { characterId: A, mountPoint: 'self' } },
    { id: 'missing-path', scope: 'document_store', path: undefined, ctx: { characterId: A, mountPoint: 'self' } },
    { id: 'no-mountpoint', scope: 'document_store', path: 'a.md', ctx: { characterId: A } },
    { id: 'missing-context', scope: 'document_store', path: 'a.md', ctx: { mountPoint: 'Project Docs' } },
    { id: 'project-alias', scope: 'project', path: 'Outline.md', ctx: { projectId: spec.projectId } },
    { id: 'project-no-id', scope: 'project', path: 'a.md', ctx: {} },
    { id: 'operator-override', scope: 'document_store', path: 'a.md', ctx: { operatorOverride: true, mountPoint: 'Project Docs' } },
    // ── P4.6bg host-filesystem branches ──
    { id: 'fs-mount-read', scope: 'document_store', path: 'docs/note.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: spec.fsStoreName } },
    { id: 'fs-mount-new', scope: 'document_store', path: 'docs/fresh.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: spec.fsStoreName } },
    { id: 'fs-mount-symlink-escape', scope: 'document_store', path: 'escape/secret.md', ctx: { characterId: A, projectId: spec.projectId, mountPoint: spec.fsStoreName } },
    { id: 'general-existing', scope: 'general', path: 'existing.md', ctx: {} },
    { id: 'general-new', scope: 'general', path: 'notes.md', ctx: {} },
    { id: 'general-subdir-new', scope: 'general', path: 'sub/deep.md', ctx: {} },
    { id: 'general-traversal', scope: 'general', path: '../x.md', ctx: {} },
    { id: 'general-symlink-escape', scope: 'general', path: 'link-out/secret.md', ctx: {} },
    { id: 'project-legacy-fallback', scope: 'project', path: 'draft.md', ctx: { projectId: spec.legacyProjectId } },
    { id: 'project-official-fs', scope: 'project', path: 'spec.md', ctx: { projectId: spec.fsProjectId } },
  ];

  for (const c of matrix) {
    let result: unknown;
    try {
      const resolved = await resolveDocEditPath(c.scope as never, c.path as never, c.ctx);
      result = { ok: true, resolved };
    } catch (e: any) {
      if (e instanceof PathResolutionError) {
        result = { ok: false, code: e.code, message: e.message };
      } else {
        result = { ok: false, code: 'UNKNOWN', message: String(e?.message ?? e) };
      }
    }
    rows.push({ kind: 'resolve', id: c.id, result: sentinelize(result) });
  }

  // URI producers.
  rows.push({
    kind: 'uri',
    id: 'uri-self',
    result: await docStoreUriFor({ mountPointId: charAVault, mountPointName: '', relativePath: 'Mail/x.md', characterId: A }),
  });
  rows.push({
    kind: 'uri',
    id: 'uri-name',
    result: await docStoreUriFor({ mountPointId: spec.normalStore, mountPointName: 'Project Docs', relativePath: 'k.md' }),
  });
  // NOTE (P4.6bg): the former `uri-ambiguous` / `res-ambiguous` URI-producer cases
  // (two enabled stores both named "Shared Name" → id-form) are OMITTED. Since the
  // d68638b4 NOCASE mount-namespace drift, v4 disambiguates duplicate ENABLED store
  // names at READ time (findEnabled overlays the second as "Shared Name (2)"), so
  // v4's `countByName('Shared Name')` returns 1 (name-form). v5's `count_by_name`
  // reads the raw `name` column and still counts 2 (id-form) — a pre-existing v5
  // divergence in the P4.d7 mount-namespace feature (db/doc_mount_points.rs), NOT in
  // this fs-seam lane's scope. The ambiguity → id-form branch stays covered by the
  // empty-name self-vault cases (`uri-self` / `res-self`).
  rows.push({
    kind: 'uri',
    id: 'uri-resolved-project',
    result: await uriForResolvedPath(
      { absolutePath: '', scope: 'project', basePath: '', relativePath: 'Outline.md' } as never,
      { characterId: A },
    ),
  });

  const resolver = await buildDocStoreUriResolver(A);
  rows.push({ kind: 'uri', id: 'res-self', result: resolver.uriForMount('', charAVault, 'x.md') });
  rows.push({ kind: 'uri', id: 'res-name', result: resolver.uriForMount('Project Docs', spec.normalStore, 'k.md') });
  // `res-ambiguous` omitted for the same P4.d7 divergence reason as `uri-ambiguous` above.
  rows.push({ kind: 'uri', id: 'res-scope', result: resolver.uriForScope('general', 'g.md') });

  closeMountIndexSQLiteClient();
  await closeDatabase();
  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-edit-path-resolver oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
