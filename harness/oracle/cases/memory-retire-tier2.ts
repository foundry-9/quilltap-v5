/**
 * Tier-2 oracle — P4.D288 (C1-7): v4's REAL retire path (`lib/memory/
 * retire.ts`, memory refactor Phase B + bugs 200 / 201 / 225 / 234, at the
 * `83334b9a5` pin) — the retire half of `memory-index-and-retire.test.ts` and
 * `retire-memories-unlink.test.ts`, over a fixture v4's own code builds.
 *
 * The case seeds `memories` through `MemoriesRepository.create` (four holders,
 * one per scenario), closes, copies the file to
 * `$QT_FIXTURE_MEMORY_RETIRE_DIR/base.db` (the Rust side's start), re-opens,
 * LOADS every holder's index (`memoryIndex.shape` — L-2,
 * `MEMORY_INDEX_ALWAYS_LOADED`, silently, before the capture starts) and runs
 * the op list, recording each op's answer and the lines it logged:
 *
 *   - H1, bug 201: `previewChatWipe`, then `retireChatMemories` sparing a
 *     MANUAL row, a cold row and a digest with a living member, retiring a
 *     digest whose only member is going (its subject bucket announced), the
 *     spared row's id staying linked; then the same wipe again (idempotence:
 *     the same spares, `Nothing to retire`);
 *   - H2, invariant 8: a digest retired with its last member; `retention`
 *     leaving its digest in place (member scrubbed out); a THREE-pass cascade
 *     up a digest chain (the fourth digest survives, scrubbed); a doomed
 *     digest's surviving members cut loose and moved cold; an empty request;
 *     an unknown id (scrubbed from a neighbour, `Nothing to retire`);
 *   - H3, the graph repair: `findDanglingLinks` scoped to the holder, then
 *     `scrubDanglingMemoryLinks` over it (a related scrub, a digest every one
 *     of whose members is gone retired, a partly dangling digest trimmed, two
 *     orphaned members cut in their own tiers), then the scan again (clean);
 *   - H4: a retirement that rewrites 200 neighbours (the WARN).
 *
 * After the ops the `memories` rows are dumped (minted `updatedAt` /
 * `supersededAt` masked on the Rust side). v4's announcement (archive
 * invalidation, the `memories` publish, the Commonplace refresh) runs inside
 * `retireMemories` here; in v5 it runs after the commit (R-AF) and is not part
 * of this family's lines (`commonplace_mirror_equivalence` pins the refresh).
 *
 * The job host is held off (`__quilltapJobHost.shuttingDown`).
 *
 * Run (Node 24, from the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-memory-retire-tier2 /tmp/oracle-memory-retire-tier2.ndjson
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MEMORY_RETIRE_DIR=/tmp/qt-memory-retire-tier2 \
 *     $N/npx tsx $V5W/harness/oracle/cases/memory-retire-tier2.ts \
 *     > /tmp/oracle-memory-retire-tier2.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

(globalThis as Record<string, unknown>).__quilltapJobHost = {
  child: null,
  spawning: false,
  shuttingDown: true,
  childCrashed: false,
  restartTimestamps: [],
  signalHandlersInstalled: false,
  invalidationListeners: new Set(),
};

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const H1 = 'e2890000-0000-4000-8000-0000000000c1';
const H2 = 'e2890000-0000-4000-8000-0000000000c2';
const H3 = 'e2890000-0000-4000-8000-0000000000c3';
const H4 = 'e2890000-0000-4000-8000-0000000000c4';
const SUBJECT = 'e2890000-0000-4000-8000-0000000000d1';
const CHAT = 'e2890000-0000-4000-8000-0000000000e1';
const mid = (n: number) => `e2890000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const T0 = '2026-01-01T00:00:00.000Z';

interface Seed {
  n: number;
  holder: string;
  source?: 'AUTO' | 'MANUAL' | 'CONSOLIDATED';
  tier?: 'hot' | 'cold';
  chat?: boolean;
  about?: string;
  related?: number[];
  supersededBy?: number;
  members?: number[];
  vector?: number[];
}

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_MEMORY_RETIRE_DIR;
  if (!dir) throw new Error('QT_FIXTURE_MEMORY_RETIRE_DIR must name the dir for base.db');
  mkdirSync(dir, { recursive: true });
  const work = join(dir, 'work.db');
  for (const p of [work, join(dir, 'base.db')]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-memory-retire-tier2-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    // retire.ts (module: memory:retire)
    'Digest retires with its last member',
    'Scrubbed digest links to retired memories',
    'Nothing to retire',
    'Retirement rewrote an unusually large neighbour set',
    'Retired memories',
    'Scrubbed dangling memory links',
    'Chat wipe spared manual, cold and digest rows',
    "Wiped a chat's extracted memories",
    'Previewed a chat wipe',
    // the repository
    'Scrubbed deleted ids from related links',
    'Found digests naming doomed members',
    'Removed members from digest',
    'Memory links updated',
    'Memory not found for link update',
    'Moved memories between tiers',
    'Scanned for dangling memory links',
    'Entities deleted',
    // the index store
    'Re-stamped vector tiers',
    'Moved memories between tiers through the index',
    'Evicted retired memories from the index',
  ]);
  const { logger } = await import('@/lib/logger');
  let capturing = false;
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!capturing || !CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemorySchema } = await import('@/lib/schemas/types');
  const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
  const ensure = async () => {
    await initializeDatabase();
    await ensureCollection('memories', MemorySchema);
  };

  // ---- seed, then hand the Rust side the same starting file ----
  const seeds: Seed[] = [
    // H1 — the chat wipe (bug 201).
    { n: 101, holder: H1, chat: true, vector: [1, 0, 0] },
    { n: 102, holder: H1, chat: true, source: 'MANUAL' },
    { n: 103, holder: H1, chat: true, tier: 'cold', vector: [0, 1, 0] },
    { n: 104, holder: H1, chat: true, source: 'CONSOLIDATED', members: [105] },
    { n: 105, holder: H1, tier: 'cold', supersededBy: 104 },
    { n: 106, holder: H1, chat: true, source: 'CONSOLIDATED', about: SUBJECT, members: [107] },
    { n: 107, holder: H1, chat: true, supersededBy: 106 },
    { n: 108, holder: H1, related: [101, 102, 107] },
    // H2 — invariant 8.
    { n: 201, holder: H2, source: 'CONSOLIDATED', members: [202] },
    { n: 202, holder: H2, tier: 'cold', supersededBy: 201, vector: [0, 0, 1] },
    { n: 203, holder: H2, source: 'CONSOLIDATED', members: [204] },
    { n: 204, holder: H2, tier: 'cold', supersededBy: 203 },
    { n: 210, holder: H2, tier: 'cold', supersededBy: 211 },
    { n: 211, holder: H2, source: 'CONSOLIDATED', tier: 'cold', members: [210], supersededBy: 212 },
    { n: 212, holder: H2, source: 'CONSOLIDATED', tier: 'cold', members: [211], supersededBy: 213 },
    { n: 213, holder: H2, source: 'CONSOLIDATED', tier: 'cold', members: [212], supersededBy: 214 },
    { n: 214, holder: H2, source: 'CONSOLIDATED', members: [213, 215] },
    { n: 215, holder: H2, tier: 'cold', supersededBy: 214 },
    { n: 220, holder: H2, source: 'CONSOLIDATED', members: [221, 222] },
    { n: 221, holder: H2, tier: 'cold', supersededBy: 220 },
    { n: 222, holder: H2, supersededBy: 220, vector: [1, 1, 0] },
    { n: 230, holder: H2, related: [202, 299] },
    // H3 — the graph repair (dangling links planted below).
    { n: 301, holder: H3, related: [302] },
    { n: 302, holder: H3 },
    { n: 303, holder: H3, source: 'CONSOLIDATED' },
    { n: 304, holder: H3, source: 'CONSOLIDATED' },
    { n: 305, holder: H3 },
    { n: 306, holder: H3, tier: 'cold' },
    // H4 — the neighbour WARN.
    { n: 400, holder: H4 },
  ];
  for (let i = 0; i < 200; i++) seeds.push({ n: 401 + i, holder: H4, related: [400] });

  await ensure();
  {
    const repo = new MemoriesRepository();
    for (const s of seeds) {
      await repo.create(
        {
          characterId: s.holder,
          content: `content ${s.n}`,
          summary: `summary ${s.n}`,
          source: s.source ?? 'AUTO',
          tier: s.tier ?? 'hot',
          ...(s.chat ? { chatId: CHAT } : {}),
          ...(s.about ? { aboutCharacterId: s.about } : {}),
          ...(s.related ? { relatedMemoryIds: s.related.map(mid) } : {}),
          ...(s.supersededBy ? { supersededById: mid(s.supersededBy) } : {}),
          ...(s.members ? { consolidatedFrom: s.members.map(mid) } : {}),
          ...(s.vector ? { embedding: new Float32Array(s.vector) } : {}),
        } as never,
        { id: mid(s.n), createdAt: T0, updatedAt: T0 },
      );
    }
    // H3's dangling links: ids that never existed.
    const gone = (n: number) => mid(900 + n);
    await rawQuery(`UPDATE memories SET relatedMemoryIds = ? WHERE id = ?`, [JSON.stringify([gone(1), mid(302)]), mid(301)]);
    await rawQuery(`UPDATE memories SET consolidatedFrom = ? WHERE id = ?`, [JSON.stringify([gone(2)]), mid(303)]);
    await rawQuery(`UPDATE memories SET consolidatedFrom = ? WHERE id = ?`, [JSON.stringify([gone(3), mid(302)]), mid(304)]);
    await rawQuery(`UPDATE memories SET supersededById = ? WHERE id = ?`, [gone(4), mid(305)]);
    await rawQuery(`UPDATE memories SET supersededById = ? WHERE id = ?`, [gone(5), mid(306)]);
  }
  await closeDatabase();
  copyFileSync(work, join(dir, 'base.db'));
  await ensure();

  const { retireMemories, retireChatMemories, previewChatWipe, scrubDanglingMemoryLinks } = await import(
    '@/lib/memory/retire'
  );
  const { memoryIndex } = await import('@/lib/memory/index-store');
  const repo = new MemoriesRepository();
  // L-2: every holder's index LOADED before the first op, silently.
  for (const h of [H1, H2, H3, H4]) await memoryIndex.shape(h, 3);
  capturing = true;

  const ops: Array<Record<string, unknown>> = [];
  const run = async (op: string, input: Record<string, unknown>, f: () => Promise<unknown>) => {
    captured = [];
    const result = await f();
    ops.push({ op, input, result, lines: captured });
  };
  const retire = (ids: number[] | string[], reason: string, keep?: 'manual') =>
    run('retire', { ids: ids.map((x) => (typeof x === 'number' ? mid(x) : x)), reason, keepManual: keep === 'manual' }, async () =>
      retireMemories(ids.map((x) => (typeof x === 'number' ? mid(x) : x)), {
        reason: reason as never,
        ...(keep ? { keep } : {}),
      }),
    );

  // H1
  await run('previewChatWipe', { chatId: CHAT }, async () => [...(await previewChatWipe(CHAT))].sort());
  await run('retireChatMemories', { chatId: CHAT }, async () => retireChatMemories(CHAT));
  await run('retireChatMemories', { chatId: CHAT }, async () => retireChatMemories(CHAT));
  // H2
  await retire([202], 'retention');
  await retire([204], 'item-delete');
  await retire([210], 'item-delete');
  await retire([220], 'item-delete');
  await retire([], 'item-delete');
  await retire([mid(299), mid(230), mid(299)], 'item-delete');
  // H3
  let links: unknown = null;
  await run('findDanglingLinks', { characterIds: [H3] }, async () => {
    links = await repo.findDanglingLinks([H3]);
    return links;
  });
  await run('scrubDanglingMemoryLinks', { characterIds: [H3] }, async () => scrubDanglingMemoryLinks(links as never));
  await run('findDanglingLinks', { characterIds: [H3] }, async () => repo.findDanglingLinks([H3]));
  // H4
  await retire([400], 'item-delete');

  capturing = false;
  const memories = await rawQuery(
    `SELECT id, characterId, source, tier, supersededById, supersededAt, consolidatedFrom, relatedMemoryIds,
            quote(embedding) AS embedding, updatedAt FROM memories ORDER BY id`,
  );
  await closeDatabase();
  process.stdout.write(JSON.stringify({ case: 'memory-retire-tier2', ops, memories }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-retire-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
