/**
 * P4.D65-remainder fixture EXTENDER — the three shapes the §3 unification
 * review (2026-08-11) recorded as CORPUS-UNDRIVEN in
 * `character_archive_tier2_equivalence` (order item 6).
 *
 * Each one is a piece of material the *existing* corpus could not stage, and
 * each closes an arm that was passing for the wrong reason:
 *
 *  1. **A default embedding profile** (`embedding_profiles`, pinned id,
 *     provider `OPENAI`). Without one, `enqueueEmbeddingJobsForMountPoint` and
 *     the import's `enqueueImportedMemoryEmbeddings` both bail at the
 *     no-default-profile guard, so BOTH sides enqueue exactly zero jobs and the
 *     `background_jobs` comparand the review added was a tripwire rather than a
 *     proof — a port that never enqueued anything passed. With the profile
 *     present, rehydrate's re-embed half writes real rows: one
 *     `EMBEDDING_GENERATE` per restored memory plus one per un-embedded mount
 *     chunk, and the rehydrate result stops carrying v4's
 *     "…no default embedding profile is configured…" warning.
 *
 *     ⚠ **`OPENAI`, deliberately not `BUILTIN`.** A BUILTIN default sends the
 *     import down `scheduleRefit` (`execute.ts:405`), which is a HOST-CADENCE
 *     seam v5 banks by name (P4.D62) rather than porting — the corpus would be
 *     pinning a known, named deferral instead of the enqueue behaviour it is
 *     here for. Any API-backed provider takes the same enqueue path minus that
 *     one call.
 *
 *  2. **An `avatarOverrides[].imageId` keep-arm.** `pruneVault`'s keep-set is
 *     `defaultImageId` PLUS every `avatarOverrides[].imageId` — but the fixture
 *     seeded only the former, so the override half of that union was dead
 *     weight: deleting the loop that reads `avatarOverrides` reddened nothing,
 *     and a regression would silently take the faces off old messages in every
 *     chat that had a per-chat portrait. This adds `photos/override-face.webp`
 *     — its own bytes, its own link, in the SAME `photos/` folder as the doomed
 *     `landscape` — and points Sable's `avatarOverrides[0]` at it.
 *
 *  3. **A standalone avatar-thumbnail `files` row** (pinned id, category
 *     `IMAGE`). It is NOT attached to Sable here: `archivedAvatarFileId` is
 *     vestigial under §4.2a — nothing in the shipped service ever writes it —
 *     so the only honest way to reach rehydrate's follow-up-patch key and its
 *     cosmetic thumbnail delete is a PRE-REVISION tombstone, which the
 *     `rehydrate_pre_revision_avatar` case plants by raw UPDATE after archiving
 *     (the §4.4 guard would refuse the same patch through the repository).
 *     What the fixture owes that case is a row real enough for
 *     `repos.files.delete` to actually remove — a missing row deletes as a
 *     silent no-op on both sides and proves nothing.
 *
 * Every id this script introduces is PINNED, so it joins the differential's
 * baked-id set and stays literal through normalization
 * (`uuid-normalizer-blinds-fixture-baked-ids`) instead of being tokenized
 * alongside the ids the operations mint.
 *
 * ⚠ This family's ONLY consumers are `character_archive_tier2_equivalence` and
 * its oracle case, so extending it obliges regenerating exactly that one
 * oracle. Run this AFTER `build-character-archive-fixture.ts` and
 * `extend-character-archive-twice-linked-blob.ts` — it extends, never creates.
 *
 * Regenerate (Node 24, from the v4 checkout at the round baseline) — the
 * committed files are mutated IN PLACE, so copy them aside first for a
 * rollback:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARCHIVE_MAIN=$W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
 *   QT_FIXTURE_ARCHIVE_MOUNT=$W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
 *     $N/node --import tsx \
 *       $W/harness/oracle/fixtures/extend-character-archive-profile-and-avatars.ts
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  sable: string;
  chatSeated: string;
}

/** The pinned ids this script introduces. */
const EMBEDDING_PROFILE_ID = 'b7000000-0000-4000-8000-0000000000b7';
const OVERRIDE_FACE_LINK = 'e5000000-0000-4000-8000-0000000000e3';
const AVATAR_THUMBNAIL_FILE = 'f5000000-0000-4000-8000-0000000000f1';

