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
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-dpr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'dpr-main-work.db');
  const mountWork = join(scratch, 'dpr-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { resolveDocEditPath, PathResolutionError } = await import('@/lib/doc-edit/path-resolver');
  const { docStoreUriFor, uriForResolvedPath, buildDocStoreUriResolver } = await import(
    '@/lib/doc-edit/uri-producers'
  );

  await initializeDatabase();
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
    rows.push({ kind: 'resolve', id: c.id, result });
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
  rows.push({
    kind: 'uri',
    id: 'uri-ambiguous',
    result: await docStoreUriFor({ mountPointId: spec.sharedStoreA, mountPointName: 'Shared Name', relativePath: 'a.md' }),
  });
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
  rows.push({ kind: 'uri', id: 'res-ambiguous', result: resolver.uriForMount('Shared Name', spec.sharedStoreA, 'a.md') });
  rows.push({ kind: 'uri', id: 'res-scope', result: resolver.uriForScope('general', 'g.md') });

  await closeDatabase();
  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-edit-path-resolver oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
