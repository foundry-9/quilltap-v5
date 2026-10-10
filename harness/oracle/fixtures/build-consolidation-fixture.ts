/**
 * The consolidation fixture builder (P4.D271) — the SEED pair every
 * consolidation family copies: `commonplace_digest_vault_bridge_tier2`,
 * `consolidation_service_tier3` and `consolidation_triggers_tier2`.
 *
 * Built through v4's REAL repositories at the `01a83539d` pin, so the memory
 * tier columns are native (no migration helper). Every id is a UUID (v4's
 * `MemorySchema` refuses a non-uuid `consolidatedFrom` entry on create).
 *
 *   - nine holders (database character vaults, the `consolidation-corpus.ts`
 *     template each — one per service run), the subject Laura, a subject
 *     named `Self`, an archived character, a vault whose store type is not
 *     `character`, a FILESYSTEM vault, a vaultless character, a dangling vault
 *     pointer, the 2,001-row Hoarder (one bucket, 64-dim embeddings mutually
 *     below the threshold, one row with the lowest `reinforcedImportance`) and
 *     an empty holder;
 *   - `Others/Laura.md` in the first two holders' vaults (subject canon) and a
 *     stale `Commonplace/Laura.md` in the second's (the mirror overwrites it);
 *   - a plain, a Locked and an Unmoderated chat; the cheap, a configured
 *     consolidation and an uncensored desk connection profile; `chat_settings`
 *     with `cheapLLMSettings` (USER_DEFINED → the cheap profile) and the
 *     Concierge on duty with the desk;
 *   - a vector entry for every embedded memory (the store the run loads);
 *   - a mount-index trigger plant failing any document write whose content
 *     carries the plant's marker (the bridge's per-file catch);
 *   - the `instance_settings` and `background_jobs` tables (the settings and
 *     the trigger family's enqueues).
 *
 * Run from the v4 checkout (or the round's pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { DAY, DIMS, TEMPLATE, digestVector, memoryId, rowOf, rowVector } from './consolidation-corpus';

interface Named {
  id: string;
  name: string;
}
interface Holder extends Named {
  index: number;
  identity: string | null;
  description: string | null;
}
interface Profile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isCheap: boolean;
  isDangerousCompatible: boolean;
}
export interface ConsolidationSpec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  seedTimestamp: string;
  now: string;
  subject: Named & { identity: string };
  selfie: Named;
  orphanSubjectId: string;
  archived: Named;
  storeTypeMismatch: Named;
  filesystemVault: Named;
  vaultless: Named;
  danglingVault: Named & { mountPointId: string };
  hoarder: Named & { rows: number; dims: number };
  quiet: Named;
  holders: Holder[];
  chats: { plain: string; locked: string; unmoderated: string };
  profiles: { cheap: Profile; consolidation: Profile; desk: Profile };
  chatSettingsId: string;
  vaultDocs: { othersLaura: string; staleCommonplace: string };
  mountWritePlant: { name: string; marker: string; message: string };
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
  const spec = JSON.parse(readFileSync(join(here, 'consolidation-tier3.json'), 'utf8')) as ConsolidationSpec;

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-consolidation-build-'));
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
  const repos = getRepositories();
  const TS = spec.seedTimestamp;
  const NOW = Date.parse(spec.now);

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

  const maindb = (await import('@/lib/database/backends/sqlite/client')).getRawDatabase?.();
  if (!maindb) throw new Error('main DB handle unavailable');
  const { VectorIndexMetaSchema, VectorEntryRowSchema } = await import('@/lib/schemas/vector-indices.types');
  for (const [name, schema] of [
    ['vector_indices', VectorIndexMetaSchema],
    ['vector_entries', VectorEntryRowSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) maindb.exec(sql);
  }
  // The instance_settings table the consolidation settings live in (v4's
  // `create-instance-settings-table` migration's DDL; a builder runs no
  // migrations).
  maindb.exec('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  // The background_jobs table the trigger family's enqueues land in (v4 creates
  // it lazily; both sides start from the same empty table).
  const { BackgroundJobSchema } = await import('@/lib/schemas/types');
  for (const sql of generateDDL('background_jobs', BackgroundJobSchema as never)) maindb.exec(sql);

  // ── characters ──────────────────────────────────────────────────────────
  const createCharacter = async (c: Named, extra: Record<string, unknown> = {}) =>
    repos.characters.create(
      { name: c.name, userId: spec.userId, controlledBy: 'llm', ...extra } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
  for (const h of spec.holders) {
    await createCharacter(h, { identity: h.identity, description: h.description });
  }
  await createCharacter(spec.subject, { identity: spec.subject.identity });
  await createCharacter(spec.selfie);
  await createCharacter(spec.archived);
  await createCharacter(spec.storeTypeMismatch);
  await createCharacter(spec.filesystemVault);
  await createCharacter(spec.vaultless);
  await createCharacter(spec.danglingVault);
  await createCharacter(spec.hoarder);
  await createCharacter(spec.quiet);

  const vaultOf = async (id: string): Promise<string> => {
    const raw = await repos.characters.findByIdRaw(id);
    const mp = raw?.characterDocumentMountPointId as string | null | undefined;
    if (!mp) throw new Error(`no vault for ${id}`);
    return mp;
  };

  maindb.prepare('UPDATE characters SET archivedAt = ? WHERE id = ?').run(TS, spec.archived.id);
  midb
    .prepare("UPDATE doc_mount_points SET storeType = 'documents' WHERE id = ?")
    .run(await vaultOf(spec.storeTypeMismatch.id));
  midb
    .prepare("UPDATE doc_mount_points SET mountType = 'filesystem', basePath = '/nonexistent/qt-consolidation' WHERE id = ?")
    .run(await vaultOf(spec.filesystemVault.id));
  maindb.prepare('UPDATE characters SET characterDocumentMountPointId = NULL WHERE id = ?').run(spec.vaultless.id);
  maindb
    .prepare('UPDATE characters SET characterDocumentMountPointId = ? WHERE id = ?')
    .run(spec.danglingVault.mountPointId, spec.danglingVault.id);

  // ── vault documents ─────────────────────────────────────────────────────
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  for (const h of spec.holders.slice(0, 2)) {
    const mp = await vaultOf(h.id);
    await ensureFolderPath(mp, 'Others');
    await writeDatabaseDocument(mp, `Others/${spec.subject.name}.md`, spec.vaultDocs.othersLaura);
  }
  {
    const mp = await vaultOf(spec.holders[1].id);
    await ensureFolderPath(mp, 'Commonplace');
    await writeDatabaseDocument(mp, `Commonplace/${spec.subject.name}.md`, spec.vaultDocs.staleCommonplace);
  }

  // ── chats ───────────────────────────────────────────────────────────────
  for (const [label, id] of Object.entries(spec.chats)) {
    await repos.chats.create(
      { userId: spec.userId, title: `Consolidation ${label}`, participants: [] } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  maindb.prepare("UPDATE chats SET conciergeMode = 'locked' WHERE id = ?").run(spec.chats.locked);
  maindb.prepare("UPDATE chats SET conciergeMode = 'unmoderated' WHERE id = ?").run(spec.chats.unmoderated);

  // ── connection profiles + chat settings ─────────────────────────────────
  for (const p of Object.values(spec.profiles)) {
    await repos.connections.create(
      {
        userId: spec.userId,
        name: p.name,
        provider: p.provider,
        modelName: p.modelName,
        isCheap: p.isCheap,
        isDangerousCompatible: p.isDangerousCompatible,
      } as never,
      { id: p.id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: {
        strategy: 'USER_DEFINED',
        userDefinedProfileId: spec.profiles.cheap.id,
        fallbackToLocal: false,
      },
      conciergeSettings: { enabled: true, uncensoredTextProfileId: spec.profiles.desk.id },
    } as never,
    { id: spec.chatSettingsId, createdAt: TS, updatedAt: TS } as never,
  );

  // ── memories ────────────────────────────────────────────────────────────
  const { VectorIndicesRepository } = await import('@/lib/database/repositories/vector-indices.repository');
  const vectors = new VectorIndicesRepository();
  const iso = (agoDays: number) => new Date(NOW - agoDays * DAY).toISOString();
  const resolveLink = (holderIndex: number, link: string) =>
    /^[0-9a-f]{8}-/.test(link) ? link : memoryId(holderIndex, rowOf(link));

  for (const h of spec.holders) {
    await vectors.saveMeta(h.id, DIMS);
    for (const t of TEMPLATE) {
      const id = memoryId(h.index, t.row);
      const about =
        t.bucket === 'self' ? h.id : t.bucket === 'none' ? null : t.bucket === 'orphan' ? spec.orphanSubjectId : spec.subject.id;
      const embedding = t.noEmbedding
        ? null
        : t.source === 'CONSOLIDATED'
          ? digestVector(t.topic as string)
          : rowVector(t.topic as string, t.row);
      const created = iso(t.createdAgoDays);
      await repos.memories.create(
        {
          characterId: h.id,
          aboutCharacterId: about,
          chatId: t.chat ? spec.chats[t.chat] : null,
          projectId: null,
          content: t.content,
          summary: t.content.replace(/^\[[A-Z]\d\] /, '').slice(0, 40).toLowerCase(),
          keywords: t.keywords,
          tags: [],
          importance: t.importance,
          embedding,
          source: t.source,
          witnessedContext: t.witnessedContext ?? null,
          occurredAt: t.occurredAt ?? null,
          narrativeTime: t.narrativeTime ?? null,
          entities: t.entities ?? [],
          kind: t.kind,
          sourceMessageId: null,
          lastAccessedAt: null,
          reinforcementCount: t.reinforcementCount,
          lastReinforcedAt: null,
          relatedMemoryIds: (t.related ?? []).map((l) => resolveLink(h.index, l)),
          reinforcedImportance: t.reinforcedImportance,
          tier: t.tier ?? 'hot',
          supersededById: null,
          consolidatedFrom: t.consolidatedFrom ?? [],
          consolidatedAt: t.consideredAgoDays !== undefined ? iso(t.consideredAgoDays) : null,
        } as never,
        { id, createdAt: created, updatedAt: created },
      );
      if (embedding) {
        await vectors.addEntry({ id, characterId: h.id, embedding });
      }
    }
  }

  // The Hoarder: one bucket of 2,001 mature, never-considered AUTO rows whose
  // 64-dim gaussian embeddings sit mutually far below the threshold; the row
  // with the lowest reinforcedImportance is the one the 2,000-row cap drops.
  {
    const r = rng(20261009);
    const gauss = () => {
      const u = Math.max(r(), 1e-12);
      const v = r();
      return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
    };
    await vectors.saveMeta(spec.hoarder.id, spec.hoarder.dims);
    for (let i = 0; i < spec.hoarder.rows; i++) {
      const id = `0dfe0000-0000-4000-8000-${(i + 1).toString(16).padStart(12, '0')}`;
      const raw = Array.from({ length: spec.hoarder.dims }, gauss);
      const n = Math.sqrt(raw.reduce((s, x) => s + x * x, 0));
      const embedding = raw.map((x) => Math.fround(x / n));
      const ri = i === 1234 ? 0.01 : Math.round((0.3 + r() * 0.6) * 1000) / 1000;
      const created = iso(30);
      await repos.memories.create(
        {
          characterId: spec.hoarder.id,
          aboutCharacterId: spec.subject.id,
          chatId: null,
          projectId: null,
          content: `Hoard note ${i + 1}.`,
          summary: `hoard note ${i + 1}`,
          keywords: [],
          tags: [],
          importance: ri,
          embedding,
          source: 'AUTO',
          witnessedContext: null,
          occurredAt: null,
          narrativeTime: null,
          entities: [],
          kind: 'semantic',
          sourceMessageId: null,
          lastAccessedAt: null,
          reinforcementCount: 1,
          lastReinforcedAt: null,
          relatedMemoryIds: [],
          reinforcedImportance: ri,
          tier: 'hot',
          supersededById: null,
          consolidatedFrom: [],
          consolidatedAt: null,
        } as never,
        { id, createdAt: created, updatedAt: created },
      );
      await vectors.addEntry({ id, characterId: spec.hoarder.id, embedding });
    }
  }

  // ── the bridge's per-file failure plant (created after every seed write) ─
  const sqlString = (v: string) => `'${v.replace(/'/g, "''")}'`;
  for (const event of ['INSERT', 'UPDATE']) {
    midb.exec(
      `CREATE TRIGGER ${spec.mountWritePlant.name}_${event.toLowerCase()} BEFORE ${event} ON doc_mount_documents ` +
        `WHEN NEW.content LIKE ${sqlString(`%${spec.mountWritePlant.marker}%`)} ` +
        `BEGIN SELECT RAISE(ABORT, ${sqlString(spec.mountWritePlant.message)}); END;`,
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built consolidation fixture: ${outMain} + ${outMount} (${spec.holders.length} holders × ${TEMPLATE.length} rows, ` +
      `hoarder ${spec.hoarder.rows} rows)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`consolidation fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
