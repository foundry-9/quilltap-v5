/**
 * P4.9P fixture builder — the /tmp-built test-pepper substrate for the
 * ui-search differential (`ui_search_equivalence`). Everything is baked via
 * v4's REAL repos so the DDL + row shapes are production-identical.
 *
 * NOT COMMITTED as .db files (the order's choice: no existing committed family
 * carries all five search types richly enough, so the fixture builds into /tmp
 * and both differential sides read the same files). Deterministic: every id and
 * timestamp is pinned by `ui-search.json`, and the only generated rows (the 60
 * "brass" messages) derive their ids/timestamps arithmetically from the spec.
 *
 * ── WHAT THE CORPUS STAGES (the ui/search route arms) ────────────────────────
 *   - Characters: a name match (C1), a DESCRIPTION-only match with an
 *     empty-string title (C2 — the `title || null` falsy arm), an exact-name
 *     match for the priority-0 arm (C3 "Ottoline"), a memories-only owner (C4),
 *     the participant-id-trick target (C5), and an other-user decoy (C6).
 *   - Chats: a title match whose participant has a NORMAL minted id (CH1 —
 *     `characterNames` comes back EMPTY, v4's broken `c.id === participant.id`
 *     compare), an exact-title chat whose participant id EQUALS its characterId
 *     (CH2 — the one shape that RESOLVES under the broken compare), the brass
 *     pagination chat (CH3, 60 messages), an EMPTY-title chat with a matching
 *     message (CH4 — `chatTitle: 'Untitled Chat'` on the message row via the
 *     `|| 'Untitled Chat'` falsy arm; the title search skips it because ''
 *     contains nothing; `chats.title` is NOT NULL so v4's `c.title?.` guard is
 *     unreachable under the current DDL), and an other-user decoy (CH5).
 *   - Messages: USER + ASSISTANT matches, a SYSTEM-role match (excluded by the
 *     role filter), a >200-char content (matchedValue truncation + centered
 *     snippet with both ellipses), one createdAt EXACTLY EQUAL to C1.updatedAt
 *     (the stable-sort insertion-order tie), and the 60 brass rows that push
 *     the "brass" query past the 50 cap.
 *   - Memories: importance 0 (the `|| 5` falsy arm), a >50-char summary (the
 *     name truncation + '...'), and a CONTENT-only match (M3 — matchedField
 *     stays 'summary' and the snippet takes createSnippet's no-match branch).
 *   - Tags: an exact match ("brass", priority 0), a name needing
 *     encodeURIComponent ("Brass & Goggles", quickHide true), a fresh
 *     priority-1 row ("airship-fittings", the newest updatedAt in the airship
 *     result set), a non-match, and an other-user decoy.
 *
 * Build (Node 24, from the v4 checkout — or the pinned worktree on drift):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this v5 worktree>
 *   cd ~/source/quilltap-server   # or the pinned detached worktree
 *   QT_FIXTURE_UI_SEARCH_MAIN=/tmp/qt-ui-search-fixture/main.db \
 *   QT_FIXTURE_UI_SEARCH_MOUNT=/tmp/qt-ui-search-fixture/mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-ui-search-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  title: string | null;
  description: string | null;
  isFavorite: boolean;
  updatedAt: string;
  user: 'primary' | 'other';
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
  createdAt: string;
}
interface ChatSpec {
  id: string;
  title: string;
  updatedAt: string;
  lastMessageAt: string | null;
  user: 'primary' | 'other';
  participants: Array<{ id: string; characterId: string }>;
  messages: MessageSpec[];
}
interface MemorySpec {
  id: string;
  characterId: string;
  content: string;
  summary: string;
  importance: number;
  source: string;
  createdAt: string;
  updatedAt: string;
}
interface TagSpec {
  id: string;
  name: string;
  quickHide: boolean;
  updatedAt: string;
  user: 'primary' | 'other';
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  otherUserId: string;
  brassMessageCount: number;
  brassChatId: string;
  brassBaseTimestamp: string;
  characters: CharacterSpec[];
  chats: ChatSpec[];
  memories: MemorySpec[];
  tags: TagSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'ui-search.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_UI_SEARCH_MAIN;
  const mountOut = process.env.QT_FIXTURE_UI_SEARCH_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_UI_SEARCH_{MAIN,MOUNT} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    mkdirSync(dirname(out), { recursive: true });
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ui-search-fixture-'));
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
  await ensureCollection('memories', (await import('@/lib/schemas/memory.types')).MemorySchema);

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

  const uid = (who: 'primary' | 'other') => (who === 'primary' ? spec.userId : spec.otherUserId);

  // 1. The two users.
  await repos.users.create(
    { username: 'searcher', email: null, name: 'Searcher' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'decoy', email: null, name: 'Decoy' } as never,
    { id: spec.otherUserId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Tags (v4's `repos.tags.findByUserId` reads them user-scoped).
  for (const t of spec.tags) {
    await repos.tags.create(
      {
        userId: uid(t.user),
        name: t.name,
        nameLower: t.name.toLowerCase(),
        quickHide: t.quickHide,
      } as never,
      { id: t.id, createdAt: TS, updatedAt: t.updatedAt } as never,
    );
  }

  // 3. Characters, in SPEC ORDER (creation order is rowid order is the
  //    findByUserId order — the memories loop and the stable-sort ties both
  //    ride on it). Each create mints its vault.
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: uid(c.user),
        ...(c.title !== null ? { title: c.title } : {}),
        description: c.description,
        aliases: [],
        controlledBy: 'llm',
        talkativeness: 0.5,
        npc: false,
        isFavorite: c.isFavorite,
        tags: [],
        defaultConnectionProfileId: null,
      } as never,
      { id: c.id, createdAt: TS, updatedAt: c.updatedAt } as never,
    );
  }

  // 4. Chats + messages. Every chat is created with a placeholder title and
  //    re-pinned to the spec title by the step-6 raw UPDATE (`chats.title` is
  //    NOT NULL in the schema and create-time Zod may reject the empty string —
  //    the EMPTY-title chat is the reachable `'Untitled Chat'` falsy arm, since
  //    a null title cannot exist under the current DDL); `addMessages` bumps
  //    lastMessageAt/updatedAt with LIVE timestamps, so both are re-pinned in
  //    the same UPDATE.
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p, i) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.characterId,
      controlledBy: 'llm',
      displayOrder: i,
      isActive: true,
      status: 'active',
      createdAt: TS,
      updatedAt: TS,
    }));
    await repos.chats.create(
      {
        userId: uid(chat.user),
        title: 'placeholder-title-repinned',
        participants,
        contextSummary: null,
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
          createdAt: m.createdAt,
          attachments: [],
        })) as never,
      );
    }
  }

  // 5. The 60 brass messages — ids and timestamps derived arithmetically from
  //    the spec so both differential sides can rely on them without listing
  //    sixty rows in JSON. Message i (1-based): id suffix = i in hex (2 chars),
  //    createdAt = brassBaseTimestamp + i seconds, role alternates USER (odd) /
  //    ASSISTANT (even), content carries "Brass" + the decimal index.
  const brassBase = Date.parse(spec.brassBaseTimestamp);
  const brassMessages = Array.from({ length: spec.brassMessageCount }, (_, k) => {
    const i = k + 1;
    return {
      id: `fb000000-0000-4000-8000-0000000000${i.toString(16).padStart(2, '0')}`,
      type: 'message',
      role: i % 2 === 1 ? 'USER' : 'ASSISTANT',
      content: `Brass gear number ${i} hummed in the engine room.`,
      createdAt: new Date(brassBase + i * 1000).toISOString(),
      attachments: [],
    };
  });
  await repos.chats.addMessages(spec.brassChatId, brassMessages as never);

  // 6. Re-pin every chat's updatedAt + lastMessageAt + title (the addMessages
  //    bump / the placeholder title), and every character's timestamps — the
  //    store-backed character create mints LIVE timestamps on the slim row
  //    regardless of the create opts (the home builder's project lesson; the
  //    first oracle run of this family caught characters carrying build-time
  //    clocks, which would have made the sort non-deterministic across
  //    rebuilds).
  for (const chat of spec.chats) {
    await rawQuery(
      'UPDATE "chats" SET "updatedAt" = ?, "lastMessageAt" = ?, "title" = ? WHERE "id" = ?',
      [chat.updatedAt, chat.lastMessageAt, chat.title, chat.id],
    );
  }
  for (const c of spec.characters) {
    await rawQuery('UPDATE "characters" SET "createdAt" = ?, "updatedAt" = ? WHERE "id" = ?', [
      TS,
      c.updatedAt,
      c.id,
    ]);
  }

  // 7. Memories (main partition; the route searches per-character content OR
  //    summary).
  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: null,
        source: m.source,
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: m.createdAt, updatedAt: m.updatedAt } as never,
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // The raw UPDATEs leave zero-byte TRUNCATE-journal files next to the outputs.
  for (const out of [mainOut, mountOut]) {
    const j = out + '-journal';
    if (existsSync(j)) rmSync(j);
  }

  process.stderr.write(
    `built ui-search fixture: ${mainOut} (+${mountOut}) — 2 users, ` +
      `${spec.characters.length} characters, ${spec.chats.length} chats ` +
      `(+${spec.brassMessageCount} brass messages), ${spec.memories.length} memories, ` +
      `${spec.tags.length} tags\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`ui-search fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
