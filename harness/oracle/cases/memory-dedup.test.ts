/**
 * @jest-environment node
 *
 * P4.43 MEMORY-DEDUP ORACLE: drives v4's REAL `deduplicateAllMemories`
 * (`lib/tools/memory-dedup.ts`) over a FRESH copy of the committed
 * `embedding-profiles-{main,mount}.db` fixture, in BOTH modes:
 *   - `preview` (dryRun=true)  — result JSON only; the memories table is unchanged;
 *   - `apply`   (dryRun=false) — result JSON + the mutated memories table (survivor
 *     content rewritten with `[+]` footnotes, discards deleted, neighbours'
 *     relatedMemoryIds scrubbed through `deleteMemoriesWithUnlinkBatch`).
 *
 * Deterministic — NO LLM, NO jobs. The seam mocks are `requireActual`
 * pass-throughs so the DB manager, repositories, embedding-service
 * (`cosineSimilarity`), memory-gate (`extractNovelDetails` /
 * `deleteMemoriesWithUnlinkBatch`), memory-merge, vector-store, and the dedup
 * tool itself run v4's REAL code against the REAL fixture DBs — EXCEPT
 * `generateEmbeddingForUser`, which answers ONE text-independent canned vector
 * and RECORDS every text it is asked for (`embedCalls`; P4.D266 R-D — the fold
 * re-embeds a changed survivor). `processedAt` is normalized; per-row
 * `createdAt` / `updatedAt` are omitted from the memories dump (a survivor's
 * `updatedAt` is minted on apply).
 *
 * P4.D266 (pin `01a83539d`, `3f7320138` F9): the fold. On each COPY, v4's REAL
 * `add-memory-tiers-v1` runs first (`applyMemoryTiersMigration` — idempotent;
 * the committed pair already carries the columns since P4.D265's in-place
 * widen), then `memory-dedup-plants.json` is created through v4's REAL
 * `MemoriesRepository.create` (six clusters on Aria: the footnote cap, summed
 * counts + an earlier occurredAt, a cross-cluster link, DEDUP_CROSSES_TIERS,
 * DEDUP_DELETES_DIGEST, and a failed fold via a planted BEFORE UPDATE trigger).
 *
 * Run from the v4 checkout under Node 24 (cp to a /tmp mirror; jest ignores
 * .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-dedup-oracle ; rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   mkdir -p "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/memory-dedup.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts "$TMPO/fixtures/"
 *   cp $V5W/harness/oracle/fixtures/memory-dedup-plants.json "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_EP_MGMT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-main.db \
 *   QT_EP_MGMT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-memory-dedup.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- memory-dedup
 */

import * as fs from 'fs';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyMemoryTiersMigration } from '../fixtures/apply-memory-tiers-migration';

interface Plant {
  id: string;
  characterId: string;
  content: string;
  summary: string;
  importance: number;
  embedding: number[] | null;
  source: 'AUTO' | 'MANUAL' | 'CONSOLIDATED';
  tier: 'hot' | 'cold';
  supersededById: string | null;
  consolidatedFrom: string[];
  reinforcementCount: number;
  relatedMemoryIds: string[];
  occurredAt: string | null;
  lastReinforcedAt: string | null;
}
const PLANTS = JSON.parse(
  readFileSync(join(__dirname, '..', 'fixtures', 'memory-dedup-plants.json'), 'utf8'),
) as {
  createdAt: string;
  failTriggerSurvivorId: string;
  cannedEmbedding: number[];
  plants: Plant[];
};

/** Every text v4's fold asked to embed, in call order (reset per mode). */
let embedCalls: string[] = [];

const PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';
const USER_ID = 'aa000000-0000-4000-8000-0000000000aa';
const THRESHOLD = 0.8;