/** The same one-pixel PNG the builder uses, with its own tail → its own sha. */
const PNG_HEAD = [
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
  0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
  0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
  0x42, 0x60, 0x82,
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, 'character-archive.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_ARCHIVE_MAIN ?? '';
  const mountOut = process.env.QT_FIXTURE_ARCHIVE_MOUNT ?? '';
  for (const [k, v] of Object.entries({ mainOut, mountOut })) {
    if (!v) throw new Error(`${k} env var is required`);
    if (!existsSync(v)) throw new Error(`${k} does not exist: ${v} (extend, do not create)`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-archive-extend2-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { EmbeddingProfileSchema } = await import('@/lib/schemas/profile.types');
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');
  const { basenameOfRelativePath } = await import('@/lib/photos/keep-image-markdown');

  await initializeDatabase();
  const repos = getRepositories();
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount index database unavailable');

  const meta = JSON.parse(fs.readFileSync(mainOut + '.meta.json', 'utf8')) as {
    sableVaultMountPointId: string;
  };
  const vault = meta.sableVaultMountPointId;

  // ── 1. The default embedding profile ──────────────────────────────────────
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await repos.embeddingProfiles.create(
    {
      userId: spec.userId,
      name: 'Fixture default',
      provider: 'OPENAI',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'text-embedding-3-small',
      dimensions: 4,
      truncateToDimensions: null,
      normalizeL2: true,
      isDefault: true,
      tags: [],
    } as never,
    { id: EMBEDDING_PROFILE_ID, createdAt: TS, updatedAt: TS } as never,
  );

  // ── 2. The avatar-override face ───────────────────────────────────────────
  // Same folder as `landscape` on purpose: the folder sweep must survive on the
  // strength of a KEPT link, and both of this folder's other kept candidates
  // (`portrait`, this) now come from a different half of the keep-set union.
  const photosFolder = await ensureFolderPath(vault, 'photos');
  const bytes = Buffer.concat([Buffer.from(PNG_HEAD), Buffer.from('\noverride-face\n', 'utf-8')]);
  const relativePath = 'photos/override-face.webp';
  await repos.docMountFileLinks.linkBlobContent({
    mountPointId: vault,
    relativePath,
    fileName: basenameOfRelativePath(relativePath),
    folderId: photosFolder,
    originalFileName: 'override-face.png',
    originalMimeType: 'image/png',
    storedMimeType: 'image/png',
    sha256: sha256OfBuffer(bytes),
    data: bytes,
    description: 'Sable, as she appears in the quay chat',
    extractedText: null,
    extractedTextSha256: null,
    extractionStatus: 'skipped',
    linkId: OVERRIDE_FACE_LINK,
  } as never);
  await rawQuery('UPDATE characters SET avatarOverrides = ? WHERE id = ?', [
    JSON.stringify([{ chatId: spec.chatSeated, imageId: OVERRIDE_FACE_LINK }]),
    spec.sable,
  ]);

  // ── 3. The standalone avatar thumbnail ────────────────────────────────────
  // Deliberately NOT pointed at by any character: the pre-revision tombstone is
  // planted per-case (see the header). `storageKey` is null — nothing ever
  // downloads these bytes; the row's existence is the whole point.
  await repos.files.create(
    {
      userId: spec.userId,
      sha256: sha256OfBuffer(Buffer.from('archived-avatar-thumbnail', 'utf-8')),
      originalFilename: 'sable-archived-avatar.png',
      mimeType: 'image/png',
      size: 25,
      width: 64,
      height: 64,
      linkedTo: [],
      source: 'UPLOADED',
      category: 'IMAGE',
      description: 'A pre-revision archived-avatar thumbnail',
      tags: [],
      projectId: null,
      folderPath: '/',
      storageKey: null,
      fileStatus: 'ok',
    } as never,
    { id: AVATAR_THUMBNAIL_FILE, createdAt: TS, updatedAt: TS } as never,
  );

  // ── The timestamp re-pin (the builder's step 7, verbatim) ─────────────────
  for (const [table, cols] of [
    ['doc_mount_points', ['createdAt', 'updatedAt']],
    ['doc_mount_files', ['createdAt', 'updatedAt']],
    ['doc_mount_documents', ['createdAt', 'updatedAt']],
    ['doc_mount_folders', ['createdAt', 'updatedAt']],
    ['doc_mount_file_links', ['createdAt', 'updatedAt', 'lastModified']],
    ['doc_mount_chunks', ['createdAt', 'updatedAt']],
    ['doc_mount_blobs', ['createdAt', 'updatedAt']],
  ] as Array<[string, string[]]>) {
    const sets = cols.map((c) => `"${c}" = ?`).join(', ');
    midb.prepare(`UPDATE "${table}" SET ${sets}`).run(...cols.map(() => TS));
  }
  midb
    .prepare(
      'UPDATE doc_mount_file_links SET descriptionUpdatedAt = ? WHERE descriptionUpdatedAt IS NOT NULL',
    )
    .run(TS);
  await rawQuery('UPDATE "characters" SET "updatedAt" = ?', [TS]);
  await rawQuery('UPDATE "files" SET "createdAt" = ?, "updatedAt" = ?', [TS, TS]);
  await rawQuery('UPDATE "embedding_profiles" SET "createdAt" = ?, "updatedAt" = ?', [TS, TS]);

  fs.writeFileSync(
    mainOut + '.meta.json',
    JSON.stringify(
      {
        ...meta,
        overrideFaceLinkId: OVERRIDE_FACE_LINK,
        embeddingProfileId: EMBEDDING_PROFILE_ID,
        avatarThumbnailFileId: AVATAR_THUMBNAIL_FILE,
      },
      null,
      2,
    ) + '\n',
  );

  await closeDatabase();
  closeMountIndexSQLiteClient();
  rmSync(scratch, { recursive: true, force: true });
  // The rollback journals close EMPTY rather than being unlinked, and an
  // untracked `*.db-journal` beside a committed fixture is pure noise waiting to
  // be committed by accident.
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  process.stderr.write(
    `character-archive fixture extended (embedding profile + avatar override + thumbnail):\n  ${mainOut}\n  ${mountOut}\n`,
  );
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exit(1);
});
