/**
 * Fresh-instance schema dumper — the authoritative capture of the DDL a v5
 * `Setup` must reproduce (P4.4 unit 1, the provisioning proof).
 *
 * A fresh v5 instance is provisioned by the CORE at setup time, encrypted from
 * byte zero. Its schema is the **generateDDL surface** — exactly what v4's real
 * repositories create on first access via `ensureCollection`/`getCollection`
 * (the same mechanism every tier-2 fixture uses, proven v4-compatible by every
 * tier-2/tier-3 differential). This script drives v4's REAL repositories to
 * materialize that schema across all three partitions (main / mount-index /
 * llm-logs) and dumps each partition's `sqlite_master`.
 *
 * NOTE on the migration-accumulated schema: a long-lived v4 instance's schema
 * is `SQLITE_TABLES` (hand-written base) + ~170 ALTER migrations, which has the
 * SAME column set but a different column ORDER and DDL text. v4's repos are
 * column-name-addressed (order-agnostic — proven by every tier-2 differential),
 * so the generateDDL surface captured here is a valid, v4-compatible schema; a
 * byte-for-byte match with a migration-accumulated instance would require
 * porting the migration runner (a tracked deferral, unnecessary for
 * correctness).
 *
 * Output: JSON `{ main: string[], mountIndex: string[], llmLogs: string[] }` —
 * per partition, CREATE TABLE statements (ordered by name) then CREATE INDEX
 * statements (ordered by name), so replay is always valid. Written to
 * `QT_SCHEMA_OUT` (a path). The Rust static DDL module and the provisioning
 * differential both consume this.
 *
 * Run from the v4 server checkout under Node 24 (matches v4's `.nvmrc`):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_SCHEMA_OUT=/tmp/qt-fresh-schema.json \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/dump-fresh-schema.ts
 */

import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';

// Raw instance_settings DDL — v4 creates this key/value store by hand
// (version-guard `CREATE TABLE IF NOT EXISTS instance_settings`), not via a Zod
// collection, so no repo drives it. Reproduced verbatim so a fresh v5 instance
// carries it too.
const INSTANCE_SETTINGS_DDL =
  'CREATE TABLE IF NOT EXISTS "instance_settings" (\n' +
  '  "key" TEXT PRIMARY KEY,\n' +
  '  "value" TEXT NOT NULL\n' +
  ')';

interface SchemaRow {
  type: string;
  name: string;
  sql: string;
}

function dumpPartition(db: import('better-sqlite3').Database): string[] {
  const rows = db
    .prepare(
      `SELECT type, name, sql FROM sqlite_master
       WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'`,
    )
    .all() as SchemaRow[];
  const tables = rows
    .filter((r) => r.type === 'table')
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map((r) => r.sql);
  const indexes = rows
    .filter((r) => r.type === 'index')
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map((r) => r.sql);
  return [...tables, ...indexes];
}

