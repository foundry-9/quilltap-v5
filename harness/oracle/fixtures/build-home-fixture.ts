/**
 * P4.6au fixture builder (lane A) — the committed test-pepper substrate for the
 * home-dashboard differential (`home_routes_equivalence`). Everything is baked
 * via v4's REAL repos so the DDL + row shapes are production-identical.
 *
 * ── TWO FILES, ONE FAMILY ────────────────────────────────────────────────────
 *   - `home-main.db`  — users / projects (slim rows) / files / chats+messages /
 *                       characters (slim rows).
 *   - `home-mount.db` — the mount-index sibling: every character's vault (minted
 *                       by the REAL `repos.characters.create` — the P4.6ab
 *                       lesson), the project stores, and the pinned photo mount
 *                       carrying the ONE vault-link avatar (Aurelia's).
 *
 * ── WHAT THE CORPUS STAGES (the getHomeData arms) ────────────────────────────
 *   - 14 salon chats (> the 12 slice cap), one `help` chat and one `autonomous`
 *     chat (both NEWER than every salon chat — the filter arm — and both
 *     carrying a participant, so the character chat counts prove they run over
 *     allChatsRaw, BEFORE the filter).
 *   - Chats with and without participants, an INACTIVE participant, a
 *     participant whose characterId dangles (the `character: null` arm), a
 *     `controlledBy: 'user'` persona participant, a story background that
 *     resolves, one that dangles (miss → null), and a lastMessageAt-null chat
 *     (the updatedAt sort fallback).
 *   - 14 projects (> the 12 cap) whose activity ranking is decided by THREE
 *     different sources: P01 by chat lastMessageAt, P02 by file updatedAt, P03
 *     by its own updatedAt. P01 carries TWO chats (the count increment).
 *   - 28 characters: 26 AI-controlled (> the 24 cap; the base-name sort drops
 *     the last two), one npc + one user-controlled (both filtered from the
 *     grid, both participants — their counts exist invisibly). Favorites,
 *     case-tie ("Aria"/"aria") and accent-tie ("Ötz"/"otz") name pairs (BASE
 *     sensitivity ties → creation order; tertiary would reorder them — the
 *     collator arm), an empty-string title (`|| null`), a vault-link avatar
 *     (resolves on BOTH the grid and the chat-list path) vs a legacy files-row
 *     avatar (resolves on the grid, null on the vault-only LIST path), and one
 *     `defaultConnectionProfileId` holder.
 *   - A second user owning NOTHING (the empty arms + the displayName
 *     fallbacks) — projects/files are findAll (unscoped), chats/characters are
 *     user-scoped, and the payload proves the split.
 *
 * `addMessages` bumps chat `lastMessageAt`/`updatedAt` with live timestamps
 * (the enclave-step-fixture lesson), so BOTH are re-pinned per chat via a raw
 * UPDATE after all staging.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HOME_MAIN=$W/crates/quilltap-web/tests/fixtures/home-main.db \
 *   QT_FIXTURE_HOME_MOUNT=$W/crates/quilltap-web/tests/fixtures/home-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-home-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

interface ParticipantSpec {
  id: string;
  characterId: string;
  displayOrder: number;
  isActive: boolean;
  controlledBy: string;
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
}
interface ChatSpec {
  id: string;
  title: string;
  chatType?: string;
  projectId?: string;
  storyBackgroundImageId?: string;
  isDangerousChat?: boolean;
  /** P4.D143 (v4 `c43d3b1b4`): the operator's Concierge override. Without it
   * the home differential could only ever see `'monitored'` and `'flagged'` —
   * two of the four states the `RecentChat` payload now carries. */
  conciergeOverride?: 'OFF' | 'UNCENSORED';
  /** The classifier's categories, shown on the mark's tooltip when Flagged.
   * Seeded non-empty on exactly one chat so a dropped `?? []` is visible. */
  dangerCategories?: string[];
  participants: ParticipantSpec[];
  messages: MessageSpec[];
  lastMessageAt: string | null;
  updatedAt: string;
}
interface CharacterSpec {
  id: string;
  name: string;
  title?: string;
  tags?: string[];
  controlledBy?: string;
  npc?: boolean;
  isFavorite?: boolean;
  defaultConnectionProfileId?: string;
  defaultImageId?: string;
  vaultAvatar?: boolean;
}
interface ProjectSpec {
  id: string;
  name: string;
  description?: string;
  color?: string;
  icon?: string;
  updatedAt: string;
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  emptyUserId: string;
  connectionProfileId: string;
  photoMountId: string;
  vaultAvatarPath: string;
  tags: Array<{ id: string; name: string }>;
  projects: ProjectSpec[];
  files: Array<Record<string, unknown>>;
  characters: CharacterSpec[];
  chats: ChatSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'home-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_HOME_MAIN;
  const mountOut = process.env.QT_FIXTURE_HOME_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_HOME_{MAIN,MOUNT} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-home-fixture-'));
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
  const { CharacterSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
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

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);

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

