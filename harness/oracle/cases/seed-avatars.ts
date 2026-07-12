/**
 * Tier-2 oracle case — v4's avatar seeding (`reseedAvatarsForCharacters`, which
 * calls the private `seedAvatars`) over the imported seed characters + the real
 * `first-startup/avatars/Lorian.webp` (P4.4u4 unit 2).
 *
 * Drives, on an empty DB pair: v4's REAL `executeImport` (to create Lorian +
 * Riya), then `reseedAvatarsForCharacters(['Lorian','Riya'])` — the exact code
 * path the startup seed + reset-builtins use. BOTH characters have an avatar file
 * at v4 `a7b1398d` (Lorian.webp + Riya.webp). Both are already WebP, so the vault
 * write stores them AS-IS (no transcode) — their blob sha256 is DETERMINISTIC and
 * diffed exactly (no codec seam).
 *
 * Dumps, per character: the `images/avatar.webp` link + its blob (relativePath /
 * fileName / originalMimeType / storedMimeType / sha256 / size), whether the
 * character's `defaultImageId` now points at that link, and the idempotency of a
 * SECOND reseed (defaultImageId stable, still one link).
 *
 * Reuses the empty qtap-import fixtures (same schema). Run (Node 24, from the v4
 * checkout — cwd must be the v4 root so `first-startup/avatars/` resolves):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
 *   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/seed-avatars.ts \
 *     > /tmp/oracle-seed-avatars.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'qtap-import-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const qtapPath = join(
    here,
    '..',
    '..',
    '..',
    'assets',
    'first-startup',
    'imports',
    'lorian-and-riya.qtap'
  );
  const exportData = JSON.parse(readFileSync(qtapPath, 'utf8'));

  const mainFixture = process.env.QT_FIXTURE_QTAPIMPORT_MAIN;
  const mountFixture = process.env.QT_FIXTURE_QTAPIMPORT_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_QTAPIMPORT_MAIN and QT_FIXTURE_QTAPIMPORT_MOUNT must point at the fixtures from build-qtap-import-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-seed-avatars-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'seed-avatars-main-work.db');
  const mountWork = join(scratch, 'seed-avatars-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { executeImport } = await import('@/lib/import/quilltap-import-service');
  const { reseedAvatarsForCharacters } = await import('@/lib/startup/seed-initial-data');
  const { SINGLE_USER_ID } = await import('@/lib/auth/single-user');
  const { getRepositories } = await import('@/lib/database/repositories');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // Create the seed characters.
  await executeImport(SINGLE_USER_ID, exportData, {
    conflictStrategy: 'skip',
    includeMemories: true,
    includeRelatedEntities: false,
  });

  // Seed the avatars (the exact reset-builtins / startup code path).
  await reseedAvatarsForCharacters(['Lorian', 'Riya']);

  const findChar = async (name: string) => {
    const all = await repos.characters.findByUserId(SINGLE_USER_ID);
    return all.find((c) => c.name.toLowerCase() === name.toLowerCase()) ?? null;
  };

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');

  const linkFor = (mountPointId: string) => {
    const rows = midb
      .prepare(
        'SELECT l.id AS linkId, l.relativePath, l.fileName, l.originalMimeType, ' +
          'b.storedMimeType, b.sha256, length(b.data) AS sizeBytes ' +
          'FROM doc_mount_file_links l JOIN doc_mount_blobs b ON b.fileId = l.fileId ' +
          "WHERE l.mountPointId = ? AND l.relativePath = 'images/avatar.webp' ORDER BY l.id"
      )
      .all(mountPointId) as Array<Record<string, unknown>>;
    return rows[0] ?? null;
  };
  const linkCountFor = (mountPointId: string) =>
    (
      midb
        .prepare(
          "SELECT count(*) AS n FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath = 'images/avatar.webp'"
        )
        .get(mountPointId) as { n: number }
    ).n;

  const dumpChar = async (name: string) => {
    const c = await findChar(name);
    if (!c) throw new Error(`seed character ${name} missing after import`);
    const mount = c.characterDocumentMountPointId as string;
    const link = linkFor(mount);
    const defaultImageId = (c.defaultImageId as string | null) ?? null;
    return {
      mount,
      defaultImageId,
      link: link
        ? {
            relativePath: link.relativePath,
            fileName: link.fileName,
            originalMimeType: link.originalMimeType,
            storedMimeType: link.storedMimeType,
            sha256: link.sha256,
            sizeBytes: link.sizeBytes,
          }
        : null,
      defaultImageIdIsLink: link != null && defaultImageId === (link.linkId as string),
    };
  };

  const lorian = await dumpChar('Lorian');
  const riya = await dumpChar('Riya');

  // Idempotency: reseed again, confirm defaultImageIds are unchanged + still 1 link each.
  await reseedAvatarsForCharacters(['Lorian', 'Riya']);
  const lorianAfter = await findChar('Lorian');
  const riyaAfter = await findChar('Riya');
  const secondRunStable =
    ((lorianAfter?.defaultImageId as string | null) ?? null) === lorian.defaultImageId &&
    ((riyaAfter?.defaultImageId as string | null) ?? null) === riya.defaultImageId;
  const secondRunLinkCounts = {
    lorian: linkCountFor(lorian.mount),
    riya: linkCountFor(riya.mount),
  };

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'seed-avatars-tier2',
      lorianLink: lorian.link,
      lorianDefaultImageIdIsLink: lorian.defaultImageIdIsLink,
      riyaLink: riya.link,
      riyaDefaultImageIdIsLink: riya.defaultImageIdIsLink,
      secondRunLinkCounts,
      secondRunDefaultImageIdStable: secondRunStable,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`seed-avatars-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
