/**
 * P4.d12 unit 2 — bring the COMMITTED test-pepper fixtures up to v4 8bf3cb5f's
 * episodic-spine columns: `memories.{occurredAt,narrativeTime,entities,kind}`,
 * `chats.timelineMode`, and the `idx_memories_occurredAt` index.
 *
 * ## Why this exists (and why it is not a fixture rebuild)
 *
 * Same rationale as `migrate-fixtures-pascal-columns.ts` (P4.6ay unit 10):
 * the committed `crates/quilltap-web/tests/fixtures/*.db` substrates were baked
 * before these columns existed; v5's ported read/write paths now name them, so
 * every such fixture fails with `no such column: occurredAt` (or
 * `timelineMode`) until it carries them. An ALTER touches the schema and
 * nothing else — a rebuild would churn fixture CONTENT through unrelated v4
 * drift and cannot reach the builder-less P4.2-era fixtures at all. And a REAL
 * instance gets these columns from v4's migration (appended at the end), so
 * the migrated shape is the shape production actually has; the generateDDL
 * mid-table shape stays separately pinned by `fresh_schema.json` + the
 * provisioning differential.
 *
 * ## What it runs
 *
 * The DDL half of v4's own migration SQL, verbatim, behind v4's own "only if
 * missing" guards (migrations/scripts/add-episodic-memory-fields.ts:112-133):
 *
 *   ALTER TABLE "memories" ADD COLUMN "occurredAt" TEXT DEFAULT NULL
 *   ALTER TABLE "memories" ADD COLUMN "narrativeTime" TEXT DEFAULT NULL
 *   ALTER TABLE "memories" ADD COLUMN "entities" TEXT DEFAULT '[]'
 *   ALTER TABLE "memories" ADD COLUMN "kind" TEXT DEFAULT 'semantic'
 *   CREATE INDEX IF NOT EXISTS "idx_memories_occurredAt" ON "memories" ("occurredAt" DESC)
 *   ALTER TABLE "chats" ADD COLUMN "timelineMode" TEXT DEFAULT NULL
 *
 * ## Deliberately NOT run: the backfill
 *
 * v4's migration also backfills `occurredAt` (source message `createdAt` when
 * `sourceMessageId` resolves, else the row's own `createdAt`). The fixtures
 * deliberately KEEP `occurredAt` NULL: round 1 of the episodic catch-up is
 * verified through the feature's inert-path guarantee (spec §4 — "degrade to
 * today, never block"), whose precondition is exactly semantic memories with
 * null `occurredAt`. Backfilling would silently shift every event-clock age
 * label in the rebased render families and defeat the round's verification
 * strategy. Rounds 2/3 add fixtures that exercise the populated columns (and
 * the backfilled shape) on purpose.
 *
 * Idempotent: a fixture that already carries a column is left untouched, so a
 * re-run is safe.
 *
 * Run from the v4 checkout (it resolves v4's aliased `better-sqlite3` →
 * better-sqlite3-multiple-ciphers, the sqleet/ChaCha20 binding), Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-fixtures-episodic-columns.ts \
 *     $W/crates/quilltap-web/tests/fixtures
 */

import { createRequire } from 'node:module';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';

// Resolve better-sqlite3 (v4 aliases it to better-sqlite3-multiple-ciphers — the
// sqleet/ChaCha20 binding) from the v4 checkout: run this script with cwd there.
const requireFromV4 = createRequire(process.cwd() + '/');
const Database = requireFromV4('better-sqlite3');

/** The committed web fixtures' synthetic test pepper (quilltap-web/tests/common/mod.rs:17). */
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';

/** v4's episodic-spine ALTERs, verbatim (DDL only — see the header for why no backfill). */
const COLUMN_MIGRATIONS: { table: string; column: string; sql: string }[] = [
  {
    table: 'memories',
    column: 'occurredAt',
    sql: `ALTER TABLE "memories" ADD COLUMN "occurredAt" TEXT DEFAULT NULL`,
  },
  {
    table: 'memories',
    column: 'narrativeTime',
    sql: `ALTER TABLE "memories" ADD COLUMN "narrativeTime" TEXT DEFAULT NULL`,
  },
  {
    table: 'memories',
    column: 'entities',
    sql: `ALTER TABLE "memories" ADD COLUMN "entities" TEXT DEFAULT '[]'`,
  },
  {
    table: 'memories',
    column: 'kind',
    sql: `ALTER TABLE "memories" ADD COLUMN "kind" TEXT DEFAULT 'semantic'`,
  },
  {
    table: 'chats',
    column: 'timelineMode',
    sql: `ALTER TABLE "chats" ADD COLUMN "timelineMode" TEXT DEFAULT NULL`,
  },
];

function main(): void {
  const dir = process.argv[2];
  if (!dir) throw new Error('usage: migrate-fixtures-episodic-columns.ts <fixtures-dir>');

  // The pepper → key in the raw-hex form (KDF skipped), exactly as the rest of
  // the harness opens a test-pepper DB.
  const hex = Buffer.from(TEST_PEPPER, 'base64').toString('hex');

  let touched = 0;
  for (const name of readdirSync(dir).filter((f) => f.endsWith('.db')).sort()) {
    const path = join(dir, name);
    const db = new Database(path);
    db.pragma(`key = "x'${hex}'"`);
    const applied: string[] = [];
    for (const m of COLUMN_MIGRATIONS) {
      const table = db
        .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name=?`)
        .get(m.table);
      if (!table) continue; // this partition does not carry the table
      const cols = (db.prepare(`PRAGMA table_info("${m.table}")`).all() as { name: string }[]).map(
        (c) => c.name,
      );
      if (cols.includes(m.column)) continue; // v4's guard: already migrated
      db.exec(m.sql);
      applied.push(`${m.table}.${m.column}`);
    }
    // The index rides with the memories table (v4 creates it whenever missing).
    const hasMemories = db
      .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name='memories'`)
      .get();
    if (hasMemories) {
      const hasIndex = db
        .prepare(`SELECT name FROM sqlite_master WHERE type='index' AND name='idx_memories_occurredAt'`)
        .get();
      if (!hasIndex) {
        db.exec(`CREATE INDEX IF NOT EXISTS "idx_memories_occurredAt" ON "memories" ("occurredAt" DESC)`);
        applied.push('idx_memories_occurredAt');
      }
    }
    db.close();
    if (applied.length) {
      touched++;
      process.stderr.write(`${name}: +${applied.join(' +')}\n`);
    }
  }
  process.stderr.write(`migrated ${touched} fixture(s)\n`);
}

main();
