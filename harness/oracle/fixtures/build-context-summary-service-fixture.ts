/**
 * Tier-3 SEED fixture builder — the context-summary service half (v4
 * `generateContextSummary` / `invalidateContextSummaryIfMessageCovered` /
 * `checkAndGenerateSummaryIfNeeded`, `lib/chat/context-summary.ts`), ported as
 * `quilltap-core::services::context_summary`.
 *
 * Round-3 Group 7 extends this to prove the fold's cross-subsystem vault seams —
 * the Commonplace-Book vault MIRROR (`writeConversationSummaryToVaults`) and the
 * relevant-conversations REFRESH (`refreshRelevantConversationsOnFold`) — which the
 * ported `RealContextSummarySeams` now runs LIVE (they were no-ops). So this builds
 * TWO databases:
 *   - the MAIN db (QT_FIXTURE_OUT): the 11 chats via v4's REAL `repos.chats.create`
 *     + `addMessages` (the fold/gate/title corpus), plus ONE fully provisioned
 *     character (`aa…0001`, the participant of the `fold_regular` chat) so its
 *     vault exists for the mirror + refresh;
 *   - the MOUNT-INDEX db (QT_FIXTURE_MOUNT_OUT): that character's vault, plus a
 *     PRE-SEEDED prior-conversation summary under `Conversation Summaries/` whose
 *     one chunk carries a CANNED embedding — the corpus the refresh's semantic
 *     search matches (the query embedding is canned to the same unit vector on both
 *     sides). The other four `generate` chats' characters are NOT provisioned, so
 *     their mirror/refresh no-op identically on both sides.
 *
 * P4.36 adds what the fold-time EPISODE pass needs to be comparable: the three
 * collections v4 would create on demand at the first `createMemoryWithGate`
 * (`memories`, `vector_indices`, `vector_entries` — the Rust port creates no tables
 * on the fly, the same reason `background_jobs` was already materialized here), and
 * ONE seeded per-turn fragment memory inside `fold_regular`'s folded window so the
 * pass's `relatedMemoryIds` linking is exercised rather than trivially empty.
 *
 * `reindexSingleFile` (run by `repos.characters.create` here and by
 * `writeConversationSummaryToVaults` in the ops) creates chunks with NULL
 * embeddings and does NOT enqueue jobs, so `background_jobs` is untouched by the
 * mirror; the differential pins the resulting link `chunkCount` and excludes
 * `doc_mount_chunks`. The canned chunk embedding is written by a direct UPDATE.
 *
 * Both the oracle and the Rust port then COPY these files and run the same op
 * sequence — which mints new ids/timestamps — so the seed rows are byte-identical
 * and only the service's own effect differs.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-ctxsum-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ctxsum-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-context-summary-service-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface MessageSeed {
  id: string;
  role?: 'USER' | 'ASSISTANT';
  content: string;
  systemSender?: string;
  summaryAnchor?: { compactionGeneration: number };
}
interface ChatSeed {
  id: string;
  title: string;
  chatType?: string;
  isDangerousChat?: boolean;
  conciergeOverride?: 'OFF';
  contextSummary?: string | null;
  compactionGeneration?: number;
  lastSummaryTurn?: number;
  lastFullRebuildTurn?: number;
  lastRenameCheckInterchange?: number;
  summaryAnchorMessageIds?: string[];
  participants: Array<Record<string, unknown>>;
  messages: MessageSeed[];
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  chats: ChatSeed[];
}

// Round-3 Group 7: the `fold_regular` chat's single participant character —
// provisioned with a real vault so the fold's mirror + refresh have somewhere to
// write / search. Its id MUST equal that chat's participant `characterId`.
const VAULT_CHARACTER_ID = 'aa000000-0000-4000-8000-000000000001';
const VAULT_CHARACTER_NAME = 'Vaulted A';
// A PRIOR conversation summary pre-seeded into the vault (a DIFFERENT chat, so the
// refresh — which excludes the current chat — surfaces it).
const PRIOR_CHAT_ID = 'dd000000-0000-4000-8000-0000000000f1';
const PRIOR_CHAT_TITLE = 'Prior Chat X';
const PRIOR_SUMMARY_BODY = 'A prior discussion about airships and clockwork resolutions.';
// P4.36: the seeded per-turn fragment the fold-episode pass links to. Its
// `sourceMessageId` MUST be a message inside `fold_regular`'s folded window
// (`c…0001`'s messages), which is what `findByCharacterAndSourceMessageIds`
// matches on.
const FRAGMENT_MEMORY_ID = 'ff000000-0000-4000-8000-0000000000a1';
const FRAGMENT_CHAT_ID = 'c0000001-0000-4000-8000-000000000001';
const FRAGMENT_SOURCE_MESSAGE_ID = 'dd000001-0000-4000-8000-000000000015';
// The unit vector the refresh query is canned to on BOTH sides (dim 8, [1,0,…]).
function cannedEmbeddingBuffer(): Buffer {
  const buf = Buffer.alloc(8 * 4);
  buf.writeFloatLE(1.0, 0);
  return buf;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'context-summary-service-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  const mountOut = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!out || !mountOut) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must both point at .db files');
  }
  for (const f of [out, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = f + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ctxsum-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');
  const { BackgroundJobSchema, MemorySchema } = await import('@/lib/schemas/types');
  const { VectorIndexMetaSchema, VectorEntryRowSchema } = await import(
    '@/lib/schemas/vector-indices.types'
  );
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { writeConversationSummaryToVaults } = await import(
    '@/lib/file-storage/conversation-summary-vault-bridge'
  );

  await initializeDatabase();
  // Materialize the background_jobs table so both sides open a fixture that
  // already has it (the title-checkpoint enqueue writes here; the Rust port
  // does not create tables on the fly).
  await ensureCollection('background_jobs', BackgroundJobSchema);
  // P4.36: same reason, for the fold-time episode pass's writes. v4 would create
  // `memories` (and the gate's `vector_indices`) on demand at the first
  // `createMemoryWithGate`; the Rust port does not, so seeding them is what makes
  // the two sides comparable rather than a table-exists-vs-not difference standing
  // in for the write diff.
  await ensureCollection('memories', MemorySchema);
  await ensureCollection('vector_indices', VectorIndexMetaSchema);
  await ensureCollection('vector_entries', VectorEntryRowSchema);

  // MOUNT-INDEX db: materialize every store table the provision/write path touches,
  // via v4's own generated DDL (idempotent).
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
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
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
  const repo = new ChatsRepository();
  const TS = spec.seedTimestamp;

  // Provision the fold_regular chat's participant character with a real vault
  // (pinned id). The other four generate chats' characters stay unprovisioned →
  // their mirror/refresh no-op on both sides.
  await repos.characters.create(
    {
      name: VAULT_CHARACTER_NAME,
      userId: spec.userId,
      aliases: [],
      controlledBy: 'llm',
    } as never,
    { id: VAULT_CHARACTER_ID }
  );

  for (const c of spec.chats) {
    await repo.create(
      {
        userId: spec.userId,
        title: c.title,
        participants: c.participants,
        chatType: c.chatType ?? 'salon',
        isDangerousChat: c.isDangerousChat ?? null,
        conciergeOverride: c.conciergeOverride ?? null,
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS }
    );

    if (c.messages.length > 0) {
      await repo.addMessages(
        c.id,
        c.messages.map((m) => {
          const base: Record<string, unknown> = {
            type: 'message',
            id: m.id,
            role: m.role ?? 'ASSISTANT',
            content: m.content,
            createdAt: TS,
          };
          if (m.systemSender !== undefined) base.systemSender = m.systemSender;
          if (m.summaryAnchor !== undefined) base.summaryAnchor = m.summaryAnchor;
          return base;
        }) as never
      );
    }

    // Reset the summary-cadence columns + metadata to the pinned seed so the
    // starting state is fully deterministic.
    await repo.update(c.id, {
      contextSummary: c.contextSummary ?? null,
      compactionGeneration: c.compactionGeneration ?? 0,
      lastSummaryTurn: c.lastSummaryTurn ?? 0,
      lastFullRebuildTurn: c.lastFullRebuildTurn ?? 0,
      lastRenameCheckInterchange: c.lastRenameCheckInterchange ?? 0,
      summaryAnchorMessageIds: c.summaryAnchorMessageIds ?? [],
      lastMessageAt: null,
      updatedAt: TS,
    } as never);
  }

  // P4.36: a per-turn FRAGMENT memory sitting inside `fold_regular`'s folded
  // window, so the episode pass's LINK half (`relatedMemoryIds`, written on both
  // the episode and the fragment) is exercised instead of trivially empty. Written
  // through v4's REAL repo with a pinned id. It gets NO `vector_entries` row, so
  // the gate's near-duplicate search never sees it — only
  // `findByCharacterAndSourceMessageIds` does — and every gate decision in the
  // corpus stays what it would have been without it.
  await repos.memories.create(
    {
      characterId: VAULT_CHARACTER_ID,
      chatId: FRAGMENT_CHAT_ID,
      content: 'Vaulted A checked the ropes twice before the run began.',
      summary: 'checked the ropes twice',
      keywords: ['ropes'],
      tags: [],
      importance: 0.4,
      source: 'AUTO',
      sourceMessageId: FRAGMENT_SOURCE_MESSAGE_ID,
    } as never,
    { id: FRAGMENT_MEMORY_ID, createdAt: TS, updatedAt: TS }
  );

  // Pre-seed a PRIOR conversation summary into the vault (a different chat), so
  // the refresh's semantic search — which excludes the current chat — surfaces it.
  await writeConversationSummaryToVaults({
    chatId: PRIOR_CHAT_ID,
    chatTitle: PRIOR_CHAT_TITLE,
    summary: PRIOR_SUMMARY_BODY,
    summaryGeneration: 1,
    participantCharacterIds: [VAULT_CHARACTER_ID],
    messageCount: 5,
    firstMessageAt: TS,
    lastMessageAt: TS,
    updatedAt: TS,
  } as never);

  // Give every seeded chunk the canned unit embedding, so the refresh's canned
  // query vector matches (cosine 1.0). Only the seeded chunk under
  // `Conversation Summaries/` is ever searched (the prefix filter), so touching
  // the provisioning chunks too is harmless.
  midb.prepare('UPDATE doc_mount_chunks SET embedding = ?').run(cannedEmbeddingBuffer());

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built context-summary-service fixtures: main=${out} mount=${mountOut} ` +
      `(${spec.chats.length} chats, 1 vaulted character + 1 seeded prior summary)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`context-summary-service fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
