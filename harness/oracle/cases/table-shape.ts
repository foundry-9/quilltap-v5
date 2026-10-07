/**
 * Tier-1 oracle case — v4's boot-time structural table check (bug 176,
 * `e5c6bd0c0`): the REAL `findTableShapeProblem` (`lib/database/table-shape.ts`),
 * the REAL `AbstractDedicatedDbRepository.verifyStructure`, and the REAL
 * repository container (`createRepositories`) the PHASE 3.1 pass iterates.
 *
 * Emits, one NDJSON row each:
 *   - `census` — every structure-verifiable repository in the container's
 *     insertion order (the pass's `seen`-set + `verifyStructure` filter,
 *     `lib/startup/verify-structural-tables.ts:53-58`): repository key,
 *     collection, `dbTarget`, and `extractSchemaMetadata(...).fields` — the
 *     column list `findTableShapeProblem` checks, in SCHEMA order. The Rust side
 *     diffs it against `quilltap_core::db::table_shape::STRUCTURAL_TABLES`
 *     field-for-field (that table is generated from this row set);
 *   - `shape` — the spec's five shapes over EVERY real schema (sound + extra,
 *     one renamed, two renamed over a REVERSED-column table, RENAME-then-VIEW,
 *     absent), each with the SQL it ran, so v5 replays the identical tables;
 *   - `links` — v4's own `verify-structural-tables.test.ts` `LinksRepository`
 *     shapes through BOTH dedicated targets: the REAL guard functions thrown in
 *     their degraded and uninitialized states, an ensure that fails (a TABLE
 *     squatting the index name), a fresh database, a pre-made renamed column;
 *   - `substrate` — the mount-index + LLM-logs DDL v4's own ensures create on an
 *     empty database (the dump v5 replays every plant on);
 *   - `plant` — each spec plant applied to that substrate, then a FRESH container
 *     (every `tableEnsured` memo false, as at boot) verified in order: the
 *     problems v4's pass would record, and (P4.150) every `Migrated …` INFO
 *     the ensures logged on the way, in order (a `Logger.prototype` spy);
 *   - `degraded` (P4.159, dogfood #150) — each spec set of dedicated partitions
 *     DEGRADED the way v4's client leaves it (`__quilltap<X>Degraded = true`,
 *     no connection; the others fresh), then a FRESH container verified in
 *     order: the problems v4's pass would record, each through the REAL
 *     guard (`<label> database unavailable: <Mount index|LLM logs> database
 *     is in degraded mode`). The pass's own ERROR lines are compared by the
 *     `degraded-sibling-open.test.ts` jest case, which drives the REAL
 *     `verifyStructuralTables` (it needs jest's manager / factory mocks).
 *
 * The `help_doc_chunks in main database:` template is NOT driven here: the
 * help-chunks repository's `getCollection` connects the configured main backend,
 * which this case must never open. Transcribed from `help-doc-chunks.repository.ts:54`
 * and unit-pinned on the Rust side (recorded).
 *
 * Run (Node 24, from the v4 checkout — or the pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/table-shape.ts \
 *     > /tmp/oracle-table-shape.ndjson
 */

import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { findTableShapeProblem, type ShapeQuery } from '@/lib/database/table-shape';
import { extractSchemaMetadata, generateDDL } from '@/lib/database/schema-translator';
import { AbstractDedicatedDbRepository } from '@/lib/database/repositories/dedicated-db.repository';
import { createRepositories } from '@/lib/database/repositories';
import { requireMountIndexDb } from '@/lib/database/backends/sqlite/mount-index-guard';
import { requireLLMLogsDb } from '@/lib/database/backends/sqlite/llm-logs-guard';
import { logger } from '@/lib/logger';

// The root package aliases better-sqlite3-multiple-ciphers as better-sqlite3;
// require the real binding by absolute path (an in-memory DB needs no cipher).
// `zod` likewise resolves from the v4 tree, never this file's directory (a bare
// import from a case outside the v4 tree dies `ERR_MODULE_NOT_FOUND`) — and it
// must be the SAME module instance v4's `schema-translator` loads, or its
// `instanceof ZodObject` refuses the test schema.
const require = createRequire(import.meta.url);
const Database = require(join(process.cwd(), 'node_modules', 'better-sqlite3'));
const requireFromV4 = createRequire(join(process.cwd(), 'package.json'));
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const { z } = requireFromV4('zod') as { z: any };

const HERE = dirname(fileURLToPath(import.meta.url));
const SPEC = JSON.parse(
  readFileSync(join(HERE, '..', 'fixtures', 'table-shape-spec.json'), 'utf8'),
) as Spec;

interface Plant {
  name: string;
  mount: string[];
  llm: string[];
  divergence?: boolean;
}
interface Spec {
  shapeCases: string[];
  linksTargets: Array<'mountIndex' | 'llmLogs'>;
  plants: Plant[];
  degradedTargets: Array<Array<'mountIndex' | 'llmLogs'>>;
}

type Db = { exec(sql: string): void; prepare(sql: string): { all(...p: unknown[]): unknown[] }; close(): void };

