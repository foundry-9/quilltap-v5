/**
 * P4.6ak route-surface fixture builder (lane A) — the committed test-pepper
 * substrate for the text-replacements + chat-get-background differential
 * (`text_replacements_routes_equivalence`) AND the web-edge integration test
 * (`crates/quilltap-web/tests/text_replacements_web_routes.rs`).
 *
 * Bakes via v4's REAL machinery so the DDL + row shapes are production-identical:
 *   - `text_replacement_rules` is created by v4's OWN `ensureCollection(...,
 *     TextReplacementRuleSchema)` (the migration table is NOT in fresh
 *     `generateDDL` — the `folders`/`terminal_sessions` precedent), then the three
 *     seed rows via the real `TextReplacementRulesRepository.create` (ids +
 *     timestamps pinned).
 *   - the background image `files` row + three chats (bg / no-bg / dangling-file)
 *     via the real `repos.files.create` / `repos.chats.create` (ids pinned).
 *   - the mount-index sibling gets the seven doc_mount_* tables via `generateDDL`
 *     (+ the hand-DDL `doc_mount_blobs`) so the get-background link-summary read
 *     resolves to the empty summary (the bg image is never written to a mount).
 *
 * Two encrypted partitions land at QT_FIXTURE_TR_MAIN / QT_FIXTURE_TR_MOUNT.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TR_MAIN=$W/crates/quilltap-web/tests/fixtures/text-replacements-main.db \
 *   QT_FIXTURE_TR_MOUNT=$W/crates/quilltap-web/tests/fixtures/text-replacements-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-text-replacements-web-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface RuleSeed {
  id: string;
  fromText: string;
  toText: string;
  caseSensitive: boolean;
  enabled: boolean;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  rules: RuleSeed[];
  backgroundFileId: string;
  backgroundFileSha256: string;
  backgroundFilename: string;
  missingFileId: string;
  chatWithBackgroundId: string;
  chatNoBackgroundId: string;
  chatMissingFileId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'text-replacements-web.json'), 'utf8'),
  ) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_TR_MAIN;
  const mountOut = process.env.QT_FIXTURE_TR_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_TR_MAIN and QT_FIXTURE_TR_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tr-fixture-'));
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
  const { ChatMetadataSchema, FileEntrySchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { TextReplacementRuleSchema } = await import('@/lib/schemas/text-replacement.types');
  const { TextReplacementRulesRepository } = await import(
    '@/lib/database/repositories/text-replacement-rules.repository'
  );
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
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('text_replacement_rules', TextReplacementRuleSchema);

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

  // 1. The single user.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. The seed text-replacement rules (ids + timestamps pinned).
  const trr = new TextReplacementRulesRepository();
  for (const row of spec.rules) {
    await trr.create(
      {
        fromText: row.fromText,
        toText: row.toText,
        caseSensitive: row.caseSensitive,
        enabled: row.enabled,
        sortOrder: row.sortOrder,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt },
    );
  }

  // 3. The background image file (sha256 set → the resolved arm computes a link
  //    summary; the bytes are never written to a mount → empty summary).
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: spec.backgroundFileSha256,
      originalFilename: spec.backgroundFilename,
      mimeType: 'image/png',
      size: 2048,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/${spec.backgroundFilename}`,
      width: 64,
      height: 48,
    } as never,
    { id: spec.backgroundFileId, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. Three chats: bg set / bg null / bg pointing at a dangling file id.
  const mkChat = (id: string, title: string, storyBackgroundImageId: string | null) =>
    repos.chats.create(
      {
        userId: spec.userId,
        title,
        chatType: 'salon',
        contextSummary: null,
        storyBackgroundImageId,
        participants: [],
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkChat(spec.chatWithBackgroundId, 'Chat With Background', spec.backgroundFileId);
  await mkChat(spec.chatNoBackgroundId, 'Chat No Background', null);
  await mkChat(spec.chatMissingFileId, 'Chat Dangling Background', spec.missingFileId);

  await closeDatabase();
  closeMountIndexSQLiteClient();

  process.stderr.write(
    `built text-replacements web fixture: ${mainOut} (+${mountOut}) — ` +
      `${spec.rules.length} rules, 3 chats, 1 bg file\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
