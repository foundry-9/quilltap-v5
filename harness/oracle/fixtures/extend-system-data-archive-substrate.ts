/**
 * P4.D62 fixture EXTENDER — widen the committed `system-data-*` family with the
 * rows the character-archive substrate's differentials need.
 *
 * ## Why an extender rather than a rebuild
 *
 * v4 `d553f72a` adds three `characters` columns (`archivedAt`, `archiveFileId`,
 * `archivedAvatarFileId`) whose port is a SIBLING lane's (P4.D63). Rebuilding
 * `build-system-data-fixture.ts` under `d553f72a` would create the table with
 * those columns, and v4's exporter would then emit the three keys on every
 * character record — dragging the sibling's surface into this lane's byte
 * diffs. The round's shared contract pins the boundary: **this family keeps its
 * OLD schema this round.** So we open the committed databases as they are and
 * add rows through v4's REAL repositories, exactly as the builder does; no
 * `ensureCollection`, no `generateDDL`, nothing that could ALTER a table.
 *
 * ## What it adds, and which arm each row exists for
 *
 *  1. **Two `files` rows for the export-exclusion predicate**
 *     (`lib/export/excluded-files.ts`, new at `01e481f6`): one carrying
 *     `category: 'ARCHIVE'` with no folderPath, one carrying `folderPath:
 *     '/archives'` under an ordinary category. Two rows rather than one so each
 *     of the predicate's two clauses is proven independently — a single row
 *     satisfying both would let either clause rot unnoticed.
 *  2. **A NESTED vault folder + its document** — `lore/ancients/first-age.md`
 *     in Lorian's vault. Every folder in the committed family is a single path
 *     segment with a NULL parent, which makes the ordinary-import path's new
 *     parent-by-path resolution structurally invisible (v4 used to leave the
 *     source `parentId` unremapped; `d553f72a` resolves the parent by path).
 *     `linkDocumentContent` find-or-creates the folder segments, so one call
 *     mints both `lore/ancients` (parented at `lore`) and the document.
 *  3. **One content row shared by TWO vaults** — `notes/shared-summary.md`
 *     written into Lorian's AND Riya's vault with identical bytes. The content
 *     tables are content-addressed, so the second write reuses the first's
 *     `doc_mount_files` row and adds only a link: precisely the shape Bug 54 is
 *     about (a group conversation summary living on one row with one link per
 *     participant). The committed family already shares its EMPTY managed-field
 *     row this way, which is too degenerate to read as a summary.
 *  4. **A real image blob in Lorian's vault** (`images/portrait.webp`) plus the
 *     avatar pointers Bug 52 remaps: Lorian's `defaultImageId` is that blob's
 *     LINK id and its `avatarOverrides` carry one remappable entry and one that
 *     resolves to nothing (dropped, with a warning); Riya's `defaultImageId` is
 *     a legacy `files.id` (kept as-is). Without these the avatar-remap arms are
 *     vacuous — v5 reproduced Bug 52 for months against a fixture where every
 *     avatar pointer was NULL.
 *
 * Every id this script adds is PINNED (v4's create paths accept explicit row
 * ids since `d553f72a`), so the committed bytes stay reproducible and the Rust
 * differentials can assert carried ids literally. Only the folder rows
 * `linkDocumentContent` find-or-creates are minted, and they are baked into the
 * committed bytes once.
 *
 * Re-running against an already-extended family fails loudly on the primary-key
 * conflict rather than silently double-adding.
 *
 * ⚠ Widening the family obliges re-running EVERY `system_*` differential (the
 * P4.9G3 lesson): export, import, import-state, backup, restore-state,
 * delete-data.
 *
 * Regenerate (Node 24, from the v4 checkout at `d553f72a`) — the committed
 * files are mutated IN PLACE, so copy them aside first if you want a rollback:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *     $N/node --import tsx \
 *       $W/harness/oracle/fixtures/extend-system-data-archive-substrate.ts
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── The committed family's pinned ids this script reads ─────────────────────
const LORIAN = 'a1000000-0000-4000-8000-000000000001';
const RIYA = 'a1000000-0000-4000-8000-000000000002';
const LORIAN_VAULT = 'c5e2a72d-b5f1-4486-8325-8308f783c90e';
const RIYA_VAULT = '60d9f423-9107-4843-ae4a-743c52fdc4c4';
const CHAT_1 = 'c1000000-0000-4000-8000-000000000001';
const CHAT_2 = 'c1000000-0000-4000-8000-000000000002';
const FILE_IMG = 'f0000001-0000-4000-8000-000000000001';

// ── The rows this script adds (all pinned) ──────────────────────────────────
const FILE_ARCHIVE_BUNDLE = 'f0000005-0000-4000-8000-000000000005';
const FILE_IN_ARCHIVES_FOLDER = 'f0000006-0000-4000-8000-000000000006';

const NESTED_FILE = 'c7000000-0000-4000-8000-000000000001';
const NESTED_DOC = 'c7000000-0000-4000-8000-000000000002';
const NESTED_LINK = 'c7000000-0000-4000-8000-000000000003';

const SHARED_FILE = 'c7000000-0000-4000-8000-000000000011';
const SHARED_DOC = 'c7000000-0000-4000-8000-000000000012';
const SHARED_LINK_LORIAN = 'c7000000-0000-4000-8000-000000000013';
const SHARED_LINK_RIYA = 'c7000000-0000-4000-8000-000000000014';

const PORTRAIT_FILE = 'c7000000-0000-4000-8000-000000000021';
const PORTRAIT_BLOB = 'c7000000-0000-4000-8000-000000000022';
const PORTRAIT_LINK = 'c7000000-0000-4000-8000-000000000023';

/** An avatar override pointing at nothing at all — the DROPPED arm (Bug 52). */
const DANGLING_IMAGE = 'c7000000-0000-4000-8000-0000000000ff';

