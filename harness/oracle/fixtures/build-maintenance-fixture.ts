/**
 * The memory-maintenance fixture builder (P4.D297) — the SEED pair the
 * `memory_maintenance_tier2` family copies (`memory-maintenance-tier2.json`).
 *
 * Built through v4's REAL repositories at the `83334b9a5` pin (the memory tier
 * columns native; every id a UUID — `MemorySchema` refuses anything else).
 * Each memory's vector is its row's own `embedding` (a Float32 LE BLOB — the
 * only copy the index loads, P4.D288). Holders:
 *
 *   - Fenwick (`folder`): near-duplicate groups for the fold — a survivor and
 *     its absorbed rows (novel details for the footnotes, entities and links
 *     to merge), a non-transitive chain, a date-guarded pair, an episodic twin
 *     (another bucket), a MANUAL twin and a cold twin (ineligible);
 *   - Dorrit (`demoter`): 30 hot AUTO rows of graded protection, three MANUAL
 *     rows, two hot digests, one row whose `tier` is NULL (reads hot);
 *   - Rosalind (`retirer`): cold superseded AUTO rows past / inside the
 *     retention window (`supersededAt`, and the `updatedAt` fallback), a cold
 *     superseded MANUAL row, a digest whose every member expired;
 *   - Mabel (`mixed`): near-duplicates + 105 demotable rows over her 100 cap
 *     override + expired cold rows — the whole automatic run;
 *   - Ghost (`archived`) and Ordway (`order`, mixed-case ids — the corpus is
 *     read `ORDER BY id ASC`, binary).
 *
 * `instance_settings` (created before the settings plants — a builder runs no
 * migrations) carries the spec's `memoryMaintenance` + `memoryConsolidation`;
 * `background_jobs` is created empty (v4 creates it lazily).
 *
 * Run from the v4 checkout (or the round's pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=${QT_FIXTURE_OUT:-/tmp/qt-maintenance-main.db} \
 *   QT_FIXTURE_MOUNT_OUT=${QT_FIXTURE_MOUNT_OUT:-/tmp/qt-maintenance-mount.db} \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-maintenance-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

const DAY = 86_400_000;

interface Named {
  id: string;
  name: string;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  now: string;
  subject: Named;
  holders: Record<'folder' | 'demoter' | 'retirer' | 'mixed' | 'archived' | 'order', Named>;
  settings: { maintenance: Record<string, unknown>; consolidation: Record<string, unknown> };
}

function unit(values: number[]): number[] {
  const n = Math.sqrt(values.reduce((s, v) => s + v * v, 0));
  return values.map((v) => Math.fround(v / n));
}

/** A unit vector at `cos` from `base` (both 8-dim; `axis` picks the orthogonal direction). */
function at(base: number[], cos: number, axis: number): number[] {
  const b = unit(base);
  const o = Array.from({ length: 8 }, (_, i) => (i === axis ? 1 : 0));
  const dot = o.reduce((s, v, i) => s + v * b[i], 0);
  const perp = unit(o.map((v, i) => v - dot * b[i]));
  const sin = Math.sqrt(1 - cos * cos);
  return unit(b.map((v, i) => cos * v + sin * perp[i]));
}

