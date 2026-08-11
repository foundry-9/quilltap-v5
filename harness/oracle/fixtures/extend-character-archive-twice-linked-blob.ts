/**
 * P4.D65 (resumed) fixture EXTENDER — give Sable's vault the twice-linked
 * sha-deduped blob shape that v4 Bug 57 was about.
 *
 * ## Why this shape needs to be in the corpus
 *
 * The document store is content-ADDRESSED: `linkBlobContent` finds-or-creates
 * the `doc_mount_files` row by sha256 and only ever inserts one
 * `doc_mount_blobs` row per file row, so saving the same image twice — into two
 * folders, or twice into the gallery — yields ONE blob over TWO links. That is
 * an ordinary shape a real vault reaches in a week.
 *
 * The export's blob leg (`listByMountPoint`) joins FROM the links, so such a
 * blob is emitted once PER LINK into an archive bundle, carrying the same
 * `blobId` twice. Until v4 `de9f70bf` its preserveIds preflight collected
 * `carriedBlobIds` undeduped and threw on the repeat BEFORE the exists/skippable
 * classifiers ran — so an archived character whose vault held a twice-linked
 * blob could never be rehydrated. v5 had deduped first-occurrence since the
 * round-2 §3 review (a pinned divergence, filed as Bug 57); v4 CONVERGED, and
 * this extension is what turns that pin into a plain-equality differential arm.
 *
 * ## What it adds — two links, no new bytes
 *
 *  1. `photos/portrait-copy.webp` — a second link over PORTRAIT's bytes. Its own
 *     id is not Sable's `defaultImageId`, so the prune dooms it while the
 *     original link (the keep-by-id arm) shelters the content row. On rehydrate
 *     the blob id is therefore ALREADY PRESENT: the skip-if-present leg.
 *  2. `photos/landscape-again.webp` — a second link over LANDSCAPE's bytes.
 *     Both of that blob's links are doomed, so the orphan GC takes the content
 *     row with them and rehydrate has to restore blob and links from the
 *     bundle: the restore-fresh leg.
 *
 * Two links rather than one because the duplicate `blobId` reaches the preflight
 * identically either way, but what happens AFTER the dedupe differs — one blob
 * is claimed against a row that still exists, the other against a row that is
 * gone. A single arm would leave the other leg unproven.
 *
 * Both link ids are PINNED so the committed bytes stay reproducible and the
 * differential's baked-id set is stable. Re-running against an already-extended
 * family is a no-op on the link rows (v4's path-unique upsert overwrites in
 * place) — but it re-stamps the live clock, so the timestamp re-pin at the end
 * runs unconditionally.
 *
 * ⚠ This family's ONLY consumers are `character_archive_tier2_equivalence` and
 * its oracle case, so extending it obliges regenerating exactly that one oracle.
 *
 * Regenerate (Node 24, from the v4 checkout at `de9f70bf`) — the committed files
 * are mutated IN PLACE, so copy them aside first if you want a rollback:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARCHIVE_MAIN=$W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
 *   QT_FIXTURE_ARCHIVE_MOUNT=$W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
 *     $N/node --import tsx \
 *       $W/harness/oracle/fixtures/extend-character-archive-twice-linked-blob.ts
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
}

/** The pinned ids of the two links this script adds. */
const PORTRAIT_COPY_LINK = 'e5000000-0000-4000-8000-0000000000e1';
const LANDSCAPE_AGAIN_LINK = 'e5000000-0000-4000-8000-0000000000e2';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, 'character-archive.json'), 'utf8'),
  ) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_ARCHIVE_MAIN ?? '';
  const mountOut = process.env.QT_FIXTURE_ARCHIVE_MOUNT ?? '';
  for (const [k, v] of Object.entries({ mainOut, mountOut })) {
    if (!v) throw new Error(`${k} env var is required`);
    if (!existsSync(v)) throw new Error(`${k} does not exist: ${v} (extend, do not create)`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-archive-extend-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { basenameOfRelativePath } = await import('@/lib/photos/keep-image-markdown');

  await initializeDatabase();
  const repos = getRepositories();
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount index database unavailable');

  // The vault + the two existing photo links, read (never assumed) from the
  // committed bytes — the builder mints the vault id, so it lives in the
  // sidecar, and the link rows are the authority for the bytes to re-link.
  const meta = JSON.parse(fs.readFileSync(mainOut + '.meta.json', 'utf8')) as {
    sableVaultMountPointId: string;
    portraitLinkId: string;
    landscapeLinkId: string;
  };
  const vault = meta.sableVaultMountPointId;

  const bytesOf = (linkId: string): { data: Buffer; sha256: string; mime: string } => {
    const row = midb
      .prepare(
        `SELECT b.data AS data, b.sha256 AS sha256, b.storedMimeType AS mime
           FROM doc_mount_file_links l
           JOIN doc_mount_blobs b ON b.fileId = l.fileId
          WHERE l.id = ?`,
      )
      .get(linkId) as { data: Buffer; sha256: string; mime: string } | undefined;
    if (!row) throw new Error(`no blob reachable from link ${linkId}`);
    return row;
  };

  for (const [linkId, relativePath, pinnedId, tag] of [
    [meta.portraitLinkId, 'photos/portrait-copy.webp', PORTRAIT_COPY_LINK, 'portrait'],
    [meta.landscapeLinkId, 'photos/landscape-again.webp', LANDSCAPE_AGAIN_LINK, 'landscape'],
  ] as Array<[string, string, string, string]>) {
    const { data, sha256, mime } = bytesOf(linkId);
    await repos.docMountFileLinks.linkBlobContent({
      mountPointId: vault,
      relativePath,
      fileName: basenameOfRelativePath(relativePath),
      originalFileName: `${tag}.png`,
      originalMimeType: 'image/png',
      storedMimeType: mime,
      sha256,
      data,
      description: `${tag} of Sable, linked twice`,
      extractedText: null,
      extractedTextSha256: null,
      extractionStatus: 'skipped',
      linkId: pinnedId,
    } as never);
  }

  // The write stamped a live clock on the new link rows (and touched the
  // folder's `updatedAt`); re-pin exactly as the builder's step 7 does so the
  // two differential sides start from byte-identical pre-state.
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
  // `descriptionUpdatedAt` is stamped only on rows that carry a description;
  // the two new links do, so it needs the same pin.
  midb
    .prepare(
      'UPDATE doc_mount_file_links SET descriptionUpdatedAt = ? WHERE descriptionUpdatedAt IS NOT NULL',
    )
    .run(TS);

  // Echo the two new link ids into the sidecar the differential reads.
  fs.writeFileSync(
    mainOut + '.meta.json',
    JSON.stringify(
      {
        ...meta,
        portraitCopyLinkId: PORTRAIT_COPY_LINK,
        landscapeAgainLinkId: LANDSCAPE_AGAIN_LINK,
      },
      null,
      2,
    ) + '\n',
  );

  await closeDatabase();
  closeMountIndexSQLiteClient();
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `character-archive fixture extended (twice-linked blobs):\n  ${mainOut}\n  ${mountOut}\n`,
  );
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exit(1);
});
