/**
 * Tier-3 fixture builder for the memory gate (v4 createMemoryWithGate).
 *
 * Bakes the SEED state both differential sides start from: for every scenario
 * with pre-existing memories it seeds one `memories` row per seed (pinned id +
 * `seedTimestamp`), its vector on the row's own `embedding` (the seed's unit
 * vector as a Float32Array), through v4's REAL `MemoriesRepository`. Both the jest oracle and
 * the Rust test then copy THIS fixture and run only the gate — which mints new
 * ids/timestamps — so the seed rows are byte-identical on both sides and only the
 * gate's own effect differs.
 *
 * Built FROM THE PIN (P4.D266, `01a83539d`): generateDDL carries the four
 * `memories` tier columns, so a seed may be cold / superseded / a CONSOLIDATED
 * digest; `idx_memories_character_tier` is migration-only and absent here
 * (harmless — no read depends on it).
 *
 * P4.D288 (v4 `af48f6cc2`, the `83334b9a5` pin): the `vector_entries` mirror is
 * gone — the index loads `memories.embedding`, so a seed's vector is written on
 * its row (a seed without one has no vector). Characters with no seed (`insert_empty`,
 * `skip_embedding_failed`) get nothing — their index is created (or not) by the
 * gate.
 *
 * Run from the v4 server checkout under Node (matches v4's real native binding):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-memory-gate-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-gate-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedMemory {
  id: string;
  content: string;
  summary: string;
  /** Absent → no embedding on the row (a digest the index never held). */
  vector?: number[];
  // P4.D266 (the `d58548051` cold tier + the `3f7320138` cap): optional row
  // overrides. Absent → the literals every pre-round scenario was seeded with,
  // so their seed bytes do not move.
  tier?: 'hot' | 'cold';
  supersededById?: string | null;
  consolidatedFrom?: string[];
  consolidatedAt?: string | null;
  source?: 'AUTO' | 'MANUAL' | 'CONSOLIDATED';
  importance?: number;
  reinforcementCount?: number;
  reinforcedImportance?: number;
  relatedMemoryIds?: string[];
  /** Episodic spine — seeded so the gate's date guard has an existing occasion. */
  occurredAt?: string;
  narrativeTime?: string;
  entities?: string[];
  kind?: 'semantic' | 'episodic';
}
interface Scenario {
  name: string;
  characterId: string;
  seedMemories: SeedMemory[];
}
interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  scenarios: Scenario[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'memory-gate-tier3.json'), 'utf8')
  ) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-gate-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');

  await initializeDatabase();
  const memories = new MemoriesRepository();

  let seededMemories = 0;
  let seededVectors = 0;
  for (const scenario of spec.scenarios) {
    for (const seed of scenario.seedMemories) {
      await memories.create(
        {
          characterId: scenario.characterId,
          aboutCharacterId: null,
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
          consolidatedAt: seed.consolidatedAt ?? null,
        } as never,
        {
          id: seed.id,
          createdAt: spec.seedTimestamp,
          updatedAt: spec.seedTimestamp,
        }
      );
      seededMemories++;

      if (seed.vector) seededVectors++;
    }
  }

  await closeDatabase();
  process.stderr.write(
    `built memory-gate fixture: ${out} (${seededMemories} seed memories, ${seededVectors} seed vectors)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-gate fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
