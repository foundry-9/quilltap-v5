/**
 * Tier-2 oracle case — the `memories` repo (Phase-2, main DB).
 *
 * Proves what state v4 leaves the `memories` table in after a fixed
 * create / update / mutate / delete sequence, so the Rust port can be diffed
 * against it (structural DB diff). Exercises the full write/mutation surface:
 * `create` (rich + minimal, embedding BLOB + null), `update`,
 * `updateForCharacter` (owned + not-owned no-op), `updateAccessTime{,Bulk}`,
 * `replaceInMemories` (literal substring replace, case-sensitive),
 * `deleteForCharacter` (not-owned no-op), `bulkDelete`, `delete`,
 * `deleteByChatId`, `deleteBySourceMessageId{,s}`.
 *
 * Flow (mirrors conversation-chunks-tier2.ts):
 *   1. copy the SEED (empty-table) fixture to a fresh working copy;
 *   2. open it through v4's real `initializeDatabase()` and run the op sequence
 *      from `memories-tier2.json` via the real `MemoriesRepository`;
 *   3. close, then dump `memories` canonically (RAW on-disk cells via `rawQuery`:
 *      the embedding cell is a raw Buffer → canonValue lowercase hex / null) and
 *      emit it as one NDJSON row.
 *
 * NORMALIZATION SPEC: minted-timestamp placeholder. ids + createdAt + every
 * payload column are pinned/deterministic; only `updatedAt` (bumped by every
 * mutator) and `lastAccessedAt` (set by updateAccessTime{,Bulk}) are minted and
 * get collapsed to `<ts>` by the Rust harness on BOTH dumps. This case emits the
 * raw (un-collapsed) dump.
 *
 * P4.D265 (v4 `d58548051` / `70f9b495c`): a TIER phase runs after the op
 * sequence and before the dump — creates with and without the tier keys
 * (incl. `null` → the repository's prefill, and a `CONSOLIDATED` digest),
 * `incrementReinforcement` (hit / miss / wrong character / a planted NULL
 * count), `updateTierBulk` (cold with extras, a foreign id untouched, hot with
 * `supersededById: null`, a bare demotion, `[]`, > `SQLITE_VARIABLE_CHUNK_SIZE`
 * ids), `markConsidered`, and v4's REAL `markMemoriesAccessed` (its
 * fire-and-forget flushed with `setImmediate` before the next op). Each op is
 * emitted as `{op, returned, lines}` — the op itself (so the Rust side replays
 * exactly this list), the method's answer, and the repository / service lines
 * it logged (a transport spliced into the singleton logger).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEM=/tmp/qt-mem-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memories-tier2.ts > /tmp/oracle-mem.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: string;
  id?: string;
  memoryId?: string;
  characterId?: string;
  chatId?: string;
  sourceMessageId?: string;
  ids?: string[];
  search?: string;
  replace?: string;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'memories-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_MEM;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_MEM must point at the seed fixture from build-memories-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-mem-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'mem-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  // P4.D265: the tier phase's lines (see the header).
  const CAPTURED = new Set([
    'Memory not found for reinforcement',
    'Memory reinforcement counted',
    'Moved memories between tiers',
    '[Memory] Marking memories accessed',
  ]);
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
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemoriesRepository } = await import(
    '@/lib/database/repositories/memories.repository'
  );

  await initializeDatabase();
  const repo = new MemoriesRepository();

  for (const op of spec.ops) {
    switch (op.kind) {
      case 'create':
        await repo.create(op.data as never, op.options);
        break;
      case 'update':
        await repo.update(op.id as string, op.data as never);
        break;
      case 'updateForCharacter':
        await repo.updateForCharacter(op.characterId as string, op.memoryId as string, op.data as never);
        break;
      case 'updateAccessTime':
        await repo.updateAccessTime(op.characterId as string, op.memoryId as string);
        break;
      case 'updateAccessTimeBulk':
        await repo.updateAccessTimeBulk(op.characterId as string, op.ids as string[]);
        break;
      case 'replaceInMemories':
        await repo.replaceInMemories(op.ids as string[], op.search as string, op.replace as string);
        break;
      case 'delete':
        await repo.delete(op.id as string);
        break;
      case 'deleteForCharacter':
        await repo.deleteForCharacter(op.characterId as string, op.memoryId as string);
        break;
      case 'bulkDelete':
        await repo.bulkDelete(op.characterId as string, op.ids as string[]);
        break;
      case 'deleteByChatId':
        await repo.deleteByChatId(op.chatId as string);
        break;
      case 'deleteBySourceMessageId':
        await repo.deleteBySourceMessageId(op.sourceMessageId as string);
        break;
      case 'deleteBySourceMessageIds':
        // v4 `ad1c4c37f` deletes `deleteBySourceMessageIds` as dead code (v5
        // never had a production caller either — retired, not ported). The
        // committed fixture keeps its final op unchanged (§R.7/no-fixture-
        // edit); no-op here (matching the Rust side's retirement) rather than
        // call the deleted method.
        break;
      default:
        throw new Error(`unknown op kind: ${op.kind}`);
    }
  }

  // === P4.D265: the tier phase ===
  const { markMemoriesAccessed } = await import('@/lib/memory/memory-service');
  const T = 'd2650000-0000-4000-8000-0000000000a1';
  const O = 'd2650000-0000-4000-8000-0000000000a2';
  const tid = (n: number) => `d2650000-0000-4000-8000-${String(n).padStart(12, '0')}`;
  const at = (n: number) => `2026-05-0${n}T00:00:00.000Z`;
  const opt = (n: number) => ({ id: tid(n), createdAt: at(1), updatedAt: at(1) });
  const mem = (characterId: string, extra: Record<string, unknown> = {}) => ({
    characterId,
    content: 'tier phase',
    summary: 'tier',
    importance: 0.5,
    ...extra,
  });
  const ghosts = Array.from({ length: 905 }, (_, i) => tid(9000 + i));
  const tierOps: Array<Record<string, unknown>> = [
    { kind: 'create', data: mem(T), options: opt(101) },
    {
      kind: 'create',
      data: mem(T, { tier: 'cold', supersededById: tid(103), consolidatedAt: at(2) }),
      options: opt(102),
    },
    {
      kind: 'create',
      data: mem(T, { source: 'CONSOLIDATED', consolidatedFrom: [tid(101), tid(102)], importance: 0.8 }),
      options: opt(103),
    },
    { kind: 'create', data: mem(T, { tier: null, consolidatedFrom: null }), options: opt(104) },
    { kind: 'create', data: mem(T, { importance: 0.7 }), options: opt(105) },
    { kind: 'create', data: mem(O), options: opt(106) },
    { kind: 'incrementReinforcement', characterId: T, memoryId: tid(101), at: at(3) },
    { kind: 'incrementReinforcement', characterId: T, memoryId: tid(999), at: at(3) },
    { kind: 'incrementReinforcement', characterId: O, memoryId: tid(101), at: at(3) },
    { kind: 'plant', sql: `UPDATE memories SET reinforcementCount = NULL WHERE id = '${tid(105)}'` },
    { kind: 'incrementReinforcement', characterId: T, memoryId: tid(105), at: at(4) },
    {
      kind: 'updateTierBulk',
      characterId: T,
      ids: [tid(101), tid(106)],
      tier: 'cold',
      extra: { supersededById: tid(103), consolidatedAt: at(5) },
    },
    { kind: 'updateTierBulk', characterId: T, ids: [tid(101)], tier: 'hot', extra: { supersededById: null } },
    { kind: 'updateTierBulk', characterId: T, ids: [tid(105)], tier: 'cold', extra: {} },
    { kind: 'updateTierBulk', characterId: T, ids: [], tier: 'cold', extra: {} },
    { kind: 'updateTierBulk', characterId: T, ids: [...ghosts, tid(104)], tier: 'cold', extra: {} },
    { kind: 'markConsidered', characterId: T, ids: [tid(103), tid(104), tid(106)], when: at(6) },
    { kind: 'markMemoriesAccessed', characterId: T, ids: [tid(101), '', tid(101), tid(103)] },
    { kind: 'markMemoriesAccessed', characterId: T, ids: [] },
  ];
  const tier: Array<Record<string, unknown>> = [];
  for (const op of tierOps) {
    captured = [];
    let returned: unknown = null;
    switch (op.kind) {
      case 'create':
        await repo.create(op.data as never, op.options as never);
        break;
      case 'plant':
        await rawQuery(op.sql as string);
        break;
      case 'incrementReinforcement':
        returned = await repo.incrementReinforcement(
          op.characterId as string,
          op.memoryId as string,
          op.at as string,
        );
        break;
      case 'updateTierBulk':
        returned = await repo.updateTierBulk(
          op.characterId as string,
          op.ids as string[],
          op.tier as 'hot' | 'cold',
          op.extra as never,
        );
        break;
      case 'markConsidered':
        returned = await repo.markConsidered(op.characterId as string, op.ids as string[], op.when as string);
        break;
      case 'markMemoriesAccessed':
        markMemoriesAccessed(op.characterId as string, op.ids as string[]);
        // Fire-and-forget: let the repository write land before the next op.
        for (let i = 0; i < 5; i++) await new Promise((r) => setImmediate(r));
        break;
      default:
        throw new Error(`unknown tier op: ${op.kind}`);
    }
    tier.push({ op, returned, lines: captured });
  }
  captured = [];

  // P4.161 (P4.155's R-E twin): creates v4's `MemorySchema` refuses at
  // `_create`'s `validate` — run AFTER the op sequence (a refused create writes
  // nothing, so the dump below is unchanged by them) and recorded as `{label,
  // data, id, message}`; the Rust side replays each through
  // `parse_create_memory` and compares the message bytes. Kept here, not in
  // `memories-tier2.json` (that fixture is not P4.161's file).
  const CHAR = 'c1610000-0000-4000-8000-0000000000b1';
  const base = { characterId: CHAR, content: 'Refused.', summary: 'refused' };
  const createExpectErrors: Array<{ label: string; data: Record<string, unknown>; id?: string }> = [
    { label: 'c6-walk', data: { ...base, importance: 5, kind: 'bogus-kind' } },
    { label: 'content-absent', data: { characterId: CHAR, summary: 'refused' } },
    { label: 'reinforcement-zero', data: { ...base, reinforcementCount: 0 } },
    { label: 'tags-not-uuid', data: { ...base, tags: ['nope'] } },
    { label: 'claimed-id-not-uuid', data: base, id: 'not-a-uuid' },
  ];
  const errors: Array<Record<string, unknown>> = [];
  for (const c of createExpectErrors) {
    let message: string | null = null;
    try {
      await repo.create(c.data as never, c.id ? ({ id: c.id } as never) : undefined);
    } catch (err) {
      message = err instanceof Error ? err.message : String(err);
    }
    errors.push({ label: c.label, data: c.data, ...(c.id ? { id: c.id } : {}), message });
  }

  const columns = (
    (await rawQuery('PRAGMA table_info(memories)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = (await rawQuery('SELECT * FROM memories')) as Array<Record<string, unknown>>;

  await closeDatabase();

  const dump = canonicalizeRows({ table: 'memories', columns, rawRows, orderBy: 'id' });
  process.stdout.write(JSON.stringify({ case: 'memories-tier2', ...dump, errors, tier }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memories-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
