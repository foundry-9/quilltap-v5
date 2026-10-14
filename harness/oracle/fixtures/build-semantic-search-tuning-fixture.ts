/**
 * The committed semantic-search-tuning fixture builder (P4.D267) — the
 * test-pepper substrate of `semantic_search_tuning_tier3_equivalence`, the
 * tier-3 differential of v4's REAL `searchMemoriesSemantic` at the
 * `01a83539d` pin (the recall retuning's R1 gate / R4 anchors / R6 reserve,
 * hot-only search, the harness knobs).
 *
 * Built AT THE PIN through v4's REAL repositories (every id and timestamp
 * pinned in the spec, `semantic-search-tuning.json`), so the pair carries the
 * memory-tier columns natively. P4.D288 (v4 `af48f6cc2`, the `83334b9a5` pin):
 * the `vector_entries` mirror the REAL `CharacterVectorStore` once wrote is
 * gone — the store loads `memories.embedding` — so the "DELETED vector entry"
 * / "un-indexed" rows below are now ordinary indexed rows (their spec
 * `unindexed` flag is read by nothing). Three searched characters:
 *   - RANKIN — the ranking / R1 / R4 / R5 corpus: a strong plain row, a
 *     low-cosine row carrying every boost, a related-memory seed and its
 *     sub-floor neighbour, a sub-floor entity hit, entity-only rows whose
 *     vector entries are DELETED (the anchor union is their only way in),
 *     one cold row, one fresh-48h row;
 *   - WINDLE — R6's hard-window corpus (four same-day rows, two older rows
 *     either side of the gate);
 *   - TIERNEY — the hot / cold corpus (cold rows reachable by the vector scan,
 *     by a literal hit, by an un-indexed literal hit and by related expansion).
 *
 * Every embedding is the 2-D unit vector `[c, √(1−c²)]`, so a query `[1, 0]`
 * scores exactly `c` (in f32) under the REAL cosine on both sides. The query
 * embeddings themselves are SCRIPTED per case on both sides (the provider
 * label decides R1's TF-IDF gate-off), so no embedding profile is needed.
 *
 * Regenerate (Node 24, from the v4 checkout pinned at the baseline) and
 * re-copy the committed .db files — this is a deliberate fixture REBUILD,
 * never part of an oracle regen (the P4.D32 rule); its only reader is
 * `semantic_search_tuning_tier3_equivalence`:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SST_MAIN=$V5W/crates/quilltap-web/tests/fixtures/semantic-search-tuning-main.db \
 *   QT_FIXTURE_SST_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/semantic-search-tuning-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-semantic-search-tuning-fixture.ts
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
  unindexed?: boolean;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  characters: Array<{ id: string; name: string }>;
  memories: SeedMemory[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'semantic-search-tuning.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_SST_MAIN;
  const mountOut = process.env.QT_FIXTURE_SST_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_SST_MAIN and QT_FIXTURE_SST_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sst-fixture-'));
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
        source: 'AUTO',
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

  // Cold rows through v4's REAL tier writer (the store stamps tiers at load).
  let cold = 0;
  for (const c of spec.characters) {
    const ids = spec.memories.filter((m) => m.characterId === c.id && m.tier === 'cold').map((m) => m.id);
    if (ids.length === 0) continue;
    cold += await repos.memories.updateTierBulk(c.id, ids, 'cold');
  }

  // Every seed carries an embedding, so every row is indexed (P4.D288: a
  // spec row's `unindexed` flag — the deleted-entry arm — can no longer be
  // expressed and is read by nothing).
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
    `built semantic-search-tuning fixture: main=${mainOut} mount=${mountOut} ` +
      `(${spec.memories.length} memories, ${indexed} indexed, ${cold} cold)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`semantic-search-tuning fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
