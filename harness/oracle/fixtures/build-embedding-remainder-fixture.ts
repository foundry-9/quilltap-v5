/**
 * SEED fixture builder for the embedding-family remainder (P4.6BM — v4's
 * `handleConversationRender`, `reconcileConversationRendering` and
 * `handleEmbeddingReindexAll`).
 *
 * Bakes the seed both differential sides start from, across BOTH databases, via
 * v4's REAL repo `create` calls (every id and timestamp pinned).
 *
 * What is in it, and which arm each row exists for:
 *
 *   - THREE characters (Aria / Bram, llm; Eve, user-controlled), each vault
 *     provisioned by v4's own `characters.create`. The render's display-name map
 *     resolves through them; reindex phase 2 walks their memories.
 *   - TWO embedding profiles: an 8-dimension OPENAI profile (the
 *     `mismatched-dim` target dim) FIRST, then the BUILTIN default (no
 *     `dimensions`, so partial scope must THROW for it). The order is
 *     deliberate: the render's `find(p => p.isDefault) || profiles[0]` is only
 *     observable when the default is NOT also the first row.
 *   - SIX chats, one per reconcile/render arm:
 *       `unrendered`          NULL renderedMarkdown + real messages → arm A.
 *                             Its cast covers all three display-name paths: a
 *                             user participant WITH a character, an llm
 *                             participant, and a user participant with NO
 *                             character (→ "User"). Carries a `system` event and
 *                             an assistant message that strips to nothing, and
 *                             spans two days (the cross-day span line).
 *       `rendered-unembedded` rendered + one un-embedded chunk → arm B, and the
 *                             upsert-preserves-embeddings case.
 *       `oversize-chunk`      rendered + an un-embedded OVERSIZE chunk → picked
 *                             up by NEITHER arm (v4's `BETWEEN 1 AND cap`).
 *       `empty`               NULL renderedMarkdown, no messages → arm A's
 *                             EXISTS fails; also the render's no-events return.
 *       `system-only`         NULL renderedMarkdown, only a `system` row → arm A
 *                             excluded (its EXISTS demands type='message' AND a
 *                             USER/ASSISTANT role).
 *       `healthy`             rendered + embedded chunk → neither arm.
 *     …plus FOUR P4.D25 chats whose activity is INSIDE the retention window, so
 *     the reconcile's new staleness gate lets them through while every chat
 *     above is skipped as cold-tiered: a fresh arm-(A) chat (enqueued), a fresh
 *     arm-(A) chat that already has a PENDING render (reused), a fresh chat
 *     whose only un-embedded chunk is FAILED for the DEFAULT profile (excluded
 *     from arm (B) entirely — and with no played messages, so it exercises the
 *     gate's `chats.updatedAt` fallback), and its twin whose FAILED row names
 *     the OTHER profile (still selected — the exclusion is per-profile).
 *   - FOUR conversation chunks (embedded / un-embedded / oversize / embedded).
 *   - THREE memories across two characters, two of them carrying an 8-dim
 *     embedding so `mismatched-dim` has rows to SKIP as well as rows to take.
 *   - A vector-index meta row + one entry for Aria plus an ORPHAN entry, so
 *     full-scope `deleteStore` is a visible change.
 *   - ONE help doc that is NOT on disk, so the reindex's `syncHelpDocs` prune is
 *     visible; the rest of `help_docs` comes from the committed help tree the
 *     handler syncs (`fixtures/help-sync/help/`, shared with the P4.1b family).
 *   - THREE seed jobs: a DEAD embed for the un-embedded chunk (which must NOT
 *     block re-enqueue), a PENDING render for `rendered-unembedded` (the
 *     reconcile's `reused` counter), and a PENDING embed (partial scope must
 *     leave it alone; full scope cancels it).
 *
 * P4.d27 (v4 `7391404e`) adds the non-conforming half of the corpus — the state
 * v4's outage was actually in, which a conforming fixture cannot express:
 *
 *   - A FOURTH character (Nadia) owned by the SECOND user, with a memory, so the
 *     reindex fan-out's not-user-scoped enumeration is pinned; plus a memory
 *     whose `characterId` has NO `characters` row at all — the vault-unavailable
 *     shape the old `characters.findByUserId` fan-out silently dropped.
 *   - `legacyDim` / `legacyFormat` on any embedding-bearing seed: after every
 *     create, that row's BLOB is rewritten to a vector of another width, in
 *     either the headerless raw Float32 layout (the TF-IDF era) or the current
 *     quantized header. The two take different branches of `EMBEDDING_DIM_SQL`,
 *     so — like v4's own test — the corpus carries both. Applied to memories
 *     (one per format), conversation chunks (stale / live / ORPHAN), the one
 *     help doc whose id is pinned, two extra vector-index entries, and mount
 *     chunks.
 *   - TWO explicit document mount points, one ENABLED and one DISABLED, with
 *     pinned ids (the per-character vault mounts exist but their ids are minted),
 *     and five chunks across them: conforming, NULL, non-conforming, FAILED, and
 *     one behind the disabled door.
 *   - FAILED `embedding_status` rows for the 8-dimension profile covering all
 *     four entity types, so both the reindex's `mismatched-dim` skips and the
 *     reconcile's NOT_FAILED exclusions are observable.
 *   - Aria's index meta dim comes from the spec (4, not the corpus width), so the
 *     reconcile's meta snap is a visible change.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<the v5 worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-er-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-er-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-embedding-remainder-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

/** A vector stored in one of the two LEGACY on-disk widths/formats (P4.d27). */
interface LegacyBlob {
  /** Component count to write — anything other than `embedDim` is non-conforming. */
  legacyDim?: number;
  /** `raw` = headerless Float32 (the TF-IDF era); `quantized` = the current header. */
  legacyFormat?: 'raw' | 'quantized';
}

