/**
 * P4.D65 fixture builder — the committed test-pepper substrate for the
 * character-archive differential (`character_archive_tier2_equivalence`).
 * Everything is baked through v4's REAL repositories and the REAL store-write
 * helpers, so the DDL, the vault scaffold, the chunk-on-write pass and the blob
 * rows are production-identical on both differential sides.
 *
 * ── TWO FILES, ONE FAMILY ────────────────────────────────────────────────────
 *   - `character-archive-main.db`  — the user, three characters, two chats, the
 *                                    memories (Sable's three plus Tor's one
 *                                    ABOUT Sable, which the prune must NOT
 *                                    touch), and the vector index Sable's
 *                                    archive deletes.
 *   - `character-archive-mount.db` — Sable's vault: the ten managed documents +
 *                                    `Wardrobe/` from the real scaffold (the
 *                                    keep-set), plus the prune material.
 *
 * ── WHAT THE CORPUS STAGES ───────────────────────────────────────────────────
 * SABLE has a real vault. Beyond the scaffold's keep-set it carries:
 *   - `Mail/letter-01.md` + `Conversation Summaries/chat-a.md` — text documents
 *     written through `writeDatabaseDocument`, so they arrive WITH chunks (the
 *     P4.6BK chunk-on-write pass). Their folders shelter nothing else, so the
 *     post-prune folder sweep must delete both.
 *   - `photos/portrait.webp` — a blob whose LINK id is Sable's `defaultImageId`.
 *     It is the keep-by-id arm: nothing about its path saves it.
 *   - `photos/landscape.webp` — a second blob under the SAME folder, doomed. The
 *     folder therefore survives the sweep only because `portrait` shelters it —
 *     a fixture where the kept blob sat elsewhere could not tell the difference.
 *   - `Wardrobe/travelling-coat.md` — doomed by path but SAVED by the wardrobe
 *     prefix, so the prefix arm is exercised by a file the ten-path list does
 *     not name.
 * TOR has NO vault (the FK is nulled and the scaffolded store torn down after
 * the real create), which is the "bundle must carry ZERO stores" verification
 * arm. QUILL is a plain third character that must be wholly untouched.
 *
 * Two chats seat Sable: `chatSeated` has her ACTIVE (the flip to `absent` and
 * back), `chatRemoved` has her `removed` — a seat that left before the archive
 * and must stay `removed` through both directions.
 *
 * Sable's three memories cross-link (`relatedMemoryIds`), and Tor's memory
 * points at one of them, so the gate's unlink scrub is measurable: Tor's row
 * must survive with the doomed id scrubbed out of its links.
 *
 * ── WHAT IS DELIBERATELY *NOT* HERE ──────────────────────────────────────────
 * No `files` rows: the archive MINTS the only ARCHIVE row the corpus needs, and
 * a pre-baked one would be indistinguishable from it after id normalization.
 *
 * ── PINNED TIMESTAMPS ────────────────────────────────────────────────────────
 * The store writes stamp a LIVE clock; every link/document/folder/chunk row's
 * `createdAt`/`updatedAt`/`lastModified` is re-pinned by raw UPDATE afterwards
 * (step 7) so the two sides' pre-state is byte-identical. The minted vault +
 * link ids are echoed to the `.meta.json` sidecar that both the oracle case and
 * the Rust differential read.
 *
 * Regenerate (Node 24, from the v4 checkout) + the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARCHIVE_MAIN=$W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
 *   QT_FIXTURE_ARCHIVE_MOUNT=$W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-character-archive-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface MemorySpec {
  id: string;
  owner: 'sable' | 'tor';
  content: string;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  sable: string;
  tor: string;
  quill: string;
  chatSeated: string;
  chatRemoved: string;
  memories: MemorySpec[];
}

/** A one-pixel PNG plus a per-image tail, so each blob has a distinct sha256. */
const PNG_HEAD = [
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
  0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
  0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
  0x42, 0x60, 0x82,
];
/** A pinned embedding-profile id — the column is plain TEXT and no profile row
 *  is needed for the prune's `deleteByEntity` sweep to be measurable. */
const EMBEDDING_PROFILE = 'b7000000-0000-4000-8000-0000000000b7';

