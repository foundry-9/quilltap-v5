/**
 * P4.D322 fixture EXTENDER — an `Inventory/` folder under Sable's vault, the
 * material the ahead-of-pin archive keep (v4 `4fbdad186`,
 * `lib/characters/archive-service.ts`) needs to be red-first.
 *
 * v4 `4fbdad186` widened `pruneVault`'s keep-set from `Wardrobe/` alone to
 * `KEPT_FOLDERS = [Wardrobe, Inventory]`, checked by lowercased PREFIX on both
 * the link sweep and the empty-folder sweep. The committed corpus had ZERO
 * `Inventory/` paths, so the keep was invisible to it: v4 before and after the
 * change archived it byte-identically. Each piece planted here closes one arm:
 *
 *  1. **A populated object** — `Inventory/Satchel/brass-lantern.json`, a text
 *     document written through v4's REAL `writeDatabaseDocument` (the generic
 *     doc-store writer the builder uses for `Wardrobe/travelling-coat.md`), so
 *     it arrives WITH chunks and the prune's `embedding_status` scrub runs on it
 *     exactly as on the coat. Nothing at the round pin interprets the JSON; it
 *     is a plain document. Kept at `4fbdad186` by the link prefix rule; pruned
 *     (with its `Inventory/Satchel` folder) at `e8e17b2d9`.
 *  2. **A picture link** — `Inventory/images/<OBJECT_ID>/lantern.webp`, a blob
 *     through `linkBlobContent` with a PINNED link id (the
 *     `extend-character-archive-profile-and-avatars.ts` `override-face` idiom;
 *     its own PNG tail → its own sha). The hazard's "their picture links" half.
 *  3. **An empty subfolder** — `Inventory/Empty Shelf`, a folder row with no
 *     link beneath it. Kept by the folder sweep's PREFIX arm even though it
 *     shelters nothing.
 *  4. **A decoy** — `Inventory Notes/ledger.md`: a sibling folder whose name
 *     starts with the word but not with `inventory/`. PRUNED at BOTH pins — it
 *     kills a prefix-without-slash mutant.
 *
 * The bare-`Inventory` equality arm (`folderPath === prefix.slice(0, -1)`)
 * cannot be reached from this committed state — `Inventory` shelters the kept
 * lantern at `4fbdad186`. The oracle case `archive_sable_inventory_bare` reaches
 * it with a per-case raw-SQL plant that strips everything beneath the bare
 * folder before archiving.
 *
 * WHY v4's generic writers, never raw rows: raw `doc_mount_*` inserts skip the
 * chunk / blob / folder-id invariants v4's writers keep, so the plant would be a
 * shape no real instance holds. WHY never v4's Inventory writers: they exist
 * only past the round pin `e8e17b2d9`, and this recipe RUNS at the round pin —
 * the plant must be producible by code that knows nothing of the Inventory,
 * which is what makes the round pin's PRUNE and `4fbdad186`'s KEEP a clean pair.
 *
 * The pinned ids join the differential's baked-id set and stay literal through
 * normalization; the minted link / document / folder ids become baked too, read
 * from the untouched copy (`uuid-normalizer-blinds-fixture-baked-ids`).
 *
 * ⚠ The committed pair has THREE consumers, all obliged by this extension:
 * `character_archive_tier2_equivalence` (+ its oracle case),
 * `archive_reencrypt_tier2_equivalence` (+ `archive-reencrypt-tier2.test.ts`,
 * whose `sweep_real_bundle` case archives Sable), and `quilltap-web`'s
 * `change_passphrase_archive_sweep` (archives Sable over the wire). Run this
 * AFTER `build-character-archive-fixture.ts`,
 * `extend-character-archive-twice-linked-blob.ts` and
 * `extend-character-archive-profile-and-avatars.ts`, in that order — it
 * extends, never creates.
 *
 * Regenerate (Node 24, from the v4 checkout at the round pin) — the committed
 * files are mutated IN PLACE, so copy them aside first for a rollback:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARCHIVE_MAIN=$W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
 *   QT_FIXTURE_ARCHIVE_MOUNT=$W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
 *     $N/node --import tsx \
 *       $W/harness/oracle/fixtures/extend-character-archive-inventory.ts
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
}

/** The pinned ids this script introduces. */
const LANTERN_PICTURE_LINK = 'e5000000-0000-4000-8000-0000000000e9';
/** The images subfolder's name — an object id in v4's layout. */
const OBJECT_ID = 'e6000000-0000-4000-8000-0000000000e1';

const INVENTORY_FOLDER = 'Inventory';
const DOCUMENT_PATH = 'Inventory/Satchel/brass-lantern.json';
const PICTURE_PATH = `Inventory/images/${OBJECT_ID}/lantern.webp`;
const EMPTY_FOLDER = 'Inventory/Empty Shelf';
const DECOY_PATH = 'Inventory Notes/ledger.md';

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-archive-extend3-'));
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
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
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
  const already = midb
    .prepare('SELECT COUNT(*) AS n FROM doc_mount_file_links WHERE id = ?')
    .get(LANTERN_PICTURE_LINK) as { n: number };
  if (already.n > 0) throw new Error('the Inventory plant is already present (extend once)');

  // ── 1. The bare folder, then the populated object (chunked on write) ──────
  await ensureFolderPath(vault, INVENTORY_FOLDER);
  await ensureFolderPath(vault, 'Inventory/Satchel');
  await writeDatabaseDocument(
    vault,
    DOCUMENT_PATH,
    JSON.stringify(
      {
        name: 'Brass lantern',
        description: 'A brass hurricane lantern, dented, still smoky from the last crossing.',
      },
      null,
      2,
    ) + '\n',
  );

  // ── 2. The object's picture link ──────────────────────────────────────────
  const imagesFolder = await ensureFolderPath(vault, `Inventory/images/${OBJECT_ID}`);
  const bytes = Buffer.concat([Buffer.from(PNG_HEAD), Buffer.from('\ninventory-lantern\n', 'utf-8')]);
  await repos.docMountFileLinks.linkBlobContent({
    mountPointId: vault,
    relativePath: PICTURE_PATH,
    fileName: basenameOfRelativePath(PICTURE_PATH),
    folderId: imagesFolder,
    originalFileName: 'lantern.png',
    originalMimeType: 'image/png',
    storedMimeType: 'image/png',
    sha256: sha256OfBuffer(bytes),
    data: bytes,
    description: 'The brass lantern, lit',
    extractedText: null,
    extractedTextSha256: null,
    extractionStatus: 'skipped',
    linkId: LANTERN_PICTURE_LINK,
  } as never);

  // ── 3. The empty subfolder ────────────────────────────────────────────────
  await ensureFolderPath(vault, EMPTY_FOLDER);

  // ── 4. The decoy sibling (pruned at every pin) ────────────────────────────
  await ensureFolderPath(vault, 'Inventory Notes');
  await writeDatabaseDocument(
    vault,
    DECOY_PATH,
    '# Ledger\n\nOne lantern, brass. One satchel, waxed. Nothing owed.\n',
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

  fs.writeFileSync(
    mainOut + '.meta.json',
    JSON.stringify(
      {
        ...meta,
        inventoryFolderPath: INVENTORY_FOLDER,
        inventoryPictureLinkId: LANTERN_PICTURE_LINK,
        inventoryDocumentPath: DOCUMENT_PATH,
        inventoryDecoyPath: DECOY_PATH,
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
    `character-archive fixture extended (Inventory/ plant):\n  ${mainOut}\n  ${mountOut}\n`,
  );
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exit(1);
});
