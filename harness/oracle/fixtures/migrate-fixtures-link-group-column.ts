/**
 * P4.D41 — bring the COMMITTED test-pepper fixtures up to v4 `40319484`'s
 * `doc_mount_file_links.linkGroupId` column (deliberate hard-link groups).
 *
 * ## Why this exists (and why it is not a fixture rebuild)
 *
 * The committed `crates/quilltap-web/tests/fixtures/*-mount.db` substrates were
 * baked before the column existed. v5's ported joined reads now name it, so
 * every such fixture fails with `no such column: linkGroupId` until it carries
 * one. This is the `migrate-fixtures-pascal-columns.ts` situation exactly, and
 * the same three reasons apply — see that file's header for the long form:
 *
 *   1. A rebuild re-bakes each substrate through unrelated v4 drift, churning
 *      fixture CONTENT and cascading into every oracle those fixtures feed. This
 *      script changes the schema and nothing else.
 *   2. Some committed fixtures have no builder at all, so a rebuild could not
 *      reach them.
 *   3. **Fidelity.** A rebuild yields the `generateDDL` shape, where the column
 *      sits at position 3 (right after `fileId`). A REAL instance gets it from
 *      v4's migration / `ensureLinkGroupColumn` — APPENDED. Migrating reproduces
 *      the shape production actually has. The generateDDL shape stays separately
 *      pinned by `fresh_schema.json` + the provisioning differential, so both
 *      are covered.
 *
 * ## What it runs
 *
 * v4's own SQL, verbatim, behind v4's own "only if the column is missing" guard
 * (`migrations/scripts/add-doc-mount-link-groups.ts:130` and the identical pair
 * in `mount-index-case-repair.ts:337`):
 *
 *   ALTER TABLE "doc_mount_file_links" ADD COLUMN "linkGroupId" TEXT DEFAULT NULL
 *   CREATE INDEX IF NOT EXISTS "idx_doc_mount_file_links_linkGroupId"
 *     ON "doc_mount_file_links" ("linkGroupId") WHERE "linkGroupId" IS NOT NULL
 *
 * The column is nullable with no default, so every pre-existing link reads NULL
 * — v4's deliberate "no backfill" decision: a pre-existing shared `fileId`
 * cannot be told apart from content-addressed dedup, so existing links start
 * ungrouped and must be re-made with `docs link`.
 *
 * Idempotent: a fixture that already carries the column is left untouched.
 *
 * Run from the v4 checkout (it resolves v4's aliased `better-sqlite3` →
 * better-sqlite3-multiple-ciphers, the sqleet/ChaCha20 binding), Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $W/harness/oracle/fixtures/migrate-fixtures-link-group-column.ts \
 *     $W/crates/quilltap-web/tests/fixtures
 */

import { createRequire } from 'node:module';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';

// Resolve better-sqlite3 from the v4 checkout: run this script with cwd there.
const requireFromV4 = createRequire(process.cwd() + '/');
const Database = requireFromV4('better-sqlite3');

/**
 * The synthetic test peppers the committed fixtures are keyed with. The web
 * fixtures started on one shared pepper (quilltap-web/tests/common/mod.rs:17),
 * but later families minted their own, so a fixture is opened by TRYING each in
 * turn — a wrong key surfaces as `SQLITE_NOTADB` on the first read, not on open.
 *
 * Refresh from the corpus by grepping `testPepperBase64` out of
 * `harness/oracle/fixtures/*.json` and de-duplicating.
 *
 * SAFETY: every one of these is a throwaway 32-byte key that decrypts nothing
 * real. The production pepper never appears here.
 */
const TEST_PEPPERS = [
  'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=',
  '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=',
  '3q2+796tvu3erb7t3q2+796tvu3erb7t3q2+796tvu0=',
  'Y2hhdC1jYXN0LXRlc3QtcGVwcGVyLTMyLWJ5dGVzISE=',
  'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=',
  'ZpjI5jcj5CYsyBA6zPH9WgY4Q0nqLT0lVbF3mFjO5vE=',
  'b3JhY2xlLXRlc3QtcGVwcGVyLTMyLWJ5dGVzLWxvbmch',
  'bWFpbGNhcmluYXRvb2xzdGVzdHBlcHBlcjAxMjM0NTY3ODlhYg==',
  'c3RhdGVzcWx0b29sc3Rlc3RwZXBwZXIwMTIzNDU2Nzg5YWJjZGU=',
  'cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==',
];

/**
 * Open `path` with whichever test pepper unlocks it, or `null` when none does
 * (a fixture keyed with a pepper this list has not learned yet — reported, never
 * silently skipped).
 */
function openWithAnyTestPepper(path: string): InstanceType<typeof Database> | null {
  for (const pepper of TEST_PEPPERS) {
    const hex = Buffer.from(pepper, 'base64').toString('hex');
    const db = new Database(path);
    try {
      db.pragma(`key = "x'${hex}'"`);
      db.prepare(`SELECT count(*) FROM sqlite_master`).get();
      return db;
    } catch {
      try {
        db.close();
      } catch {
        /* ignore */
      }
    }
  }
  return null;
}

const TABLE = 'doc_mount_file_links';
const COLUMN = 'linkGroupId';
const ADD_COLUMN = `ALTER TABLE "${TABLE}" ADD COLUMN "${COLUMN}" TEXT DEFAULT NULL`;
const ADD_INDEX =
  `CREATE INDEX IF NOT EXISTS "idx_doc_mount_file_links_linkGroupId" ` +
  `ON "${TABLE}" ("${COLUMN}") WHERE "${COLUMN}" IS NOT NULL`;

function main(): void {
  const dir = process.argv[2];
  if (!dir) throw new Error('usage: migrate-fixtures-link-group-column.ts <fixtures-dir>');

  let touched = 0;
  const unopened: string[] = [];
  for (const name of readdirSync(dir).filter((f) => f.endsWith('.db')).sort()) {
    const path = join(dir, name);
    const db = openWithAnyTestPepper(path);
    if (!db) {
      unopened.push(name);
      continue;
    }
    const table = db
      .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name=?`)
      .get(TABLE);
    if (!table) {
      db.close();
      continue; // this partition does not carry the mount index
    }
    const cols = (db.prepare(`PRAGMA table_info("${TABLE}")`).all() as { name: string }[]).map(
      (c) => c.name,
    );
    let applied = false;
    if (!cols.includes(COLUMN)) {
      db.exec(ADD_COLUMN);
      applied = true;
    }
    db.exec(ADD_INDEX);
    db.close();
    if (applied) {
      touched++;
      process.stderr.write(`${name}: +${TABLE}.${COLUMN}\n`);
    }
  }
  process.stderr.write(`migrated ${touched} fixture(s)\n`);
  if (unopened.length) {
    // Loud, never silent: an unopened fixture is one this script did NOT migrate,
    // and its family will fail with `no such column: linkGroupId`.
    process.stderr.write(
      `NOT OPENED (add its pepper to TEST_PEPPERS): ${unopened.join(', ')}\n`
    );
    process.exitCode = 1;
  }
}

main();
