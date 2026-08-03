/**
 * Fixture builder for the W4.1d5 `send_mail` / `list_email` tool differential
 * (mail_carina_tools_equivalence — the mail half; ask_carina is DB-free).
 *
 * Bakes a shared state across main + mount-index so BOTH v4 and the Rust port read
 * it identically; each case runs on its own fresh copy (send_mail WRITES). Seeds,
 * via the REAL repos + the REAL post-office `deliverLetter`:
 *   - Three characters (sender Friday, recipient Aurora, empty-mailbox Jeeves), each
 *     minting a linked vault via repos.characters.create.
 *   - One letter in the SENDER's (Friday's) mailbox (for the in_reply_to test).
 *   - One older letter in the RECIPIENT's (Aurora's) mailbox (so a send-then-list
 *     shows two letters, newest first).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-mail-carina-tools-fixture.ts
 *
 * Echoes the minted vault ids to <main>.meta.json.
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedLetter {
  path: string;
  fromName: string;
  fromCharacterId: string;
  sentAt: string;
  body: string;
}
interface Spec {
  testPepperBase64: string;
  senderId: string;
  recipientId: string;
  emptyId: string;
  sender: Record<string, unknown>;
  recipient: Record<string, unknown>;
  empty: Record<string, unknown>;
  seedLetterInSenderMailbox: SeedLetter;
  seedLetterInRecipientMailbox: SeedLetter;
}

const PINNED_TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'mail-carina-tools.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_TMP_MAIN;
  const mountOut = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mail-fixture-build-'));
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { deliverLetter } = await import('@/lib/post-office/mailbox');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );

  const repos = getRepositories();

  const create = async (id: string, data: Record<string, unknown>): Promise<string> => {
    await repos.characters.create(data as never, {
      id,
      createdAt: PINNED_TS,
      updatedAt: PINNED_TS,
    } as never);
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`vault not minted for ${id}`);
    return vault;
  };

  const senderVault = await create(spec.senderId, spec.sender);
  const recipientVault = await create(spec.recipientId, spec.recipient);
  await create(spec.emptyId, spec.empty);

  // Seed one letter into the SENDER's mailbox (for the in_reply_to test), and one
  // older letter into the RECIPIENT's mailbox (so a send-then-list shows two).
  const seedSender = spec.seedLetterInSenderMailbox;
  await deliverLetter({
    recipientVaultId: senderVault,
    fromName: seedSender.fromName,
    fromCharacterId: seedSender.fromCharacterId,
    sentAt: seedSender.sentAt,
    body: seedSender.body,
    inReplyTo: null,
  } as never);

  const seedRecipient = spec.seedLetterInRecipientMailbox;
  await deliverLetter({
    recipientVaultId: recipientVault,
    fromName: seedRecipient.fromName,
    fromCharacterId: seedRecipient.fromCharacterId,
    sentAt: seedRecipient.sentAt,
    body: seedRecipient.body,
    inReplyTo: null,
  } as never);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  writeFileSync(mainOut + '.meta.json', JSON.stringify({ senderVault, recipientVault }));
  process.stderr.write(
    `built mail-carina fixtures: main=${mainOut} mount=${mountOut} senderVault=${senderVault} recipientVault=${recipientVault}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`mail-carina fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
