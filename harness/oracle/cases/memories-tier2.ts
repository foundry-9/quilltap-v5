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
 * ids), `markConsidered`, and (L-6, P4.D288) `updateAccessTimeBulk` — the
 * repository verb v4's `stampAccessed` calls, re-aimed from the retired
 * `markMemoriesAccessed`. Each op is
 * emitted as `{op, returned, lines}` — the op itself (so the Rust side replays
 * exactly this list), the method's answer, and the repository / service lines
 * it logged (a transport spliced into the singleton logger).
 *
 * P4.D288 (C1-7, the `83334b9a5` pin): v4 `af48f6cc2` DELETED the
 * repository's deleters (`deleteForCharacter`, `bulkDelete`, `deleteByChatId`,
 * `deleteBySourceMessageId`; the bare `delete` now REFUSES) and `e0ac7493a`
 * deleted `replaceInMemories`. The committed op list (§R.7, unchanged) is
 * RE-AIMED onto the pin's successors, the same way on both sides: `delete` →
 * `retireMemories([id], 'item-delete')`; `deleteForCharacter` /
 * `bulkDelete` → `bulkDeleteUnsafe` (character-scoped, so the not-owned no-op
 * holds); `deleteByChatId` → `retireChatMemories`; `deleteBySourceMessageId`
 * → `findBySourceMessageId` + `retireMemories(…, 'message-cascade')`;
 * `replaceInMemories` → the retired method's own body over v4's REAL
 * `findById` / `update` (`REPLACE_IN_MEMORIES_OVER_PRIMITIVES`). A KEYSTONE
 * phase follows the tier phase — every new repository verb (counted
 * reinforcement with `by` / forward-only / the refusal, the composing link
 * deltas, the everywhere scrub, the footnotes cap, `bulkDeleteUnsafe`, the
 * digest member delta, the cap breakdown, the dangling scan, the three digest
 * lookups, bug 215's recap bands, `supersededAt` in all three states) — each
 * `{op, returned, lines}` like the tier phase; the four repository jest
 * files' (`memories-{link-deltas,cap-breakdown,bulk-delete,recap-bands}`)
 * rows are its seeds.
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
    // P4.D288: the keystone phase's repository lines.
    'Memory links updated',
    'Memory not found for link update',
    'Scrubbed deleted ids from related links',
    'Reinforcement footnotes appended',
    'Memory not found for reinforcement footnotes',
    'Entities deleted',
    'Removed members from digest',
    'Found digests naming doomed members',
    'Scanned for dangling memory links',
    'Fetched recap importance band',
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

  const { retireMemories, retireChatMemories } = await import('@/lib/memory/retire');
  // Hold off v4's job host (the retire's announcement may enqueue nothing,
  // but a runner claiming mid-case would race the dump).
  (globalThis as Record<string, unknown>).__quilltapJobHost = { shuttingDown: true };

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
      case 'replaceInMemories': {
        // REPLACE_IN_MEMORIES_OVER_PRIMITIVES: the retired method's body
        // (`split(search).join(replace)` on content / summary / keywords,
        // `update` only when something changed) over v4's REAL repository.
        const search = op.search as string;
        const rep = (t: string) => t.split(search).join(op.replace as string);
        for (const id of op.ids as string[]) {
          const m = await repo.findById(id);
          if (!m) continue;
          const keywords = m.keywords ?? [];
          const changed =
            m.content.includes(search) || m.summary.includes(search) || keywords.some((k) => k.includes(search));
          if (changed) {
            await repo.update(id, { content: rep(m.content), summary: rep(m.summary), keywords: keywords.map(rep) } as never);
          }
        }
        break;
      }
      case 'delete':
        await retireMemories([op.id as string], { reason: 'item-delete' });
        break;
      case 'deleteForCharacter':
        await repo.bulkDeleteUnsafe(op.characterId as string, [op.memoryId as string]);
        break;
      case 'bulkDelete':
        await repo.bulkDeleteUnsafe(op.characterId as string, op.ids as string[]);
        break;
      case 'deleteByChatId':
        await retireChatMemories(op.chatId as string);
        break;
      case 'deleteBySourceMessageId': {
        const rows = await repo.findBySourceMessageId(op.sourceMessageId as string);
        await retireMemories(rows.map((m) => m.id), { reason: 'message-cascade' });
        break;
      }
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
  // L-6 (P4.D288): `lib/memory/memory-service.ts` is gone at the pin — the
  // two `markMemoriesAccessed` ops are RE-AIMED at the repository verb v4's
  // `recall/deliver.ts` `stampAccessed` calls (`updateAccessTimeBulk`, its
  // answer recorded); delivery semantics are P4.D291's.
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
    // A supersededById that SURVIVES to the dump (op above's 101 is reset by
    // the hot move): a dropped `supersededById` SET arm reddens the table.
    { kind: 'updateTierBulk', characterId: T, ids: [tid(105)], tier: 'cold', extra: { supersededById: tid(103) } },
    { kind: 'updateTierBulk', characterId: T, ids: [], tier: 'cold', extra: {} },
    { kind: 'updateTierBulk', characterId: T, ids: [...ghosts, tid(104)], tier: 'cold', extra: {} },
    { kind: 'markConsidered', characterId: T, ids: [tid(103), tid(104), tid(106)], when: at(6) },
    { kind: 'updateAccessTimeBulk', characterId: T, ids: [tid(101), '', tid(101), tid(103)] },
    { kind: 'updateAccessTimeBulk', characterId: T, ids: [] },
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
      case 'updateAccessTimeBulk':
        returned = await repo.updateAccessTimeBulk(op.characterId as string, op.ids as string[]);
        break;
      default:
        throw new Error(`unknown tier op: ${op.kind}`);
    }
    tier.push({ op, returned, lines: captured });
  }
  captured = [];

  // === P4.D288: the keystone phase (see the header) ===
  const K = 'd2880000-0000-4000-8000-0000000000a1';
  const KO = 'd2880000-0000-4000-8000-0000000000a2';
  const kid = (n: number) => `d2880000-0000-4000-8000-${String(n).padStart(12, '0')}`;
  const kat = (n: number) => `2026-05-0${n}T00:00:00.000Z`;
  const kopt = (n: number, created = kat(1)) => ({ id: kid(n), createdAt: created, updatedAt: kat(1) });
  const kmem = (characterId: string, extra: Record<string, unknown> = {}) => ({
    characterId,
    content: 'keystone phase',
    summary: 'keystone',
    importance: 0.5,
    ...extra,
  });
  const ri = (n: number, v: number, created: string) => ({
    kind: 'create',
    data: kmem(K, { importance: v, reinforcedImportance: v, source: 'AUTO' }),
    options: kopt(n, created),
  });
  const keystoneOps: Array<Record<string, unknown>> = [
    // Seeds: links, footnotes, a digest pair, a dangling graph, the cap mix.
    { kind: 'create', data: kmem(K, { relatedMemoryIds: [kid(99)] }), options: kopt(1) },
    { kind: 'create', data: kmem(K, { content: 'Friday keeps the ledger.' }), options: kopt(2) },
    { kind: 'create', data: kmem(KO, { relatedMemoryIds: [kid(1), kid(2)] }), options: kopt(3) },
    {
      kind: 'create',
      data: kmem(K, { source: 'CONSOLIDATED', aboutCharacterId: KO, consolidatedFrom: [kid(5), kid(6), kid(77)] }),
      options: kopt(4),
    },
    { kind: 'create', data: kmem(K, { tier: 'cold', supersededById: kid(4), supersededAt: kat(2) }), options: kopt(5) },
    { kind: 'create', data: kmem(K, { tier: 'cold', supersededById: kid(4), supersededAt: null }), options: kopt(6) },
    { kind: 'create', data: kmem(K, { tier: 'cold', supersededById: kid(88) }), options: kopt(7) },
    { kind: 'create', data: kmem(K, { source: 'MANUAL' }), options: kopt(8) },
    // Counted reinforcement: by, forward-only, the refusal, a miss.
    { kind: 'incrementReinforcement', characterId: K, memoryId: kid(2), at: kat(4), by: 3 },
    { kind: 'incrementReinforcement', characterId: K, memoryId: kid(2), at: kat(3) },
    { kind: 'incrementReinforcement', characterId: K, memoryId: kid(2), at: kat(5), by: 0 },
    { kind: 'incrementReinforcement', characterId: KO, memoryId: kid(2), at: kat(5), by: 2 },
    // The link deltas compose; self + present skipped; unchanged → no write.
    { kind: 'addRelatedMemoryIds', characterId: K, memoryId: kid(1), ids: [kid(2)] },
    { kind: 'addRelatedMemoryIds', characterId: K, memoryId: kid(1), ids: [kid(8), kid(2), kid(1), '', kid(8)] },
    { kind: 'removeRelatedMemoryIds', characterId: K, memoryId: kid(1), ids: [kid(42)] },
    { kind: 'removeRelatedMemoryIds', characterId: K, memoryId: kid(1), ids: [kid(8), kid(42)] },
    { kind: 'addRelatedMemoryIds', characterId: KO, memoryId: kid(1), ids: [kid(8)] },
    { kind: 'plant', sql: `UPDATE memories SET relatedMemoryIds = NULL WHERE id = '${kid(8)}'` },
    { kind: 'addRelatedMemoryIds', characterId: K, memoryId: kid(8), ids: [kid(2)] },
    // The everywhere scrub — across holders, an invalid cell untouched.
    { kind: 'plant', sql: `UPDATE memories SET relatedMemoryIds = 'not json' WHERE id = '${kid(6)}'` },
    { kind: 'removeRelatedMemoryIdsEverywhere', ids: [kid(2), kid(2)] },
    { kind: 'removeRelatedMemoryIdsEverywhere', ids: [] },
    // Footnotes against the live content; the cap; a miss.
    { kind: 'appendReinforcementFootnotes', characterId: K, memoryId: kid(2), details: ['Marguerite'] },
    { kind: 'appendReinforcementFootnotes', characterId: K, memoryId: kid(2), details: ['Valparaiso', 'Marguerite'] },
    {
      kind: 'appendReinforcementFootnotes',
      characterId: K,
      memoryId: kid(2),
      details: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'],
    },
    { kind: 'appendReinforcementFootnotes', characterId: KO, memoryId: kid(2), details: ['x'] },
    // The reads.
    { kind: 'countCapBreakdown', characterId: K },
    { kind: 'countCapBreakdown', characterId: kid(999) },
    { kind: 'findDanglingLinks' },
    { kind: 'findDanglingLinks', characterIds: [K, K] },
    { kind: 'findDanglingLinks', characterIds: [] },
    { kind: 'findDigestsContainingMembers', characterIds: [K, KO], memberIds: [kid(5), kid(77)] },
    { kind: 'findDigestsContainingMembers', characterIds: [], memberIds: [kid(5)] },
    { kind: 'findIdsSupersededByAny', characterIds: [K], digestIds: [kid(4), kid(88)] },
    { kind: 'findDigestSubjectsByIds', ids: [kid(4), kid(1), kid(4)] },
    // The digest member delta (a dedupe, an absent member, another holder).
    { kind: 'removeConsolidatedMembers', characterId: K, digestId: kid(4), ids: [kid(77), kid(77), kid(0)] },
    { kind: 'removeConsolidatedMembers', characterId: KO, digestId: kid(4), ids: [kid(5)] },
    { kind: 'removeConsolidatedMembers', characterId: K, digestId: kid(4), ids: [] },
    // supersededAt moves with supersededById: stamped, cut, untouched.
    { kind: 'updateTierBulk', characterId: K, ids: [kid(8)], tier: 'cold', extra: { supersededById: kid(4) } },
    { kind: 'updateTierBulk', characterId: K, ids: [kid(5)], tier: 'cold', extra: { supersededById: null } },
    { kind: 'updateTierBulk', characterId: K, ids: [kid(6)], tier: 'hot', extra: {} },
    // Bug 215's recap bands on reinforcedImportance alone.
    ri(20, 0.8, '2026-10-01T00:01:00.000Z'),
    ri(21, 0.7, '2026-10-01T00:02:00.000Z'),
    ri(22, 0.69, '2026-10-01T00:03:00.000Z'),
    ri(23, 0.3, '2026-10-01T00:04:00.000Z'),
    ri(24, 0.29, '2026-10-01T00:05:00.000Z'),
    { kind: 'plant', sql: `UPDATE memories SET importance = 0.65 WHERE id = '${kid(20)}'` },
    { kind: 'findRecentByImportanceTier', characterId: K, limits: {} },
    { kind: 'findRecentByImportanceTier', characterId: K, limits: { high: 1, medium: 0, low: 5 } },
    // bulkDeleteUnsafe last: another holder's id untouched, then empty.
    { kind: 'bulkDeleteUnsafe', characterId: K, ids: [kid(24), kid(3), kid(1000)] },
    { kind: 'bulkDeleteUnsafe', characterId: K, ids: [] },
  ];
  const keystone: Array<Record<string, unknown>> = [];
  for (const op of keystoneOps) {
    captured = [];
    let returned: unknown = null;
    const ids = (k: string) => (op[k] as string[]) ?? [];
    try {
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
            ...(op.by === undefined ? [] : [op.by as number]),
          );
          break;
        case 'addRelatedMemoryIds':
          returned = await repo.addRelatedMemoryIds(op.characterId as string, op.memoryId as string, ids('ids'));
          break;
        case 'removeRelatedMemoryIds':
          returned = await repo.removeRelatedMemoryIds(op.characterId as string, op.memoryId as string, ids('ids'));
          break;
        case 'removeRelatedMemoryIdsEverywhere':
          returned = await repo.removeRelatedMemoryIdsEverywhere(ids('ids'));
          break;
        case 'appendReinforcementFootnotes':
          returned = await repo.appendReinforcementFootnotes(
            op.characterId as string,
            op.memoryId as string,
            ids('details'),
          );
          break;
        case 'countCapBreakdown':
          returned = await repo.countCapBreakdown(op.characterId as string);
          break;
        case 'findDanglingLinks':
          returned = await repo.findDanglingLinks(op.characterIds as string[] | undefined);
          break;
        case 'findDigestsContainingMembers':
          returned = await repo.findDigestsContainingMembers(ids('characterIds'), ids('memberIds'));
          break;
        case 'findIdsSupersededByAny':
          returned = await repo.findIdsSupersededByAny(ids('characterIds'), ids('digestIds'));
          break;
        case 'findDigestSubjectsByIds':
          returned = await repo.findDigestSubjectsByIds(ids('ids'));
          break;
        case 'removeConsolidatedMembers':
          returned = await repo.removeConsolidatedMembers(
            op.characterId as string,
            op.digestId as string,
            ids('ids'),
          );
          break;
        case 'updateTierBulk':
          returned = await repo.updateTierBulk(
            op.characterId as string,
            ids('ids'),
            op.tier as 'hot' | 'cold',
            op.extra as never,
          );
          break;
        case 'findRecentByImportanceTier': {
          const r = await repo.findRecentByImportanceTier(op.characterId as string, op.limits as never);
          returned = { high: r.high.map((m) => m.id), medium: r.medium.map((m) => m.id), low: r.low.map((m) => m.id) };
          break;
        }
        case 'bulkDeleteUnsafe':
          returned = await repo.bulkDeleteUnsafe(op.characterId as string, ids('ids'));
          break;
        default:
          throw new Error(`unknown keystone op: ${op.kind}`);
      }
    } catch (err) {
      returned = { error: err instanceof Error ? err.message : String(err) };
    }
    keystone.push({ op, returned, lines: captured });
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
  process.stdout.write(JSON.stringify({ case: 'memories-tier2', ...dump, errors, tier, keystone }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memories-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
