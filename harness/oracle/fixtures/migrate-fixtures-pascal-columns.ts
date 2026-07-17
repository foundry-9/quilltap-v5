/**
 * P4.6ay unit 10 — bring the COMMITTED test-pepper fixtures up to v4 4.8.0's
 * `chat_messages.pascalMeta` + `chat_settings.customTools` columns.
 *
 * ## Why this exists (and why it is not a fixture rebuild)
 *
 * The committed `crates/quilltap-web/tests/fixtures/*.db` substrates were baked
 * by v4 BEFORE those two columns existed. v5's ported write/read paths now name
 * them, so every such fixture fails with `no such column: pascalMeta` until it
 * carries them.
 *
 * The obvious fix — re-run each `build-*-fixture.ts` against v4 `a33ac8b8` —
 * was rejected for three reasons:
 *
 *   1. It re-bakes the whole substrate through EIGHT commits of unrelated v4
 *      drift, so a two-column adoption would churn fixture CONTENT and cascade
 *      into every oracle those fixtures feed. This script changes the schema and
 *      nothing else.
 *   2. Three committed fixtures (`chat-send-{main,mount}.db`, `salon-llm-logs.db`)
 *      have NO builder at all — they are P4.2-era artifacts — so a rebuild could
 *      not reach them anyway.
 *   3. **Fidelity.** A rebuild yields the `generateDDL` shape, where the column
 *      sits mid-table in schema position. A REAL instance gets these columns from
 *      v4's migration — appended at the end. Migrating the fixtures reproduces the
 *      shape production actually has, which is the shape v5 must read. (The
 *      generateDDL shape is separately pinned by `fresh_schema.json` + the
 *      provisioning differential, so both shapes stay covered.)
 *
 * ## What it runs
 *
 * v4's own migration SQL, verbatim, behind v4's own "only if the column is
 * missing" guard:
 *
 *   migrations/scripts/add-pascal-message-meta-v1.ts:52
 *     ALTER TABLE "chat_messages" ADD COLUMN "pascalMeta" TEXT DEFAULT NULL
 *   migrations/scripts/add-custom-tools-field.ts:71
 *     ALTER TABLE "chat_settings" ADD COLUMN "customTools" INTEGER DEFAULT 1
 *
 * SQLite's `ADD COLUMN ... DEFAULT` back-fills existing rows with the default, so
 * the post-state is byte-identical to what a v4 4.8.0 upgrade produces: every
 * pre-existing message reads `pascalMeta` NULL, every settings row reads
 * `customTools` 1.
 *
 * Idempotent: a fixture that already carries a column is left untouched, so a
 * re-run is safe.
 *
 * Run from the v4 checkout (it resolves v4's aliased `better-sqlite3` →
 * better-sqlite3-multiple-ciphers, the sqleet/ChaCha20 binding), Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-fixtures-pascal-columns.ts \
 *     $W/crates/quilltap-web/tests/fixtures
 */

import { createRequire } from 'node:module';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';

// Resolve better-sqlite3 (v4 aliases it to better-sqlite3-multiple-ciphers — the
// sqleet/ChaCha20 binding) from the v4 checkout: run this script with cwd there.
// A bare `import` would resolve from THIS file's directory, and the v5 worktree
// has no node_modules — the `enclave-cron.ts` croner idiom, same reason.
const requireFromV4 = createRequire(process.cwd() + '/');
const Database = requireFromV4('better-sqlite3');

/** The committed web fixtures' synthetic test pepper (quilltap-web/tests/common/mod.rs:17). */
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';

/** v4's two 4.8.0 ALTERs, verbatim. */
const MIGRATIONS: { table: string; column: string; sql: string }[] = [
  {
    table: 'chat_messages',
    column: 'pascalMeta',
    sql: `ALTER TABLE "chat_messages" ADD COLUMN "pascalMeta" TEXT DEFAULT NULL`,
  },
  {
    table: 'chat_settings',
    column: 'customTools',
    sql: `ALTER TABLE "chat_settings" ADD COLUMN "customTools" INTEGER DEFAULT 1`,
  },
];

function main(): void {
  const dir = process.argv[2];
  if (!dir) throw new Error('usage: migrate-fixtures-pascal-columns.ts <fixtures-dir>');

  // The pepper → key in the raw-hex form (KDF skipped), exactly as the rest of
  // the harness opens a test-pepper DB.
  const hex = Buffer.from(TEST_PEPPER, 'base64').toString('hex');

  let touched = 0;
  for (const name of readdirSync(dir).filter((f) => f.endsWith('.db')).sort()) {
    const path = join(dir, name);
    const db = new Database(path);
    db.pragma(`key = "x'${hex}'"`);
    const applied: string[] = [];
    for (const m of MIGRATIONS) {
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
    db.close();
    if (applied.length) {
      touched++;
      process.stderr.write(`${name}: +${applied.join(' +')}\n`);
    }
  }
  process.stderr.write(`migrated ${touched} fixture(s)\n`);
}

main();
