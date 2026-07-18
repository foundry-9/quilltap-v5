/**
 * P4.9c fixture builder (lane C) — the committed test-pepper substrate for the
 * user-profile differential (`profile_routes_equivalence`). Everything is baked
 * via v4's REAL repos so the DDL + row shapes are production-identical.
 *
 * ── TWO FILES, ONE FAMILY ────────────────────────────────────────────────────
 *   - `profile-main.db`  — the two users + the three files rows.
 *   - `profile-mount.db` — the mount-index sibling. The profile surface never
 *                          reads it, but `Db::open` takes the partition set, so
 *                          the family ships both and the Rust side opens the
 *                          same two-partition world every other route
 *                          differential does.
 *
 * ── WHAT THE CORPUS STAGES ───────────────────────────────────────────────────
 *   - The PRIMARY user: a full row (name + email set, `image` NULL) — the GET
 *     shape, the PUT happy paths, and the "re-submitting my own address is not
 *     a clash" arm.
 *   - A SECOND user holding a DIFFERENT email — the uniqueness rejection arm.
 *     Its `name` is NULL so the profile projection's nullable columns are
 *     exercised on a real row rather than only through mutations.
 *   - Three files rows covering the avatar category gate: `IMAGE` (accepted),
 *     `AVATAR` (also accepted — the second literal in v4's list, which a port
 *     that only checked `IMAGE` would silently fail), and `DOCUMENT` (the
 *     rejection, whose 400 quotes the category back).
 *   - `missingUserId` / `missingFileId` are deliberately ABSENT ids — the
 *     GET's 500 arm and the PATCH's 404 arm.
 *
 * Regenerate (Node 24, from the PINNED v4 worktree — the checkout is dirty)
 * + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd /tmp/qt-v4-baseline
 *   QT_FIXTURE_PROFILE_MAIN=$W/crates/quilltap-web/tests/fixtures/profile-main.db \
 *   QT_FIXTURE_PROFILE_MOUNT=$W/crates/quilltap-web/tests/fixtures/profile-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-profile-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  otherUserId: string;
  primaryEmail: string;
  otherEmail: string;
  files: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'profile-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_PROFILE_MAIN;
  const mountOut = process.env.QT_FIXTURE_PROFILE_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_PROFILE_{MAIN,MOUNT} must point at the .db files to write');
  }
  // Delete FIRST so a failed build fails the next step loudly instead of
  // silently leaving yesterday's substrate in place.
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-profile-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);

  // The mount-index sibling: empty, but with the production DDL in place so the
  // Rust side opens the same two-partition world (it has no lazy CREATE TABLE).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // The primary user (fully populated but `image` NULL — the avatar arms set it).
  await repos.users.create(
    { username: 'friday', email: spec.primaryEmail, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  // The second user: a DIFFERENT email (the uniqueness clash) and a NULL name.
  await repos.users.create(
    { username: 'amy', email: spec.otherEmail, name: null } as never,
    { id: spec.otherUserId, createdAt: TS, updatedAt: TS } as never,
  );

  // The three files rows behind the avatar category gate.
  for (const f of spec.files) {
    const { id, ...rest } = f as Record<string, unknown>;
    await repos.files.create({ userId: spec.userId, linkedTo: [], tags: [], ...rest } as never, {
      id,
      createdAt: TS,
      updatedAt: TS,
    } as never);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  for (const out of [mainOut, mountOut]) {
    const j = out + '-journal';
    if (existsSync(j)) rmSync(j);
  }

  process.stderr.write(
    `built profile fixture: ${mainOut} (+${mountOut}) — 2 users, ${spec.files.length} files\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`profile fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