// v4's real logger writes to stdout through its console transport; this case's
// stdout IS the NDJSON, so every non-row line is routed to stderr.
const out = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');
const realLog = console.log;
console.log = (...args: unknown[]) => console.error(...args);
console.info = (...args: unknown[]) => console.error(...args);
console.debug = (...args: unknown[]) => console.error(...args);
void realLog;

const g = globalThis as unknown as Record<string, unknown>;

// P4.150: v4's `Logger.prototype.info` spy — the `Migrated doc_mount_points:
// added <col> column` lines the `doc_mount_points` ensure (`onTableEnsured`)
// logs as it heals a pre-ALTER table. Every logger (the root and each child)
// shares the prototype; the real method still runs (to stderr, above).
const infos: Array<{ message: string; context: unknown }> = [];
{
  const proto = Object.getPrototypeOf(logger) as { info: (m: string, c?: unknown) => void };
  const realInfo = proto.info;
  proto.info = function (this: unknown, message: string, context?: unknown) {
    if (message.startsWith('Migrated ')) infos.push({ message, context: context ?? null });
    return realInfo.call(this, message, context);
  };
}

function queryOf(db: Db): ShapeQuery {
  return <R>(sql: string, params: unknown[]) => db.prepare(sql).all(...params) as R[];
}

const q = (name: string) => `"${name}"`;

/** The five shapes over one schema's field list — the SQL each case runs. */
function shapeSql(shape: string, name: string, fields: string[]): string[] {
  const cols = (fs: string[]) => fs.map(f => `${q(f)} TEXT`).join(', ');
  const last = fields.length - 1;
  switch (shape) {
    case 'sound-extra':
      return [`CREATE TABLE ${q(name)} (${cols([...fields, 'qtExtraColumn'])})`];
    case 'one-renamed':
      return [`CREATE TABLE ${q(name)} (${cols(fields.map((f, i) => (i === 1 ? `${f}_x` : f)))})`];
    case 'two-renamed-reversed':
      return [
        `CREATE TABLE ${q(name)} (${cols(
          fields.map((f, i) => (i === 1 || i === last ? `${f}_x` : f)).reverse(),
        )})`,
      ];
    case 'rename-then-view':
      return [
        `CREATE TABLE ${q(`${name}_x`)} (${cols(fields)})`,
        `CREATE INDEX ${q(`idx_${name}_createdAt`)} ON ${q(`${name}_x`)} ("createdAt" DESC)`,
        `CREATE VIEW ${q(name)} AS SELECT * FROM ${q(`${name}_x`)}`,
      ];
    case 'absent':
      return [];
    default:
      throw new Error(`unknown shape ${shape}`);
  }
}

/** Every structure-verifiable repository, as the pass walks the container. */
function verifiable(repos: Record<string, unknown>): Array<[string, any]> {
  const seen = new Set<unknown>();
  const found: Array<[string, any]> = [];
  for (const [key, repo] of Object.entries(repos)) {
    if (seen.has(repo) || typeof (repo as any)?.verifyStructure !== 'function') continue;
    seen.add(repo);
    found.push([key, repo]);
  }
  return found;
}

function setDedicated(mount: Db | null, llm: Db | null): void {
  g.__quilltapMountIndexDatabase = mount ?? undefined;
  g.__quilltapLLMLogsDatabase = llm ?? undefined;
  g.__quilltapMountIndexDegraded = false;
  g.__quilltapLLMLogsDegraded = false;
}

function dumpDdl(db: Db): string[] {
  return (db.prepare(
    "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY rowid",
  ).all() as Array<{ sql: string }>).map(r => r.sql);
}

/** v4's own test schema + repository (`verify-structural-tables.test.ts:53-75`). */
const LinkSchema = z.object({
  id: z.string(),
  relativePath: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
});
type Link = { id: string; relativePath: string; createdAt: string; updatedAt: string };

class LinksRepository extends AbstractDedicatedDbRepository<Link> {
  constructor(dbTarget: 'mountIndex' | 'llmLogs', acquireDb: () => unknown) {
    super('links', LinkSchema, { dbTarget, acquireDb: acquireDb as () => never });
  }
  async findById(): Promise<Link | null> { return null; }
  async findAll(): Promise<Link[]> { return []; }
  async create(): Promise<Link> { throw new Error('unused'); }
  async update(): Promise<Link | null> { return null; }
  async delete(): Promise<boolean> { return false; }
}

function guardMessage(guard: () => unknown): string {
  try {
    guard();
  } catch (e) {
    return (e as Error).message;
  }
  throw new Error('the guard did not throw');
}

