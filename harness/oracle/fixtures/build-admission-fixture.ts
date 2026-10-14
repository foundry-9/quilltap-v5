/**
 * Tier-3 SEED fixture builder for ADMISSION (v4 `lib/memory/admission/`, the
 * `83334b9a5` pin) — the main + mount-index pair both differential sides
 * start from (P4.D290; renamed from `build-memory-gate-fixture.ts`, whose
 * subject v4 deleted in `8af764514`).
 *
 * Bakes, through v4's REAL repositories:
 *   - one `chat_settings` row for the user with NO timezone (the Chicago leg
 *     of the oracle sets it with a raw UPDATE on both sides — a valid row, so
 *     v4's whole-row `ChatSettingsSchema` keeps it);
 *   - the name-presence characters (full vault provisioning in the
 *     mount-index DB);
 *   - every seed memory (pinned id + `seedTimestamp`), its vector on the
 *     row's own `embedding` (v4 `af48f6cc2`: the index loads `memories.
 *     embedding`), cold rows / digests / chains / mixed widths as the spec
 *     says.
 *
 * Then two RAW plants the repository cannot make (identical on both sides):
 *   - `raw` seeds — a schema-invalid row (a non-UUID id) carrying a vector,
 *     for the `FIND_BY_IDS_UNVALIDATED` measurement (v4's `findByFilter`
 *     drops it; v5's `find_by_ids` does not);
 *   - `vanishOnPromote` seeds — an `AFTER UPDATE OF tier` trigger that
 *     deletes the row once a promotion has moved it hot, so admission's next
 *     write finds it gone: the "target retired between decide and write"
 *     arm, driven through both sides' REAL code.
 *
 * Run from the v4 pin under Node 24 (see the oracle case's header):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   QT_FIXTURE_OUT=/tmp/p4d290/admission-main.db QT_FIXTURE_MOUNT_OUT=/tmp/p4d290/admission-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-admission-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedMemory {
  id: string;
  characterId: string;
  content: string;
  summary: string;
  vector?: number[];
  tier?: 'hot' | 'cold';
  supersededById?: string | null;
  consolidatedFrom?: string[];
  source?: 'AUTO' | 'MANUAL' | 'CONSOLIDATED';
  importance?: number;
  reinforcementCount?: number;
  reinforcedImportance?: number;
  relatedMemoryIds?: string[];
  occurredAt?: string;
  narrativeTime?: string;
  entities?: string[];
  kind?: 'semantic' | 'episodic';
  aboutCharacterId?: string;
  raw?: boolean;
  vanishOnPromote?: boolean;
}
interface CharSpec {
  id: string;
  name: string;
  aliases: string[];
  controlledBy: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  chatSettingsId: string;
  characters: CharSpec[];
  seeds: SeedMemory[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'admission-tier3.json'), 'utf8')) as Spec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-admission-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
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
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  // The mount-index content tables the vault scaffold needs (hand-written
  // repos whose DDL v4 emits elsewhere at startup) + `doc_mount_blobs`.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
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
  midb.exec('CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")');

  await repos.chatSettings.create({ userId: spec.userId } as never, {
    id: spec.chatSettingsId,
    createdAt: spec.seedTimestamp,
    updatedAt: spec.seedTimestamp,
  });

  for (const c of spec.characters) {
    await repos.characters.create(
      { name: c.name, aliases: c.aliases, userId: spec.userId, controlledBy: c.controlledBy } as never,
      { id: c.id }
    );
  }

  const raw = getRawDatabase();
  if (!raw) throw new Error('main DB handle unavailable');
  let seeded = 0;
  let vectors = 0;
  for (const seed of spec.seeds) {
    const data = {
      characterId: seed.characterId,
      aboutCharacterId: seed.aboutCharacterId ?? null,
      chatId: null,
      projectId: null,
      content: seed.content,
      summary: seed.summary,
      keywords: [],
      tags: [],
      importance: seed.importance ?? 0.5,
      embedding: seed.vector ? new Float32Array(seed.vector) : null,
      source: seed.source ?? 'AUTO',
      witnessedContext: null,
      occurredAt: seed.occurredAt ?? null,
      narrativeTime: seed.narrativeTime ?? null,
      entities: seed.entities ?? [],
      kind: seed.kind ?? 'semantic',
      sourceMessageId: null,
      lastAccessedAt: null,
      reinforcementCount: seed.reinforcementCount ?? 1,
      lastReinforcedAt: null,
      relatedMemoryIds: seed.relatedMemoryIds ?? [],
      reinforcedImportance: seed.reinforcedImportance ?? 0.5,
      tier: seed.tier ?? 'hot',
      supersededById: seed.supersededById ?? null,
      consolidatedFrom: seed.consolidatedFrom ?? [],
      consolidatedAt: null,
    };
    if (seed.raw) {
      // A row v4's MemorySchema refuses (a non-UUID id): only a raw INSERT
      // can make it.
      raw
        .prepare(
          `INSERT INTO memories (id, characterId, content, summary, keywords, tags, importance, embedding, source, entities, kind, reinforcementCount, relatedMemoryIds, reinforcedImportance, tier, consolidatedFrom, createdAt, updatedAt)
           VALUES (?, ?, ?, ?, '[]', '[]', ?, ?, 'AUTO', '[]', 'semantic', 1, '[]', ?, 'hot', '[]', ?, ?)`
        )
        .run(
          seed.id,
          seed.characterId,
          seed.content,
          seed.summary,
          data.importance,
          Buffer.from(new Float32Array(seed.vector ?? []).buffer),
          data.reinforcedImportance,
          spec.seedTimestamp,
          spec.seedTimestamp
        );
    } else {
      await repos.memories.create(data as never, {
        id: seed.id,
        createdAt: spec.seedTimestamp,
        updatedAt: spec.seedTimestamp,
      });
    }
    if (seed.vanishOnPromote) {
      raw.exec(
        `CREATE TRIGGER "admission_vanish_${seed.id.replace(/-/g, '_')}" AFTER UPDATE OF "tier" ON "memories"
         WHEN NEW."id" = '${seed.id}' AND NEW."tier" = 'hot'
         BEGIN DELETE FROM "memories" WHERE "id" = NEW."id"; END`
      );
    }
    seeded++;
    if (seed.vector) vectors++;
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built admission fixture: ${outMain} + ${outMount} (${spec.characters.length} characters, ${seeded} seed memories, ${vectors} seed vectors)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`admission fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