function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'memory-maintenance-tier2.json'), 'utf8')) as Spec;
  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(base + suffix)) rmSync(base + suffix);
    }
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-maintenance-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { DocMountFileSchema, DocMountDocumentSchema, DocMountFolderSchema, DocMountFileLinkSchema } =
    await import('@/lib/schemas/mount-index.types');
  await initializeDatabase();
  // The character vault's document tables (a character create scaffolds its
  // vault; the consolidation builder's DDL).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const [name, schema] of [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ] as Array<[string, unknown]>) {
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
  const repos = getRepositories();
  const TS = spec.seedTimestamp;
  const NOW = Date.parse(spec.now);
  const iso = (agoDays: number) => new Date(NOW - agoDays * DAY).toISOString();

  const maindb = (await import('@/lib/database/backends/sqlite/client')).getRawDatabase?.();
  if (!maindb) throw new Error('main DB handle unavailable');
  maindb.exec('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  const { BackgroundJobSchema } = await import('@/lib/schemas/types');
  for (const sql of generateDDL('background_jobs', BackgroundJobSchema as never)) maindb.exec(sql);
  const settings = await import('@/lib/instance-settings');
  await settings.setMemoryMaintenanceSettings(spec.settings.maintenance as never);
  await settings.setMemoryConsolidationSettings(spec.settings.consolidation as never);

  // v4's repositories create `embedding_profiles` lazily (a fresh instance's
  // default-profile read answers null); every v5 instance carries the table.
  const { EmbeddingProfileSchema } = await import('@/lib/schemas/types');
  for (const sql of generateDDL('embedding_profiles', EmbeddingProfileSchema as never)) maindb.exec(sql);

  // ── the user (the route families' session resolves it) ──────────────────
  await repos.users.create(
    { username: 'maintainer', email: null, name: 'Maintainer' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // ── characters ──────────────────────────────────────────────────────────
  for (const c of [spec.subject, ...Object.values(spec.holders)]) {
    await repos.characters.create(
      { name: c.name, userId: spec.userId, controlledBy: 'llm' } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  maindb.prepare('UPDATE characters SET archivedAt = ? WHERE id = ?').run(TS, spec.holders.archived.id);

  // ── memories ────────────────────────────────────────────────────────────
  interface Row {
    id: string;
    content: string;
    about?: string | null;
    importance?: number;
    reinforcedImportance?: number;
    reinforcementCount?: number;
    embedding?: number[] | null;
    source?: 'AUTO' | 'MANUAL' | 'CONSOLIDATED';
    kind?: 'semantic' | 'episodic';
    tier?: 'hot' | 'cold';
    occurredAt?: string | null;
    entities?: string[];
    related?: string[];
    createdAgo?: number;
    lastReinforcedAgo?: number;
    supersededById?: string | null;
    consolidatedFrom?: string[];
  }
  const put = async (holder: Named, r: Row) => {
    const created = iso(r.createdAgo ?? 30);
    await repos.memories.create(
      {
        characterId: holder.id,
        aboutCharacterId: r.about === undefined ? spec.subject.id : r.about,
        chatId: null,
        projectId: null,
        content: r.content,
        summary: r.content.slice(0, 40).toLowerCase(),
        keywords: [],
        tags: [],
        importance: r.importance ?? 0.5,
        embedding: r.embedding ?? null,
        source: r.source ?? 'AUTO',
        witnessedContext: null,
        occurredAt: r.occurredAt ?? null,
        narrativeTime: null,
        entities: r.entities ?? [],
        kind: r.kind ?? 'semantic',
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: r.reinforcementCount ?? 1,
        lastReinforcedAt: r.lastReinforcedAgo !== undefined ? iso(r.lastReinforcedAgo) : null,
        relatedMemoryIds: r.related ?? [],
        reinforcedImportance: r.reinforcedImportance ?? r.importance ?? 0.5,
        tier: r.tier ?? 'hot',
        supersededById: r.supersededById ?? null,
        consolidatedFrom: r.consolidatedFrom ?? [],
        consolidatedAt: null,
      } as never,
      { id: r.id, createdAt: created, updatedAt: created },
    );
  };
  const id = (prefix: string, n: number) => `${prefix}-0000-4000-8000-${n.toString(16).padStart(12, '0')}`;

  // Fenwick — the fold.
  {
    const h = spec.holders.folder;
    const T = [1, 0.1, 0, 0, 0, 0, 0, 0];
    const C = [0, 0, 1, 0.1, 0, 0, 0, 0];
    const E = [0, 0, 0, 0, 1, 0.2, 0, 0];
    const rows: Row[] = [
      { id: id('4a010001', 1), content: 'Laura keeps a brass telescope by the east window.', importance: 0.8, reinforcementCount: 2, embedding: unit(T), entities: ['Laura', 'telescope'] },
      { id: id('4a010001', 2), content: 'Laura keeps a brass telescope by the east window and polishes it every Sunday.', importance: 0.5, embedding: at(T, 0.97, 5), entities: ['Laura', 'Sunday'], related: [id('4a010001', 9)] },
      { id: id('4a010001', 3), content: 'Laura keeps a brass telescope near the window; it was her grandfather’s.', importance: 0.4, embedding: at(T, 0.95, 6), entities: ['grandfather'], lastReinforcedAgo: 2 },
      // A chain: c1 ≈ c2 ≈ c3 but c1 and c3 apart (non-transitive).
      { id: id('4a010001', 4), content: 'Laura walks the sea wall at dawn.', importance: 0.9, embedding: unit(C) },
      { id: id('4a010001', 5), content: 'Laura walks the sea wall at dawn with her dog Pickle.', importance: 0.3, embedding: at(C, 0.93, 6) },
      { id: id('4a010001', 6), content: 'Laura and Pickle walk the harbour wall before breakfast.', importance: 0.3, embedding: at(at(C, 0.93, 6), 0.93, 7) },
      // Different occasions (the date guard) — the same vector, 30 days apart.
      { id: id('4a010001', 7), content: 'Laura visited the lighthouse on the first.', importance: 0.6, embedding: unit(E), occurredAt: '2026-08-01T10:00:00.000Z', kind: 'episodic' },
      { id: id('4a010001', 8), content: 'Laura visited the lighthouse again on the thirty-first.', importance: 0.6, embedding: at(E, 0.99, 7), occurredAt: '2026-08-31T10:00:00.000Z', kind: 'episodic' },
      { id: id('4a010001', 9), content: 'Laura collects old charts.', importance: 0.5, embedding: unit([0, 1, 0, 0, 0, 0, 0, 0.1]) },
      // An episodic twin of the telescope (another bucket), a MANUAL twin and a cold twin.
      { id: id('4a010001', 10), content: 'Laura keeps a brass telescope by the east window (an episode).', embedding: at(T, 0.99, 4), kind: 'episodic' },
      { id: id('4a010001', 11), content: 'Laura keeps a brass telescope (written by hand).', embedding: at(T, 0.99, 3), source: 'MANUAL' },
      { id: id('4a010001', 12), content: 'Laura keeps a brass telescope (gone cold).', embedding: at(T, 0.99, 2), tier: 'cold' },
      // About the holder (self bucket) — no near-duplicate.
      { id: id('4a010001', 13), content: 'I keep the ledger in green ink.', about: h.id, embedding: unit([0, 0, 0, 0, 0, 0, 1, 1]) },
    ];
    for (const r of rows) await put(h, r);
  }

  // Dorrit — the demotion.
  {
    const h = spec.holders.demoter;
    const r = rng(2970);
    for (let i = 1; i <= 30; i++) {
      await put(h, {
        id: id('4a020001', i),
        content: `Dorrit note ${i}.`,
        importance: Math.round((0.1 + r() * 0.8) * 100) / 100,
        reinforcementCount: 1 + (i % 4),
        createdAgo: 5 + (i % 9) * 11,
        lastReinforcedAgo: i % 5 === 0 ? 1 : undefined,
        related: i % 7 === 0 ? [id('4a020001', 1)] : [],
        kind: i % 6 === 0 ? 'episodic' : 'semantic',
      });
    }
    for (let i = 1; i <= 3; i++) await put(h, { id: id('4a020002', i), content: `Dorrit wrote this ${i}.`, source: 'MANUAL', importance: 0.1 });
    for (let i = 1; i <= 2; i++) await put(h, { id: id('4a020003', i), content: `Dorrit digest ${i}.`, source: 'CONSOLIDATED', importance: 0.2 });
    // Rows whose protection ties exactly (same fields, different createdAt —
    // past ~100 days the content decay sits on its floor): oldest first.
    await put(h, { id: id('4a020004', 1), content: 'Dorrit tie a.', importance: 0.15, createdAgo: 200 });
    await put(h, { id: id('4a020004', 2), content: 'Dorrit tie b.', importance: 0.15, createdAgo: 300 });
    await put(h, { id: id('4a020004', 3), content: 'Dorrit tie c.', importance: 0.15, createdAgo: 250 });
    maindb.prepare('UPDATE memories SET tier = NULL WHERE id = ?').run(id('4a020001', 30));
  }

  // Rosalind — the retention.
  {
    const h = spec.holders.retirer;
    const digest = id('4a030002', 1);
    const digest2 = id('4a030002', 2);
    const members = [1, 2, 3, 4, 5].map((n) => id('4a030001', n));
    const members2 = [6, 7].map((n) => id('4a030001', n));
    await put(h, { id: digest, content: 'Rosalind digest of the garden.', source: 'CONSOLIDATED', consolidatedFrom: [...members, id('4a030001', 8)] });
    await put(h, { id: digest2, content: 'Rosalind digest of the orchard.', source: 'CONSOLIDATED', consolidatedFrom: members2 });
    const superseded = async (n: number, supersededAgo: number | null, updatedAgo: number, extra: Partial<Row> = {}) => {
      await put(h, { id: id('4a030001', n), content: `Rosalind member ${n}.`, tier: 'cold', supersededById: n >= 6 && n <= 7 ? digest2 : digest, ...extra });
      const sa = supersededAgo === null ? null : iso(supersededAgo);
      maindb.prepare('UPDATE memories SET supersededAt = ?, updatedAt = ? WHERE id = ?').run(sa, iso(updatedAgo), id('4a030001', n));
    };
    await superseded(1, 40, 40);
    await superseded(2, 10, 40);
    await superseded(3, null, 45);
    await superseded(4, 31, 31);
    await superseded(5, 40, 40, { source: 'MANUAL' });
    await superseded(6, 50, 50);
    await superseded(7, 50, 50);
    await superseded(8, 29, 29);
    // A cold demoted row (no successor) — never retired.
    await put(h, { id: id('4a030001', 9), content: 'Rosalind demoted.', tier: 'cold' });
    maindb.prepare('UPDATE memories SET updatedAt = ? WHERE id = ?').run(iso(90), id('4a030001', 9));
  }

  // Mabel — the whole automatic run.
  {
    const h = spec.holders.mixed;
    const r = rng(2971);
    const T = [0, 0, 0, 1, 0.1, 0, 0, 0];
    await put(h, { id: id('4a040001', 1), content: 'Mabel bakes rye on Thursdays.', importance: 0.9, embedding: unit(T) });
    await put(h, { id: id('4a040001', 2), content: 'Mabel bakes rye bread on Thursdays at five.', importance: 0.4, embedding: at(T, 0.96, 7) });
    for (let i = 1; i <= 105; i++) {
      await put(h, {
        id: id('4a040002', i),
        content: `Mabel note ${i}.`,
        importance: Math.round((0.1 + r() * 0.8) * 100) / 100,
        createdAgo: 3 + (i % 13) * 7,
        embedding: unit([r() - 0.5, r() - 0.5, r() - 0.5, r() - 0.5, r() - 0.5, r() - 0.5, r() - 0.5, r() - 0.5]),
      });
    }
    const digest = id('4a040003', 1);
    await put(h, { id: digest, content: 'Mabel digest.', source: 'CONSOLIDATED', consolidatedFrom: [id('4a040004', 1), id('4a040004', 2)] });
    for (const n of [1, 2]) {
      await put(h, { id: id('4a040004', n), content: `Mabel member ${n}.`, tier: 'cold', supersededById: digest });
      maindb.prepare('UPDATE memories SET supersededAt = ? WHERE id = ?').run(iso(n === 1 ? 60 : 5), id('4a040004', n));
    }
  }

  // Ghost — archived, with one row.
  await put(spec.holders.archived, { id: id('4a050001', 1), content: 'Ghost note.' });

  // Ordway — mixed-case ids (the corpus reads `ORDER BY id ASC`, binary): two
  // demotable rows whose protection ties, so the read order decides.
  {
    const h = spec.holders.order;
    for (const [n, v] of [
      ['4A060001-0000-4000-8000-000000000001', 'upper'],
      ['4a060001-0000-4000-8000-000000000001', 'lower'],
      ['4a060001-0000-4000-8000-00000000000A', 'mixed'],
    ] as Array<[string, string]>) {
      await put(h, { id: n, content: `Ordway ${v}.`, importance: 0.2, createdAgo: 40 });
    }
  }

  // Tilda — a fold ahead of a demotion over tied rows: the fold's absorbed row
  // leaves the corpus with its slot closed up (v4's `Map` delete), so the
  // tied rows keep their read order for the demotion's stable sort (a
  // swap-remove would move the last tied row into the vacated slot).
  {
    const h = spec.holders.tiefold;
    const P = [0, 0, 0, 0, 0, 1, 0.1, 0];
    await put(h, { id: '40070002-0000-4000-8000-000000000001', content: 'Tilda rows a skiff on the millpond.', importance: 0.9, createdAgo: 5, embedding: unit(P) });
    await put(h, { id: '40070002-0000-4000-8000-000000000002', content: 'Tilda rows a skiff on the millpond each evening.', importance: 0.4, createdAgo: 5, embedding: at(P, 0.97, 4) });
    for (const [n, v] of [
      ['4A070001-0000-4000-8000-000000000001', 'upper'],
      ['4a070001-0000-4000-8000-000000000001', 'lower'],
      ['4a070001-0000-4000-8000-00000000000A', 'mixed'],
    ] as Array<[string, string]>) {
      await put(h, { id: n, content: `Tilda ${v}.`, importance: 0.2, createdAgo: 40 });
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(`built maintenance fixture: ${outMain} + ${outMount}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`maintenance fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
