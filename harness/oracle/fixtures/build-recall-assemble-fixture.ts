/**
 * The committed recall-assemble fixture builder (P4.D291) — the test-pepper
 * substrate of `recall_assemble_tier3_equivalence`, the tier-3 differential of
 * v4's REAL `assembleRecall` / `deliverRecall` (`lib/memory/recall/`,
 * `584d28164`, pinned at `83334b9a5`) over a REAL frozen archive.
 *
 * The corpus is v4's own `assemble-deliver.test.ts` one, re-planted so the
 * archive is computed rather than mocked (`recall-assemble.json`'s
 * `$comment`): the digest about Iris and the 27 newest importance-0.9 rows
 * fill the 28-row archive a 2000-token budget sizes; the head corpus — eight
 * fillers, the two rows about Iris, the recap-only row — sits below it in
 * salience. Built AT THE PIN through v4's REAL repositories (every id and
 * timestamp pinned in the spec); the `memories.embedding` column IS the index
 * (P4.D288, v4 `af48f6cc2`). No character rows: the plans hand the names in.
 * ONE chat row (no participants) — delivery writes its anti-repetition ring.
 *
 * Regenerate (Node 24, from the v4 pin) and commit the main .db (the mount-index file stays empty and is not committed) —
 * a deliberate fixture REBUILD, never part of an oracle regen (the P4.D32
 * rule); its only reader is `recall_assemble_tier3_equivalence`:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd <the v4 pin>
 *   QT_FIXTURE_RA_MAIN=$V5W/crates/quilltap-web/tests/fixtures/recall-assemble-main.db \
 *   QT_FIXTURE_RA_MOUNT=/tmp/qt-ra-mount-unused.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-recall-assemble-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedMemory {
  id: string;
  characterId: string;
  cosine: number;
  content: string;
  summary: string;
  importance: number;
  createdAt: string;
  embedding: number[];
  keywords?: string[];
  projectId?: string | null;
  aboutCharacterId?: string | null;
  chatId?: string | null;
  relatedMemoryIds?: string[];
  tier?: 'hot' | 'cold';
  source?: 'AUTO' | 'CONSOLIDATED';
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: Array<{ id: string; name: string }>;
  memories: SeedMemory[];
  chat: { id: string; title: string; seedTimestamp: string };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'recall-assemble.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_RA_MAIN;
  const mountOut = process.env.QT_FIXTURE_RA_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_RA_MAIN and QT_FIXTURE_RA_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ra-fixture-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');

  await initializeDatabase();

  const repos = getRepositories();

  // No user / character rows: the search reads only `memories` (and a
  // character create would scaffold a vault). Memories, each with its embedding
  // COLUMN — since P4.D288 (v4 `af48f6cc2`, the `83334b9a5` pin) that column IS
  // the index: the `vector_entries` mirror is gone and the store loads its
  // entries from the rows, so there is no separate store write.
  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: m.characterId,
        aboutCharacterId: m.aboutCharacterId ?? null,
        chatId: m.chatId ?? null,
        projectId: m.projectId ?? null,
        content: m.content,
        summary: m.summary,
        keywords: m.keywords ?? [],
        tags: [],
        importance: m.importance,
        embedding: new Float32Array(m.embedding),
        source: m.source ?? 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: m.relatedMemoryIds ?? [],
        reinforcedImportance: m.importance,
        occurredAt: null,
        narrativeTime: null,
        kind: 'semantic',
        entities: [],
      } as never,
      { id: m.id, createdAt: m.createdAt, updatedAt: m.createdAt },
    );
  }

  // The chat delivery writes its ring to (no participants, no messages).
  await repos.chats.create(
    { userId: spec.userId, title: spec.chat.title, participants: [] } as never,
    { id: spec.chat.id, createdAt: spec.chat.seedTimestamp, updatedAt: spec.chat.seedTimestamp },
  );

  // Cold rows through v4's REAL tier writer (the store stamps tiers at load).
  let cold = 0;
  for (const c of spec.characters) {
    const ids = spec.memories.filter((m) => m.characterId === c.id && m.tier === 'cold').map((m) => m.id);
    if (ids.length === 0) continue;
    cold += await repos.memories.updateTierBulk(c.id, ids, 'cold');
  }

  // Every seed carries an embedding, so every row is indexed.
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  const indexed = (
    mainDb.prepare('SELECT COUNT(*) AS n FROM "memories" WHERE "embedding" IS NOT NULL').get() as {
      n: number;
    }
  ).n;
  if (indexed !== spec.memories.length) {
    throw new Error(`memories with an embedding ${indexed} != seeded ${spec.memories.length}`);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built recall-assemble fixture: main=${mainOut} mount=${mountOut} ` +
      `(${spec.memories.length} memories, ${indexed} indexed, ${cold} cold)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`recall-assemble fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