function applyMocks(): void {
  const cipherDriverPath = join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  for (const m of [
    '@/lib/database/manager',
    '@/lib/database/repositories',
    '@/lib/repositories/factory',
    '@/lib/database/backends/sqlite/client',
    '@/lib/database/backends/sqlite/mount-index-client',
    '@/lib/database/backends/sqlite/introspection',
    '@/lib/paths',
    '@/lib/tools/memory-dedup',
    '@/lib/memory/memory-gate',
    '@/lib/memory/memory-merge',
    '@/lib/embedding/vector-store',
  ]) {
    jest.doMock(m, () => jest.requireActual(m));
  }
  // The jest-incomplete `embedding-service` trap: `deduplicateCharacterMemories`
  // calls `cosineSimilarity` from it, which is `undefined` under the default
  // jest mock — so the REAL module, with only `generateEmbeddingForUser`
  // canned + recorded (P4.D266 R-D).
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        embedCalls.push(text);
        return {
          embedding: new Float32Array(PLANTS.cannedEmbedding),
          model: 'canned',
          dimensions: PLANTS.cannedEmbedding.length,
          provider: 'canned',
        };
      },
    };
  });
  jest.doMock('@/lib/startup/startup-state', () => {
    const a = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...a,
      startupState: {
        ...a.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

/** The memories columns the differential compares (dedup-relevant; the minted
 * `updatedAt` and unchanged `createdAt` are omitted). */
const MEM_COLS = [
  'id',
  'characterId',
  'content',
  'summary',
  'importance',
  'embedding',
  'occurredAt',
  'narrativeTime',
  'entities',
  'kind',
  'source',
  'keywords',
  'tags',
  'relatedMemoryIds',
  // P4.D266: the fold's count fields and the tier columns.
  'reinforcementCount',
  'reinforcedImportance',
  'lastReinforcedAt',
  'tier',
  'supersededById',
  'consolidatedFrom',
  'consolidatedAt',
];

async function dumpMemories(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: () => Array<Record<string, unknown>> };
  };
  const rows = main.prepare(`SELECT * FROM memories ORDER BY id`).all();
  return rows.map((r) => {
    const out: Record<string, unknown> = {};
    for (const col of MEM_COLS) {
      const v = r[col];
      out[col] = Buffer.isBuffer(v) ? Buffer.from(v).toString('hex') : v ?? null;
    }
    return out;
  });
}

async function runOne(
  mode: 'preview' | 'apply',
  fixtures: { main: string; mount: string },
): Promise<{
  mode: string;
  threshold: number;
  result: unknown;
  memories: unknown;
  embedCalls: string[];
}> {
  jest.resetModules();
  applyMocks();
  embedCalls = [];

  const scratch = mkdtempSync(join(tmpdir(), `qt-dedup-${mode}-`));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  // Place the mount at the derived path AND point SQLITE_MOUNT_INDEX_PATH at it
  // so both the standard client (characters.findAll overlay) and any
  // getMountIndexDatabasePath consumer see it.
  const mountWork = join(scratch, 'data', 'quilltap-mount-index.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  // v4's REAL `add-memory-tiers-v1` on the COPY (idempotent on the widened pair).
  await applyMemoryTiersMigration(mainWork, { pepperBase64: PEPPER });

  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { deduplicateAllMemories } = await import('@/lib/tools/memory-dedup');

  await initializeDatabase();

  // The plants, through v4's REAL repository (pinned ids + timestamps).
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const repo = new MemoriesRepository();
  for (const p of PLANTS.plants) {
    await repo.create(
      {
        characterId: p.characterId,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: p.content,
        summary: p.summary,
        keywords: [],
        tags: [],
        importance: p.importance,
        embedding: p.embedding ? new Float32Array(p.embedding) : null,
        source: p.source,
        witnessedContext: null,
        occurredAt: p.occurredAt,
        narrativeTime: null,
        entities: [],
        kind: 'semantic',
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: p.reinforcementCount,
        lastReinforcedAt: p.lastReinforcedAt,
        relatedMemoryIds: p.relatedMemoryIds,
        reinforcedImportance: p.importance,
        tier: p.tier,
        supersededById: p.supersededById,
        consolidatedFrom: p.consolidatedFrom,
        consolidatedAt: null,
      } as never,
      { id: p.id, createdAt: PLANTS.createdAt, updatedAt: PLANTS.createdAt },
    );
  }
  // The failed fold: any UPDATE of that survivor aborts.
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  (getRawDatabase() as unknown as { exec: (s: string) => void }).exec(
    `CREATE TRIGGER p4d266_planted_fold_failure BEFORE UPDATE ON memories ` +
      `WHEN OLD.id = '${PLANTS.failTriggerSurvivorId}' ` +
      `BEGIN SELECT RAISE(ABORT, 'planted fold failure'); END`,
  );

  const result = (await deduplicateAllMemories(USER_ID, THRESHOLD, mode === 'preview')) as Record<
    string,
    unknown
  >;
  const memories = await dumpMemories();
  await closeDatabase();

  // Normalize the one non-deterministic result field.
  result.processedAt = '<processedAt>';

  rmSync(scratch, { recursive: true, force: true });
  return { mode, threshold: THRESHOLD, result, memories, embedCalls: [...embedCalls] };
}

async function main(): Promise<void> {
  const fixtures = {
    main: process.env.QT_EP_MGMT_MAIN ?? '',
    mount: process.env.QT_EP_MGMT_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const preview = await runOne('preview', fixtures);
  const apply = await runOne('apply', fixtures);

  fs.writeFileSync(outPath, JSON.stringify(preview) + '\n' + JSON.stringify(apply) + '\n');
}

it('memory-dedup oracle', async () => {
  await main();
}, 120000);