function imageBytes(tag: string): Buffer {
  return Buffer.concat([Buffer.from(PNG_HEAD), Buffer.from(`\n${tag}\n`, 'utf-8')]);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'character-archive.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_ARCHIVE_MAIN;
  const mountOut = process.env.QT_FIXTURE_ARCHIVE_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_ARCHIVE_{MAIN,MOUNT} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-archive-fixture-'));
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
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema, MemorySchema } = await import(
    '@/lib/schemas/types'
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
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');
  const { basenameOfRelativePath } = await import('@/lib/photos/keep-image-markdown');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('memories', MemorySchema);

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

  // 1. The owner.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Three characters. Each real `create` mints and scaffolds a vault.
  for (const [id, name] of [
    [spec.sable, 'Sable'],
    [spec.tor, 'Tor'],
    [spec.quill, 'Quill'],
  ] as Array<[string, string]>) {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy: 'llm' } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  const sableVault = (await repos.characters.findByIdRaw(spec.sable))
    ?.characterDocumentMountPointId as string | null;
  const torVault = (await repos.characters.findByIdRaw(spec.tor))
    ?.characterDocumentMountPointId as string | null;
  if (!sableVault || !torVault) throw new Error('character vault(s) not minted');

  // 2b. Tor becomes the NO-VAULT character: null the FK and tear the scaffolded
  //     store down entirely, so nothing dangles in the mount index.
  await rawQuery('UPDATE characters SET characterDocumentMountPointId = NULL WHERE id = ?', [
    spec.tor,
  ]);
  //     ⚠ Content rows are sha256-ADDRESSED and therefore SHARED: Sable's and
  //     Tor's scaffolds write byte-identical managed documents, so they hold the
  //     same `doc_mount_files` row. Deleting content by `fileId IN (… WHERE
  //     mountPointId = tor)` orphans SABLE's links and her vault stops resolving
  //     (`properties.json missing`). Drop the links first, then GC only the
  //     content nothing links to any more.
  midb.prepare('DELETE FROM doc_mount_chunks WHERE mountPointId = ?').run(torVault);
  midb.prepare('DELETE FROM doc_mount_file_links WHERE mountPointId = ?').run(torVault);
  midb.prepare('DELETE FROM doc_mount_folders WHERE mountPointId = ?').run(torVault);
  midb.prepare('DELETE FROM doc_mount_points WHERE id = ?').run(torVault);
  midb
    .prepare('DELETE FROM doc_mount_documents WHERE fileId NOT IN (SELECT fileId FROM doc_mount_file_links)')
    .run();
  midb
    .prepare('DELETE FROM doc_mount_files WHERE id NOT IN (SELECT fileId FROM doc_mount_file_links)')
    .run();

  // 3. The prune material — two text documents (chunked on write) …
  await ensureFolderPath(sableVault, 'Mail');
  await ensureFolderPath(sableVault, 'Conversation Summaries');
  await writeDatabaseDocument(
    sableVault,
    'Mail/letter-01.md',
    '# From the quay\n\nThe sextant is spoken for. Do not write again by the usual hand.\n',
  );
  await writeDatabaseDocument(
    sableVault,
    'Conversation Summaries/chat-a.md',
    '## Summary\n\nSable and Tor argued about the orchids, then did not speak of them.\n',
  );
  // … one wardrobe document saved by the `Wardrobe/` PREFIX (not by the
  //   ten-path list) …
  await writeDatabaseDocument(
    sableVault,
    'Wardrobe/travelling-coat.md',
    '# Travelling coat\n\nOilcloth, brass buttons, one pocket sewn shut.\n',
  );

  // … and two blobs under one folder: `portrait` is kept BY ID (it becomes
  //   defaultImageId), `landscape` is doomed.
  const photosFolder = await ensureFolderPath(sableVault, 'photos');
  const linkIds: Record<string, string> = {};
  for (const tag of ['portrait', 'landscape']) {
    const bytes = imageBytes(tag);
    const relativePath = `photos/${tag}.webp`;
    const { link } = await repos.docMountFileLinks.linkBlobContent({
      mountPointId: sableVault,
      relativePath,
      fileName: basenameOfRelativePath(relativePath),
      folderId: photosFolder,
      originalFileName: `${tag}.png`,
      originalMimeType: 'image/png',
      storedMimeType: 'image/png',
      sha256: sha256OfBuffer(bytes),
      data: bytes,
      description: `${tag} of Sable`,
      extractedText: null,
      extractedTextSha256: null,
      extractionStatus: 'skipped',
    } as never);
    linkIds[tag] = (link as { id: string }).id;
  }
  // `defaultImageId` is the keep-by-id arm. The four default-* FKs are set to
  // NON-NULL values on purpose: the archive commit nulls exactly those four, and
  // a fixture whose columns were already NULL cannot tell a port that nulls them
  // from one that forgets to (the mutation test that caught this is the D24
  // rule doing its job). `defaultPartnerId` points at a REAL character; the
  // other three are plain nullable TEXT with no FK, so pinned ids suffice.
  await rawQuery(
    'UPDATE characters SET defaultImageId = ?, defaultPartnerId = ?, defaultConnectionProfileId = ?, defaultImageProfileId = ?, defaultRoleplayTemplateId = ? WHERE id = ?',
    [
      linkIds.portrait,
      spec.quill,
      'c0000000-0000-4000-8000-0000000000c0',
      'e0000000-0000-4000-8000-0000000000e0',
      'a0000000-0000-4000-8000-0000000000f0',
      spec.sable,
    ],
  );

  // 4. Two chats. `chatSeated` has Sable ACTIVE (the flip arm); `chatRemoved`
  //    has her `removed` — a seat that left before the archive and must stay
  //    `removed` in BOTH directions.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The quay at first light',
      participants: [
        {
          id: '90000000-0000-4000-8000-000000000001',
          type: 'CHARACTER',
          characterId: spec.sable,
          status: 'active',
          controlledBy: 'llm',
          createdAt: TS,
          updatedAt: TS,
        },
        {
          id: '90000000-0000-4000-8000-000000000002',
          type: 'CHARACTER',
          characterId: spec.tor,
          status: 'active',
          controlledBy: 'llm',
          createdAt: TS,
          updatedAt: TS,
        },
      ],
    } as never,
    { id: spec.chatSeated, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The glasshouse, later',
      participants: [
        {
          id: '90000000-0000-4000-8000-000000000003',
          type: 'CHARACTER',
          characterId: spec.sable,
          status: 'removed',
          controlledBy: 'llm',
          createdAt: TS,
          updatedAt: TS,
        },
        {
          id: '90000000-0000-4000-8000-000000000004',
          type: 'CHARACTER',
          characterId: spec.quill,
          status: 'active',
          controlledBy: 'llm',
          createdAt: TS,
          updatedAt: TS,
        },
      ],
    } as never,
    { id: spec.chatRemoved, createdAt: TS, updatedAt: TS } as never,
  );

  // 5. The memories. Sable's three cross-link each other; Tor's points at
  //    Sable's first, so the gate's unlink scrub has something to prove.
  const sableIds = spec.memories.filter((m) => m.owner === 'sable').map((m) => m.id);
  for (const m of spec.memories) {
    const isSable = m.owner === 'sable';
    await repos.memories.create(
      {
        characterId: isSable ? spec.sable : spec.tor,
        aboutCharacterId: isSable ? null : spec.sable,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.content.slice(0, 40),
        keywords: [],
        tags: [],
        importance: 0.6,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: isSable ? sableIds.filter((id) => id !== m.id) : [sableIds[0]],
        reinforcedImportance: 0.6,
      } as never,
      { id: m.id, createdAt: TS, updatedAt: TS },
    );
    await repos.embeddingStatus.markAsEmbedded('MEMORY', m.id, EMBEDDING_PROFILE, spec.userId);
  }

  // 6. Sable's vector index — the prune's `deleteStore` target. Tor's stays.
  for (const [character, ids] of [
    [spec.sable, sableIds],
    [spec.tor, [spec.memories[spec.memories.length - 1].id]],
  ] as Array<[string, string[]]>) {
    await repos.vectorIndices.saveMeta(character, 4);
    await repos.vectorIndices.addEntries(
      ids.map((id) => ({ id, characterId: character, embedding: [0.1, 0.2, 0.3, 0.4] })) as never,
    );
  }

  // 7. Re-pin every live-clock timestamp so the two sides' pre-state is
  //    byte-identical (the store writes and the embedding-status marks all
  //    stamp `Date.now()`).
  for (const [table, cols] of [
    ['doc_mount_points', ['createdAt', 'updatedAt']],
    ['doc_mount_files', ['createdAt', 'updatedAt']],
    ['doc_mount_documents', ['createdAt', 'updatedAt']],
    ['doc_mount_folders', ['createdAt', 'updatedAt']],
    ['doc_mount_file_links', ['createdAt', 'updatedAt', 'lastModified']],
    ['doc_mount_chunks', ['createdAt', 'updatedAt']],
  ] as Array<[string, string[]]>) {
    const sets = cols.map((c) => `"${c}" = ?`).join(', ');
    midb.prepare(`UPDATE "${table}" SET ${sets}`).run(...cols.map(() => TS));
  }
  await rawQuery(
    'UPDATE "embedding_status" SET "createdAt" = ?, "updatedAt" = ?, "embeddedAt" = ?',
    [TS, TS, TS],
  );
  await rawQuery('UPDATE "characters" SET "updatedAt" = ?', [TS]);
  await rawQuery('UPDATE "vector_indices" SET "createdAt" = ?, "updatedAt" = ?', [TS, TS]);

  // 8. The sidecar both differential sides read for the minted ids.
  writeFileSync(
    mainOut + '.meta.json',
    JSON.stringify(
      {
        sableVaultMountPointId: sableVault,
        portraitLinkId: linkIds.portrait,
        landscapeLinkId: linkIds.landscape,
      },
      null,
      2,
    ) + '\n',
  );

  await closeDatabase();
  closeMountIndexSQLiteClient();
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`character-archive fixture written:\n  ${mainOut}\n  ${mountOut}\n`);
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exit(1);
});