async function main(): Promise<void> {
  const out = process.env.QT_SCHEMA_OUT;
  if (!out) {
    throw new Error('QT_SCHEMA_OUT must point at the JSON file to write');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fresh-schema-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'quilltap.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'quilltap-mount-index.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'quilltap-llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, rawQuery, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  // Raw instance_settings store (no repo drives it).
  await rawQuery(INSTANCE_SETTINGS_DDL);

  const repos = getRepositories() as Record<string, unknown>;

  // Drive every repository to materialize its table(s) in the right partition
  // via v4's real first-access DDL. `count()` (a base-repo public method) is the
  // universal trigger; some repos (wardrobe = vault-only) legitimately have no
  // table and throw — skip them, they are not part of a fresh instance's schema.
  const skip = new Set(['wardrobe']); // wardrobe_items is legacy/dropped
  const drove: string[] = [];
  const skipped: string[] = [];
  for (const [key, repo] of Object.entries(repos)) {
    if (skip.has(key)) {
      skipped.push(key);
      continue;
    }
    const r = repo as {
      count?: () => Promise<number>;
      findAll?: () => Promise<unknown[]>;
      findById?: (id: string) => Promise<unknown>;
    };
    try {
      if (typeof r.count === 'function') {
        await r.count();
      } else if (typeof r.findAll === 'function') {
        await r.findAll();
      } else if (typeof r.findById === 'function') {
        await r.findById('00000000-0000-0000-0000-000000000000');
      } else {
        skipped.push(key + '(no-trigger)');
        continue;
      }
      drove.push(key);
    } catch (err) {
      skipped.push(`${key}(${err instanceof Error ? err.message.slice(0, 60) : String(err)})`);
    }
  }

  // Secondary tables owned by multi-table repos (whose `count()` only touches
  // the primary table): trigger each explicitly with a benign read.
  //   chats            → chat_messages
  //   connections      → api_keys (hosted inside ConnectionProfilesRepository)
  //   vectorIndices    → vector_indices + vector_entries (custom repo)
  // docMountBlobs also has no base count/findAll (hand-written repo).
  const secondary: [string, (r: any) => Promise<unknown>][] = [
    ['chats', (r) => r.getMessageCount('00000000-0000-0000-0000-000000000000')],
    ['connections', (r) => r.getApiKeysByUserId('00000000-0000-0000-0000-000000000000')],
    ['vectorIndices', (r) => r.findMetaByCharacterId('00000000-0000-0000-0000-000000000000')],
    ['docMountBlobs', (r) => r.findByFileId('00000000-0000-0000-0000-000000000000')],
  ];
  for (const [key, trigger] of secondary) {
    const repo = repos[key];
    if (!repo) {
      skipped.push(`${key}(secondary:absent)`);
      continue;
    }
    try {
      await trigger(repo);
      drove.push(`${key}#secondary`);
    } catch (err) {
      skipped.push(
        `${key}#secondary(${err instanceof Error ? err.message.slice(0, 60) : String(err)})`,
      );
    }
  }

  // Dump each partition.
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');

  const mainDdl = dumpPartition(getRawDatabase());
  const mountIndexDdl = dumpPartition(getRawMountIndexDatabase());
  const llmLogsDdl = dumpPartition(getRawLLMLogsDatabase());

  writeFileSync(
    out,
    JSON.stringify({ main: mainDdl, mountIndex: mountIndexDdl, llmLogs: llmLogsDdl }, null, 2),
  );

  // Capture the deterministic chat_settings seed row v4's getOrCreateSingleUser
  // produces. The ported ChatSettings nested structs serialize optional keys as
  // explicit `null` (built for the always-present tier-2 corpus), but
  // updateForUser OMITS keys the input doesn't supply — so a byte-exact seed must
  // replay v4's captured JSON columns (raw INSERT), not compose the ported
  // `create`. This captures every column EXCEPT the minted id/createdAt/updatedAt
  // (provisioning mints those); userId is the fixed SINGLE_USER_ID.
  const seedOut = process.env.QT_SEED_OUT;
  if (seedOut) {
    const { getOrCreateSingleUser, SINGLE_USER_ID } = await import('@/lib/auth/single-user');
    await getOrCreateSingleUser();
    const row = getRawDatabase()
      .prepare('SELECT * FROM chat_settings WHERE userId = ?')
      .get(SINGLE_USER_ID) as Record<string, unknown>;
    if (!row) throw new Error('chat_settings row not created by getOrCreateSingleUser');
    // Column order from PRAGMA table_info so the INSERT names match the schema.
    const cols = (getRawDatabase().prepare('PRAGMA table_info(chat_settings)').all() as {
      name: string;
    }[]).map((c) => c.name);
    // Strip the minted/identity columns; provisioning supplies them.
    const minted = new Set(['id', 'userId', 'createdAt', 'updatedAt']);
    const columns = cols.filter((c) => !minted.has(c));
    const values: Record<string, unknown> = {};
    for (const c of columns) values[c] = row[c] ?? null;
    writeFileSync(seedOut, JSON.stringify({ columns, values }, null, 2));
    process.stderr.write(`captured chat_settings seed (${columns.length} cols) → ${seedOut}\n`);
  }

  await closeDatabase();

  process.stderr.write(
    `dumped fresh schema: main=${mainDdl.length} mount-index=${mountIndexDdl.length} ` +
      `llm-logs=${llmLogsDdl.length} statements → ${out}\n` +
      `drove ${drove.length} repos; skipped: ${skipped.join(', ')}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`schema dump failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
