/**
 * Read-differential oracle — v4's `MemoriesRepository` findBy* / count* queries
 * (Phase-2, the memories repo).
 *
 * Opens a COPY of the pre-baked fixture and drives v4's REAL repository read
 * methods for each spec query, emitting each result verbatim. The Rust port
 * (db::memories_read) reads the same fixture and must produce the same results —
 * exactly (no normalization; nothing is mutated).
 *
 * Result shapes per kind: a Memory or null (findById/findByIdForCharacter), a
 * Memory[] (most), a number (count*), `{ memories, totalCount }` (paginated),
 * `{ high, medium, low }` (tier), a `Map`→object (countByChatIds), an array of
 * batches (findByCharacterIdInBatches), a string[] (findDistinctChatIds), or an
 * array of `{ id, characterId }` (findIdsWithoutEmbedding), a string[]
 * (findDistinctCharacterIds — P4.d27). The `embedding` field
 * of any returned Memory is a `Float32Array` → `JSON.stringify` emits the
 * `{"0":…}` object the Rust port reproduces from the BLOB.
 *
 * P4.D265 (v4 `d58548051` / `3f7320138`): the tier corpus (a third character
 * with cold rows, digests, the F4 tie rows and two planted NULL rows), the five
 * new reads (`findHotDigests`, `countHotByCharacterId`, `countUnconsideredHot`,
 * `findColdIdsByCharacterId`, `findExpiredColdIds`), the paginated `tier`, and
 * — per query — the repository's raw-path lines (`Fetched most important
 * memories` DEBUG, `Memory failed validation in <context>` WARN), captured by
 * splicing a transport into the singleton logger, as `lines: [{level,
 * message, fields}]`.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMREAD=/tmp/qt-memread.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memories-read.ts \
 *     > /tmp/oracle-memread.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Query {
  kind: string;
  id?: string;
  characterId?: string | null;
  memoryId?: string;
  chatId?: string | null;
  aboutCharacterId?: string;
  aboutCharacterIds?: string[];
  sourceMessageId?: string;
  since?: string;
  ids?: string[];
  keywords?: string[];
  query?: string;
  searchText?: string;
  source?: string;
  minImportance?: number;
  limit?: number;
  offset?: number;
  sortBy?: string;
  sortOrder?: 'asc' | 'desc';
  search?: string;
  batchSize?: number;
  limitPerCharacter?: number;
  high?: number;
  medium?: number;
  low?: number;
  chatIds?: string[];
  // P4.D265
  tier?: 'hot' | 'cold';
  subject?: string;
  olderThan?: string;
}
interface Spec {
  testPepperBase64: string;
  queries: Query[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'memories-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_MEMREAD;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_MEMREAD must point at the fixture from build-memories-read-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-memread-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'memread-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED_PREFIXES = ['Fetched most important memories', 'Memory failed validation in '];
  const { logger } = await import('@/lib/logger');
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      if (!CAPTURED_PREFIXES.some((p) => data.message.startsWith(p))) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import(
    '@/lib/database/repositories/memories.repository'
  );

  await initializeDatabase();
  const repo = new MemoriesRepository();

  // v4 `ad1c4c37f` deletes `findByKeywords` + `findByAboutCharacterId` as dead
  // code (v5 never had a production caller either — retired, not ported). The
  // committed fixture keeps their rows (§R.7/no-fixture-edit); skip them here
  // rather than crash on the deleted methods.
  const retiredKinds = new Set(['findByKeywords', 'findByAboutCharacterId']);
  const queries = spec.queries.filter((q) => !retiredKinds.has(q.kind));

  const results: Array<{ kind: string; result: unknown; lines: unknown[] }> = [];
  for (const q of queries) {
    captured = [];
    let result: unknown;
    switch (q.kind) {
      case 'findById':
        result = await repo.findById(q.id as string);
        break;
      case 'findByIdForCharacter':
        result = await repo.findByIdForCharacter(q.characterId as string, q.memoryId as string);
        break;
      case 'findAll':
        result = await repo.findAll();
        break;
      case 'findByCharacterId':
        result = await repo.findByCharacterId(q.characterId as string);
        break;
      case 'findByCharacterIdInBatches': {
        const batches: unknown[] = [];
        for await (const batch of repo.findByCharacterIdInBatches(
          q.characterId as string,
          q.batchSize as number
        )) {
          batches.push(batch);
        }
        result = batches;
        break;
      }
      case 'findByIds':
        result = await repo.findByIds(q.ids as string[]);
        break;
      case 'findByCharacterIdPaginated':
        result = await repo.findByCharacterIdPaginated(q.characterId as string, {
          limit: q.limit as number,
          offset: q.offset as number,
          sortBy: q.sortBy,
          sortOrder: q.sortOrder,
          search: q.search,
          source: q.source as 'AUTO' | 'MANUAL' | 'CONSOLIDATED' | undefined,
          minImportance: q.minImportance,
          tier: q.tier,
        });
        break;
      case 'searchByContent':
        result = await repo.searchByContent(q.characterId as string, q.query as string);
        break;
      case 'findByImportance':
        result = await repo.findByImportance(q.characterId as string, q.minImportance as number);
        break;
      case 'findBySource':
        result = await repo.findBySource(q.characterId as string, q.source as 'AUTO' | 'MANUAL');
        break;
      case 'findRecent':
        result = await repo.findRecent(q.characterId as string, q.limit as number);
        break;
      case 'findMostImportant':
        result = await repo.findMostImportant(q.characterId as string, q.limit as number);
        break;
      case 'findRecentByImportanceTier':
        result = await repo.findRecentByImportanceTier(q.characterId as string, {
          high: q.high,
          medium: q.medium,
          low: q.low,
        });
        break;
      case 'findByCharacterAboutCharacter':
        result = await repo.findByCharacterAboutCharacter(
          q.characterId as string,
          q.aboutCharacterId as string
        );
        break;
      case 'findByCharacterAboutCharacters':
        result = await repo.findByCharacterAboutCharacters(
          q.characterId as string,
          q.aboutCharacterIds as string[],
          q.limitPerCharacter as number
        );
        break;
      case 'findByChatId':
        result = await repo.findByChatId(q.chatId as string);
        break;
      case 'findBySourceMessageId':
        result = await repo.findBySourceMessageId(q.sourceMessageId as string);
        break;
      case 'findMemoriesWithText':
        result = await repo.findMemoriesWithText(
          q.characterId ?? null,
          q.chatId ?? null,
          q.searchText as string
        );
        break;
      case 'countMemoriesWithText':
        result = await repo.countMemoriesWithText(
          q.characterId ?? null,
          q.chatId ?? null,
          q.searchText as string
        );
        break;
      case 'countByCharacterId':
        result = await repo.countByCharacterId(q.characterId as string);
        break;
      case 'countCreatedSince':
        result = await repo.countCreatedSince(q.characterId as string, q.since as string);
        break;
      case 'countWithoutEmbedding':
        result = await repo.countWithoutEmbedding(q.characterId ?? undefined);
        break;
      case 'findIdsWithoutEmbedding':
        result = await repo.findIdsWithoutEmbedding({
          characterId: q.characterId ?? undefined,
          limit: q.limit,
        });
        break;
      case 'countByChatId':
        result = await repo.countByChatId(q.chatId as string);
        break;
      case 'countBySourceMessageId':
        result = await repo.countBySourceMessageId(q.sourceMessageId as string);
        break;
      case 'countBySourceMessageIds':
        result = await repo.countBySourceMessageIds(q.ids as string[]);
        break;
      case 'countByChatIds': {
        const map = await repo.countByChatIds(q.chatIds as string[]);
        result = Object.fromEntries(map);
        break;
      }
      case 'findDistinctChatIds':
        result = await repo.findDistinctChatIds();
        break;
      case 'findDistinctCharacterIds':
        result = await repo.findDistinctCharacterIds();
        break;
      case 'searchByContentAboutCharacter':
        result = await repo.searchByContentAboutCharacter(
          q.characterId as string,
          q.aboutCharacterId as string,
          q.query as string
        );
        break;
      // === P4.D265 (v4 `d58548051`) ===
      case 'findHotDigests':
        result = await repo.findHotDigests(q.characterId as string, q.subject as string, q.limit as number);
        break;
      case 'countHotByCharacterId':
        result = await repo.countHotByCharacterId(q.characterId as string);
        break;
      case 'countUnconsideredHot':
        result = await repo.countUnconsideredHot(q.characterId as string);
        break;
      case 'findColdIdsByCharacterId':
        result = await repo.findColdIdsByCharacterId(q.characterId as string);
        break;
      case 'findExpiredColdIds':
        result = await repo.findExpiredColdIds(q.characterId as string, q.olderThan as string);
        break;
      default:
        throw new Error(`unknown query kind: ${q.kind}`);
    }
    results.push({ kind: q.kind, result, lines: captured });
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'memories-read', queries: results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memories-read oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