const NESTED_CONTENT = '# The First Age\n\nBefore the clockwork, before the tap.\n';
const SHARED_CONTENT =
  '# Shared summary\n\nThe crew agreed the bridge would hold. Both remember it.\n';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, 'system-data.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_SD_MAIN ?? '';
  const mountOut = process.env.QT_FIXTURE_SD_MOUNT ?? '';
  const llmOut = process.env.QT_FIXTURE_SD_LLM ?? '';
  for (const [k, v] of Object.entries({ mainOut, mountOut, llmOut })) {
    if (!v) throw new Error(`${k} env var is required`);
    if (!existsSync(v)) throw new Error(`${k} does not exist: ${v} (extend, do not create)`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sd-extend-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');

  await initializeDatabase();
  const repos = getRepositories();

  // 1. The two exclusion rows. `ARCHIVE` joined `FileCategoryEnum` at
  //    `d553f72a`; v5 stores the category as plain TEXT either way.
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: '5555555555555555555555555555555555555555555555555555555555555555',
      originalFilename: 'lorian-archive.qtap',
      mimeType: 'application/octet-stream',
      size: 2048,
      source: 'GENERATED',
      category: 'ARCHIVE',
      storageKey: `${spec.userId}/archives/lorian-archive.qtap`,
    } as never,
    { id: FILE_ARCHIVE_BUNDLE, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: '6666666666666666666666666666666666666666666666666666666666666666',
      originalFilename: 'stray-note.txt',
      mimeType: 'text/plain',
      size: 12,
      source: 'UPLOADED',
      category: 'DOCUMENT',
      storageKey: `${spec.userId}/archives/stray-note.txt`,
      folderPath: '/archives',
    } as never,
    { id: FILE_IN_ARCHIVES_FOLDER, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. The nested vault folder + its document (one call mints both).
  const links = repos.docMountFileLinks;
  await links.linkDocumentContent({
    mountPointId: LORIAN_VAULT,
    relativePath: 'lore/ancients/first-age.md',
    fileName: 'first-age.md',
    fileType: 'markdown',
    content: NESTED_CONTENT,
    contentSha256: sha256(NESTED_CONTENT),
    plainTextLength: NESTED_CONTENT.length,
    fileSizeBytes: Buffer.byteLength(NESTED_CONTENT, 'utf-8'),
    fileId: NESTED_FILE,
    documentId: NESTED_DOC,
    linkId: NESTED_LINK,
  } as never);

  // 3. One content row, two vaults (the Bug-54 shape). The second call finds
  //    the row by sha256 and adds only a link — its carried `fileId` /
  //    `documentId` are deliberately NOT passed, because they would be ignored
  //    and passing them would misrepresent what the writer honors.
  await links.linkDocumentContent({
    mountPointId: LORIAN_VAULT,
    relativePath: 'notes/shared-summary.md',
    fileName: 'shared-summary.md',
    fileType: 'markdown',
    content: SHARED_CONTENT,
    contentSha256: sha256(SHARED_CONTENT),
    plainTextLength: SHARED_CONTENT.length,
    fileSizeBytes: Buffer.byteLength(SHARED_CONTENT, 'utf-8'),
    fileId: SHARED_FILE,
    documentId: SHARED_DOC,
    linkId: SHARED_LINK_LORIAN,
  } as never);
  await links.linkDocumentContent({
    mountPointId: RIYA_VAULT,
    relativePath: 'notes/shared-summary.md',
    fileName: 'shared-summary.md',
    fileType: 'markdown',
    content: SHARED_CONTENT,
    contentSha256: sha256(SHARED_CONTENT),
    plainTextLength: SHARED_CONTENT.length,
    fileSizeBytes: Buffer.byteLength(SHARED_CONTENT, 'utf-8'),
    linkId: SHARED_LINK_RIYA,
  } as never);

  // 4. The portrait blob + the avatar pointers Bug 52 remaps.
  const portraitBytes = Buffer.from(
    'RIFF____WEBPVP8 a portrait of Lorian, in bytes',
    'utf-8',
  );
  await repos.docMountBlobs.create({
    mountPointId: LORIAN_VAULT,
    relativePath: 'images/portrait.webp',
    originalFileName: 'portrait.webp',
    originalMimeType: 'image/webp',
    storedMimeType: 'image/webp',
    sha256: sha256Bytes(portraitBytes),
    description: 'Lorian, mid-sentence.',
    data: portraitBytes,
    fileId: PORTRAIT_FILE,
    linkId: PORTRAIT_LINK,
    blobId: PORTRAIT_BLOB,
  } as never);

  // Written straight to the columns rather than through `characters.update`:
  // both fields are plain columns (not vault-overlay managed), and the repo
  // would stamp a wall-clock `updatedAt` into a fixture whose every timestamp
  // is pinned.
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb
    .prepare('UPDATE characters SET defaultImageId = ?, avatarOverrides = ? WHERE id = ?')
    .run(
      PORTRAIT_LINK,
      JSON.stringify([
        { chatId: CHAT_1, imageId: PORTRAIT_LINK },
        { chatId: CHAT_2, imageId: DANGLING_IMAGE },
      ]),
      LORIAN,
    );
  // Riya points at a LEGACY `files.id` — the arm that must be left alone.
  mainDb.prepare('UPDATE characters SET defaultImageId = ? WHERE id = ?').run(FILE_IMG, RIYA);

  await closeDatabase();
  closeMountIndexSQLiteClient();
  process.stderr.write(
    `extended system-data-* (archive substrate): 2 files rows, ` +
      `lore/ancients/first-age.md, notes/shared-summary.md ×2 vaults, ` +
      `images/portrait.webp, avatar pointers on ${LORIAN}/${RIYA}\n`,
  );
}

function sha256(text: string): string {
  return sha256Bytes(Buffer.from(text, 'utf-8'));
}

function sha256Bytes(bytes: Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}

await main();