async function main(): Promise<void> {
  // ---- census --------------------------------------------------------------
  setDedicated(null, null);
  const census = verifiable(createRepositories() as unknown as Record<string, unknown>);
  const tables = census.map(([repository, repo], index) => ({
    kind: 'census',
    index,
    repository,
    collection: repo.collectionName as string,
    dbTarget: repo.dbTarget as string,
    fields: extractSchemaMetadata(repo.collectionName, repo.schema).fields.map(f => f.name),
    schema: repo.schema,
  }));
  for (const { schema: _schema, ...row } of tables) out(row);
  out({ kind: 'census-count', checked: tables.length });

  // ---- shapes over every real schema ---------------------------------------
  for (const t of tables) {
    for (const shape of SPEC.shapeCases) {
      const db: Db = new Database(':memory:');
      const sql = shapeSql(shape, t.collection, t.fields);
      for (const s of sql) db.exec(s);
      const problem = await findTableShapeProblem(queryOf(db), t.collection, t.schema);
      db.close();
      out({ kind: 'shape', repository: t.repository, collection: t.collection, case: shape, sql, out: problem });
    }
  }

  // ---- v4's LinksRepository shapes, both dedicated targets -----------------
  const linkFields = extractSchemaMetadata('links', LinkSchema).fields.map(f => f.name);
  const linkDdl = generateDDL('links', LinkSchema);
  for (const target of SPEC.linksTargets) {
    const guard = target === 'mountIndex' ? requireMountIndexDb : requireLLMLogsDb;
    const degradedFlag = target === 'mountIndex' ? '__quilltapMountIndexDegraded' : '__quilltapLLMLogsDegraded';
    const base = { kind: 'links', target, fields: linkFields, ddl: linkDdl };

    setDedicated(null, null);
    const uninit = guardMessage(guard);
    out({ ...base, case: 'uninitialized', guardMessage: uninit, sql: [], out: await new LinksRepository(target, guard).verifyStructure() });

    g[degradedFlag] = true;
    const degraded = guardMessage(guard);
    out({ ...base, case: 'degraded', guardMessage: degraded, sql: [], out: await new LinksRepository(target, guard).verifyStructure() });
    setDedicated(null, null);

    const plants: Array<[string, string[]]> = [
      ['fresh', []],
      ['index-squatted', ['CREATE TABLE "idx_links_createdAt" (x TEXT)']],
      ['premade-renamed', ['CREATE TABLE links (id TEXT, relativePath_x TEXT, createdAt TEXT, updatedAt TEXT)']],
    ];
    for (const [name, sql] of plants) {
      const db: Db = new Database(':memory:');
      for (const s of sql) db.exec(s);
      const problem = await new LinksRepository(target, () => db).verifyStructure();
      db.close();
      out({ ...base, case: name, sql, out: problem });
    }
  }

  // ---- the substrate v4's own ensures create --------------------------------
  const dedicated = tables.filter(t => t.dbTarget !== 'main').map(t => t.repository);
  const substrateMount: Db = new Database(':memory:');
  const substrateLlm: Db = new Database(':memory:');
  setDedicated(substrateMount, substrateLlm);
  {
    const repos = createRepositories() as unknown as Record<string, any>;
    for (const key of dedicated) {
      const problem = await repos[key].verifyStructure();
      if (problem !== null) throw new Error(`substrate ${key}: ${problem}`);
    }
  }
  const mountDdl = dumpDdl(substrateMount);
  const llmDdl = dumpDdl(substrateLlm);
  substrateMount.close();
  substrateLlm.close();
  setDedicated(null, null);
  out({ kind: 'substrate', mount: mountDdl, llm: llmDdl });

  // ---- plants: a fresh container over the planted substrate ----------------
  for (const plant of SPEC.plants) {
    const mount: Db = new Database(':memory:');
    const llm: Db = new Database(':memory:');
    for (const s of mountDdl) mount.exec(s);
    for (const s of llmDdl) llm.exec(s);
    for (const s of plant.mount) mount.exec(s);
    for (const s of plant.llm) llm.exec(s);
    setDedicated(mount, llm);
    infos.length = 0;
    const repos = createRepositories() as unknown as Record<string, any>;
    const problems: Array<{ repository: string; problem: string }> = [];
    for (const key of dedicated) {
      const problem = await repos[key].verifyStructure();
      if (problem) problems.push({ repository: key, problem });
    }
    mount.close();
    llm.close();
    setDedicated(null, null);
    out({
      kind: 'plant',
      name: plant.name,
      divergence: plant.divergence ?? false,
      problems,
      infos: [...infos],
    });
  }

  // ---- degraded partitions (P4.159) -----------------------------------------
  for (const degradedSet of SPEC.degradedTargets) {
    const mount: Db | null = degradedSet.includes('mountIndex') ? null : new Database(':memory:');
    const llm: Db | null = degradedSet.includes('llmLogs') ? null : new Database(':memory:');
    setDedicated(mount, llm);
    if (degradedSet.includes('mountIndex')) g.__quilltapMountIndexDegraded = true;
    if (degradedSet.includes('llmLogs')) g.__quilltapLLMLogsDegraded = true;
    const repos = createRepositories() as unknown as Record<string, any>;
    const problems: Array<{ repository: string; problem: string }> = [];
    for (const key of dedicated) {
      const problem = await repos[key].verifyStructure();
      if (problem) problems.push({ repository: key, problem });
    }
    mount?.close();
    llm?.close();
    setDedicated(null, null);
    out({ kind: 'degraded', degraded: degradedSet, problems });
  }
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