  // 1. The two users: the primary (named — the `user?.name` greeting arm) and
  //    the empty one (name null, owns nothing — the fallback + empty-list arms).
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'nameless', email: null, name: null } as never,
    { id: spec.emptyUserId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. The connection profile the defaultConnectionProfileId holder names.
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Fixture Cheap',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-cheap-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: null,
      isCheap: true,
    } as never,
    { id: spec.connectionProfileId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2b. The two tag rows Aurelia's `tags` reference (character tags are tag
  //     IDS — the home DTOs pass the raw id strings through).
  for (const t of spec.tags) {
    await repos.tags.create(
      {
        userId: spec.userId,
        name: t.name,
        nameLower: t.name.toLowerCase(),
        quickHide: false,
      } as never,
      { id: t.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 3. The legacy files rows: the story background, the legacy avatar, and the
  //    three project-activity files (each with a PINNED updatedAt — the
  //    project-activity max reads it).
  for (const f of spec.files) {
    const { id, updatedAt, ...rest } = f as Record<string, unknown>;
    await repos.files.create({ userId: spec.userId, linkedTo: [], tags: [], ...rest } as never, {
      id,
      createdAt: TS,
      updatedAt: updatedAt ?? TS,
    } as never);
  }

  // 4. Projects (store-backed — each provisions a store in the mount-index DB).
  //    updatedAt is PINNED per project: the three-source activity sort reads it.
  for (const p of spec.projects) {
    await repos.projects.create(
      {
        userId: spec.userId,
        name: p.name,
        ...(p.description !== undefined ? { description: p.description } : {}),
        ...(p.color !== undefined ? { color: p.color } : {}),
        ...(p.icon !== undefined ? { icon: p.icon } : {}),
      } as never,
      { id: p.id, createdAt: TS, updatedAt: p.updatedAt } as never,
    );
  }

  // 5. The pinned photo mount + the ONE vault-link avatar (Aurelia's). The link
  //    id is MINTED (linkDocumentContent owns it) and baked into the committed
  //    fixture — both differential sides read the identical row.
  await repos.docMountPoints.create(
    {
      name: 'Fixture Photos',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: spec.photoMountId, createdAt: TS, updatedAt: TS } as never,
  );
  const portrait = 'a small portrait of an aeronaut, rendered in fixture bytes';
  const { link } = await repos.docMountFileLinks.linkDocumentContent({
    mountPointId: spec.photoMountId,
    relativePath: spec.vaultAvatarPath,
    fileName: spec.vaultAvatarPath.split('/').pop(),
    folderId: null,
    fileType: 'image',
    content: portrait,
    contentSha256: createHash('sha256').update(portrait, 'utf-8').digest('hex'),
    plainTextLength: portrait.length,
    fileSizeBytes: Buffer.byteLength(portrait, 'utf-8'),
  } as never);

  // 6. Characters, in SPEC ORDER — creation order is rowid order is the
  //    findByUserId order, which is what the sort's stable ties preserve (the
  //    base-sensitivity tie pairs depend on it). Each create mints its vault.
  for (const c of spec.characters) {
    const defaultImageId = c.vaultAvatar ? (link as { id: string }).id : c.defaultImageId;
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        ...(c.title !== undefined ? { title: c.title } : {}),
        description: null,
        aliases: [],
        controlledBy: c.controlledBy ?? 'llm',
        talkativeness: 0.5,
        npc: c.npc ?? false,
        isFavorite: c.isFavorite ?? false,
        tags: c.tags ?? [],
        defaultConnectionProfileId: c.defaultConnectionProfileId ?? null,
        ...(defaultImageId !== undefined ? { defaultImageId } : {}),
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 7. Chats + messages. `addMessages` bumps lastMessageAt/updatedAt with LIVE
  //    timestamps, so both are re-pinned afterwards via a raw UPDATE (step 8).
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      displayOrder: p.displayOrder,
      isActive: p.isActive,
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: chat.title,
        participants,
        contextSummary: null,
        ...(chat.chatType !== undefined ? { chatType: chat.chatType } : {}),
        ...(chat.projectId !== undefined ? { projectId: chat.projectId } : {}),
        ...(chat.isDangerousChat !== undefined ? { isDangerousChat: chat.isDangerousChat } : {}),
        ...(chat.conciergeOverride !== undefined
          ? { conciergeOverride: chat.conciergeOverride }
          : {}),
        ...(chat.dangerCategories !== undefined
          ? { dangerCategories: chat.dangerCategories }
          : {}),
        ...(chat.storyBackgroundImageId !== undefined
          ? { storyBackgroundImageId: chat.storyBackgroundImageId }
          : {}),
      } as never,
      { id: chat.id, createdAt: TS, updatedAt: chat.updatedAt } as never,
    );
    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m) => ({
          id: m.id,
          type: 'message',
          role: m.role,
          content: m.content,
          createdAt: TS,
          attachments: [],
        })) as never,
      );
    }
  }

  // 8. Re-pin every chat's updatedAt + lastMessageAt (the addMessages bump),
  //    and every project's updatedAt (the store-backed create mints LIVE
  //    timestamps on the slim row regardless of the create opts — and the
  //    three-source activity sort reads that value, so it must be pinned).
  for (const chat of spec.chats) {
    await rawQuery('UPDATE "chats" SET "updatedAt" = ?, "lastMessageAt" = ? WHERE "id" = ?', [
      chat.updatedAt,
      chat.lastMessageAt,
      chat.id,
    ]);
  }
  for (const p of spec.projects) {
    await rawQuery('UPDATE "projects" SET "createdAt" = ?, "updatedAt" = ? WHERE "id" = ?', [
      TS,
      p.updatedAt,
      p.id,
    ]);
  }

  // 9. Force the empty tables the read path touches into existence (the salon
  //    fixture's lesson): the chat enrichment counts memories per chat, and the
  //    Rust side has no lazy CREATE TABLE.
  await ensureCollection('memories', (await import('@/lib/schemas/memory.types')).MemorySchema);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // The raw UPDATEs leave zero-byte TRUNCATE-journal files next to the
  // outputs; only the .db files are the committed fixture.
  for (const out of [mainOut, mountOut]) {
    const j = out + '-journal';
    if (existsSync(j)) rmSync(j);
  }

  process.stderr.write(
    `built home fixture: ${mainOut} (+${mountOut}) — 2 users, ` +
      `${spec.characters.length} characters, ${spec.chats.length} chats, ` +
      `${spec.projects.length} projects, ${spec.files.length} files, ` +
      `vault avatar link ${(link as { id: string }).id}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`home fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