interface CharacterSeed {
  id: string;
  name: string;
  controlledBy: string;
  /** Defaults to the corpus `userId`; set to prove a read is not user-scoped. */
  userId?: string;
}
interface ProfileSeed {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  dimensions: number | null;
  truncateToDimensions: number | null;
  isDefault: boolean;
}
interface ParticipantSeed {
  id: string;
  /** Always a UUID — v4's schema forbids null. The "User" display-name fallback
   *  is reached by pointing at a character row that does NOT exist. */
  characterId: string;
  controlledBy: string;
}
/** A `message` row carries role/content/participantId; a `system` row carries
 *  `systemEventType` + `description` instead (v4's `ChatEvent` union). */
interface MessageSeed {
  id: string;
  type: string;
  role?: string;
  content?: string;
  participantId?: string | null;
  systemSender?: string;
  isSilentMessage?: boolean;
  systemEventType?: string;
  description?: string;
  createdAt: string;
}
interface ChatSeed {
  id: string;
  key: string;
  title: string;
  renderedMarkdown: string | null;
  chatType?: string;
  /** P4.D25: the staleness gate's fallback when a chat has no played messages.
   *  Defaults to the corpus `seedTimestamp` (which is stale by design). */
  updatedAt?: string;
  participants: ParticipantSeed[];
  messages: MessageSeed[];
}
interface ConnectionSeed {
  id: string;
  userId: string;
  name: string;
  provider: string;
  modelName: string;
}
interface ChatSettingsSeed {
  id: string;
  userId: string;
  cheapLLMSettings: Record<string, unknown>;
}
interface ChunkSeed extends LegacyBlob {
  id: string;
  chatId: string;
  interchangeIndex: number;
  content: string;
  participantNames: string[];
  embedded: boolean;
}
interface MemorySeed extends LegacyBlob {
  id: string;
  characterId: string;
  summary: string;
  content: string;
  embedded: boolean;
}
interface HelpDocSeed extends LegacyBlob {
  id: string;
  title: string;
  path: string;
  url: string;
  content: string;
  contentHash: string;
  embedded: boolean;
}
/** P4.d27 — a document mount point in the mount-index DB. */
interface MountPointSeed {
  id: string;
  name: string;
  enabled: boolean;
}
/** P4.d27 — a document mount chunk in the mount-index DB. */
interface MountChunkSeed extends LegacyBlob {
  id: string;
  mountPointId: string;
  chunkIndex: number;
  content: string;
  embedded: boolean;
}
/** P4.d27 — an extra vector-index entry, written at a legacy width. */
interface VectorEntrySeed extends LegacyBlob {
  id: string;
}
interface StatusSeed {
  id: string;
  entityType: string;
  entityId: string;
  profileId: string;
  status: string;
}
interface JobSeed {
  id: string;
  name: string;
  type: string;
  status: string;
  payload: Record<string, unknown>;
  priority: number;
  attempts: number;
  createdAt: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  secondUserId: string;
  connectionProfiles: ConnectionSeed[];
  chatSettings: ChatSettingsSeed;
  seedTimestamp: string;
  embedDim: number;
  oversizeChars: number;
  characters: CharacterSeed[];
  embeddingProfiles: ProfileSeed[];
  chats: ChatSeed[];
  conversationChunks: ChunkSeed[];
  oversizeChunkId: string;
  memories: MemorySeed[];
  seedVectorEntryMemoryId: string;
  orphanVectorEntryId: string;
  /** Aria's index meta dim — DELIBERATELY not `embedDim`, so the reconcile snaps it. */
  seedVectorIndexDimensions: number;
  extraVectorEntries: VectorEntrySeed[];
  docMountPoints: MountPointSeed[];
  docMountChunkLinkId: string;
  docMountChunks: MountChunkSeed[];
  helpDocSeeds: HelpDocSeed[];
  statusRows: StatusSeed[];
  jobs: JobSeed[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'embedding-remainder.json'), 'utf8')
  ) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-er-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  // Materialize the mount-index content tables (the vault scaffold needs them).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
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

  const { float32ToBlob, float32ToBlobRaw } = await import('@/lib/embedding/float32-conversion');

  const TS = spec.seedTimestamp;
  const embedding = () => new Float32Array(Array.from({ length: spec.embedDim }, (_, i) => (i === 0 ? 1 : 0)));

  /**
   * P4.d27 — overwrite a row's `embedding` BLOB with a vector at a LEGACY width,
   * in either on-disk format. The repositories always write the current
   * quantized format at the corpus width, so a non-conforming corpus can only be
   * built by rewriting the cell afterwards. `raw` reproduces the headerless
   * TF-IDF-era layout that started v4's outage; `quantized` is the same width in
   * the current header format, and the two take DIFFERENT branches of
   * EMBEDDING_DIM_SQL — v4's own test covers both, so this corpus does too.
   */
  const legacyRewrites: Array<{ db: 'main' | 'mount'; table: string; seed: LegacyBlob & { id: string } }> = [];
  const applyLegacyRewrites = () => {
    for (const { db, table, seed } of legacyRewrites) {
      if (!seed.legacyDim) continue;
      const vec = new Float32Array(
        Array.from({ length: seed.legacyDim }, (_, i) => (i === 0 ? 1 : 0))
      );
      const blob = seed.legacyFormat === 'quantized' ? float32ToBlob(vec) : float32ToBlobRaw(vec);
      const handle = db === 'main' ? getRawDatabase() : getRawMountIndexDatabase();
      if (!handle) throw new Error(`${db} DB handle unavailable for the legacy rewrite`);
      const changed = handle
        .prepare(`UPDATE "${table}" SET embedding = ? WHERE id = ?`)
        .run(blob, seed.id).changes;
      if (changed !== 1) {
        throw new Error(`legacy rewrite matched ${changed} rows for ${table}.${seed.id}`);
      }
    }
  };

  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never
  );
  // The SECOND user exists only so `queue-memories` has a caller with no
  // chat-settings row (the resolver's `null` → 400 arm) and so one connection
  // profile can be owned by somebody else (the ownership check).
  await repos.users.create(
    { username: 'stranger', email: null, name: 'A Stranger' } as never,
    { id: spec.secondUserId, createdAt: TS, updatedAt: TS } as never
  );

  for (const c of spec.connectionProfiles) {
    await repos.connections.create(
      { userId: c.userId, name: c.name, provider: c.provider, modelName: c.modelName } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never
    );
  }

  // `defaultCheapProfileId` deliberately points at the OTHER user's profile, so
  // v4's resolver finds the row, rejects it on ownership, and falls through to
  // the USER_DEFINED branch — both branches exercised by one seed.
  await repos.chatSettings.create(
    {
      userId: spec.chatSettings.userId,
      cheapLLMSettings: spec.chatSettings.cheapLLMSettings,
    } as never,
    { id: spec.chatSettings.id, createdAt: TS, updatedAt: TS } as never
  );

  for (const c of spec.characters) {
    await repos.characters.create(
      { name: c.name, userId: c.userId ?? spec.userId, controlledBy: c.controlledBy } as never,
      { id: c.id }
    );
  }

  for (const p of spec.embeddingProfiles) {
    await repos.embeddingProfiles.create(
      {
        userId: spec.userId,
        name: p.name,
        provider: p.provider,
        modelName: p.modelName,
        dimensions: p.dimensions,
        truncateToDimensions: p.truncateToDimensions,
        isDefault: p.isDefault,
      } as never,
      { id: p.id, createdAt: TS, updatedAt: TS }
    );
  }

  for (const chat of spec.chats) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        chatType: chat.chatType ?? 'salon',
        contextSummary: null,
        renderedMarkdown: chat.renderedMarkdown,
        participants: chat.participants.map((p, i) => ({
          id: p.id,
          type: 'CHARACTER',
          characterId: p.characterId,
          controlledBy: p.controlledBy,
          displayOrder: i,
          status: 'active',
          connectionProfileId: null,
          hasHistoryAccess: false,
          createdAt: TS,
          updatedAt: TS,
        })),
        tags: [],
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: chat.updatedAt ?? TS } as never
    );
    for (const m of chat.messages) {
      const event =
        m.type === 'system'
          ? {
              type: 'system',
              id: m.id,
              systemEventType: m.systemEventType,
              description: m.description,
              createdAt: m.createdAt,
            }
          : { ...m, attachments: [] };
      await repos.chats.addMessage(chat.id, event as never);
    }
  }
  // `addMessage` bumps `updatedAt`; re-pin it so both sides read identical rows
  // (and so the render's `- Last Updated:` header line is deterministic).
  for (const chat of spec.chats) {
    await repos.chats.update(chat.id, { updatedAt: chat.updatedAt ?? TS } as never);
  }

  for (const c of spec.conversationChunks) {
    const content = c.id === spec.oversizeChunkId ? 'x'.repeat(spec.oversizeChars) : c.content;
    await repos.conversationChunks.create(
      {
        chatId: c.chatId,
        interchangeIndex: c.interchangeIndex,
        content,
        participantNames: c.participantNames,
        messageIds: [],
        embedding: c.embedded ? embedding() : null,
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS }
    );
    legacyRewrites.push({ db: 'main', table: 'conversation_chunks', seed: c });
  }

  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        occurredAt: null,
        keywords: [],
        tags: [],
        importance: 0.4,
        embedding: m.embedded ? embedding() : null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: 0.4,
      } as never,
      { id: m.id, createdAt: TS, updatedAt: TS }
    );
    legacyRewrites.push({ db: 'main', table: 'memories', seed: m });
  }

  // Aria's vector index: a meta row with a DELIBERATELY wrong dim plus one real
  // entry and one ORPHAN entry, so full-scope `deleteStore` is a visible change
  // and partial scope's leave-it-alone is equally visible. P4.d27 adds two
  // entries at a LEGACY width (one per on-disk format) that the boot dimension
  // reconcile DELETEs outright, and pins the meta dim from the spec so the
  // reconcile's meta snap is a visible change too.
  const vectors = new VectorIndicesRepository();
  const ariaId = spec.characters[0].id;
  await vectors.saveMeta(ariaId, spec.seedVectorIndexDimensions);
  await vectors.addEntry({
    id: spec.seedVectorEntryMemoryId,
    characterId: ariaId,
    embedding: embedding(),
  });
  await vectors.addEntry({
    id: spec.orphanVectorEntryId,
    characterId: ariaId,
    embedding: embedding(),
  });
  for (const e of spec.extraVectorEntries) {
    await vectors.addEntry({ id: e.id, characterId: ariaId, embedding: embedding() });
    legacyRewrites.push({ db: 'main', table: 'vector_entries', seed: e });
  }

  // P4.d27 — two explicit document mount points (one enabled, one disabled) and
  // their chunks. Vault mounts exist already (one per character), but their ids
  // are minted at build time; these are pinned so the corpus can name them.
  for (const mp of spec.docMountPoints) {
    await repos.docMountPoints.create(
      {
        name: mp.name,
        basePath: '',
        mountType: 'database',
        storeType: 'documents',
        includePatterns: ['*.md'],
        excludePatterns: [],
        enabled: mp.enabled,
        scanStatus: 'idle',
        conversionStatus: 'idle',
        fileCount: 0,
        chunkCount: 0,
        totalSizeBytes: 0,
      } as never,
      { id: mp.id, createdAt: TS, updatedAt: TS }
    );
  }
  for (const c of spec.docMountChunks) {
    await repos.docMountChunks.create(
      {
        linkId: spec.docMountChunkLinkId,
        mountPointId: c.mountPointId,
        chunkIndex: c.chunkIndex,
        content: c.content,
        tokenCount: c.content.split(/\s+/).length,
        headingContext: null,
        embedding: c.embedded ? embedding() : null,
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS }
    );
    legacyRewrites.push({ db: 'mount', table: 'doc_mount_chunks', seed: c });
  }

  for (const d of spec.helpDocSeeds) {
    await repos.helpDocs.create(
      {
        title: d.title,
        path: d.path,
        url: d.url,
        content: d.content,
        contentHash: d.contentHash,
        embedding: d.embedded ? embedding() : null,
      } as never,
      { id: d.id, createdAt: TS, updatedAt: TS }
    );
    legacyRewrites.push({ db: 'main', table: 'help_docs', seed: d });
  }

  for (const s of spec.statusRows) {
    await repos.embeddingStatus.create(
      {
        userId: spec.userId,
        entityType: s.entityType,
        entityId: s.entityId,
        profileId: s.profileId,
        status: s.status,
        embeddedAt: s.status === 'EMBEDDED' ? TS : null,
        error: null,
      } as never,
      { id: s.id, createdAt: TS }
    );
  }

  for (const j of spec.jobs) {
    await repos.backgroundJobs.create(
      {
        userId: spec.userId,
        type: j.type,
        status: j.status,
        payload: j.payload,
        priority: j.priority,
        attempts: j.attempts,
        maxAttempts: 3,
        lastError: j.status === 'DEAD' ? 'fetch failed' : null,
        scheduledAt: j.createdAt,
        startedAt: null,
        completedAt: null,
      } as never,
      { id: j.id, createdAt: j.createdAt, updatedAt: j.createdAt }
    );
  }

  // LAST: rewrite every seed that asked for a legacy width/format. The repos
  // always write the current quantized format at the corpus width, so this is
  // the only way to bake a non-conforming corpus — and it must run after every
  // create, since a later write would overwrite the cell.
  applyLegacyRewrites();

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built embedding-remainder fixture: ${outMain} + ${outMount} ` +
      `(${spec.chats.length} chats, ${spec.conversationChunks.length} chunks, ` +
      `${spec.memories.length} memories, ${spec.helpDocSeeds.length} help docs, ` +
      `${spec.statusRows.length} status rows, ${spec.jobs.length} jobs, ` +
      `${spec.docMountPoints.length} mount points, ${spec.docMountChunks.length} mount chunks, ` +
      `${legacyRewrites.filter((r) => r.seed.legacyDim).length} legacy-width vectors)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`embedding-remainder fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
