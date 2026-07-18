import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { makeDbKeyFile } from './dbkey';
import { ARTIFACTS_DIR, E2E_PASSPHRASE, FIXTURES_DIR, INSTANCE_DIR, TEST_PEPPER } from './env';

/**
 * The d68638b4-round §4 unification wire (P4.6ay unit 7's e2e-fixture
 * obligation): seed a `Tools/` roster into ARIA's character vault in the SHARED
 * e2e instance, copied from lane AY's committed
 * `pascal-run-custom-mount.db` differential fixture. The
 * `salon-custom-tools-flow.spec.ts` popup→run→bubble beat is probe-guarded on
 * the composer's "Custom tools" button, which the SPA gates on a non-empty
 * roster — this seed is what makes the button (and so the beat) light up.
 *
 * Mechanics (the `seedCourierImagesFixture` pattern, cut down): a throwaway
 * READ instance is assembled over the committed pascal fixture; the vault-A
 * `Tools` folder row, its three `Tools/*.tool.json` link rows, and their
 * files/documents rows are read via the CLI's `--json` raw SQL and re-inserted
 * into the shared instance's mount partition with `--write`. Two facts make
 * this a clean copy:
 *
 *  - Every copied id is v4-MINTED (fresh UUIDs per fixture build), so nothing
 *    can collide with the salon fixture's rows. The ONE remap is
 *    `mountPointId`: vault A's minted id → Aria's salon vault
 *    (`7e056034-…` — survey-verified; the salon scaffold creates NO `Tools`
 *    folder, so the folder insert cannot trip the NOCASE unique index).
 *  - `doc_mount_files` / `doc_mount_documents` are mount-global (no
 *    `mountPointId` column) — they copy verbatim; only the folder + link rows
 *    carry the remap.
 *
 * Tool semantics on the salon instance: `coin` runs public, `whispered` runs
 * private (the operator-facing carve-out assertion), and `ansible` is
 * metadata-gated on `hasAnsibleAccess` — Aria's salon vault has no
 * `metadata.json`, so it hydrates `{}` and ansible deals its catch-all. All
 * three rolls are `min === max`, so outcomes are deterministic. Data seeding
 * for the walk, NOT a fixture regen — the committed fixtures are never
 * mutated.
 */

/** Vault A's minted mount id, from `pascal-run-custom-main.db.meta.json`. */
const PASCAL_VAULT_A = '67d3538d-54bf-49b7-be86-08d02654cb0c';
/** Aria's vault in the committed salon fixture (survey-verified 2026-07-17). */
const ARIA_VAULT = '7e056034-f5ae-4fc6-a6ec-6c646b72d016';

type Row = Record<string, unknown>;

export function seedPascalToolsFixture(cli: string): void {
  // 1. The throwaway read instance over the committed pascal fixture.
  const src = resolve(ARTIFACTS_DIR, 'pascal-src');
  const srcData = resolve(src, 'data');
  rmSync(src, { recursive: true, force: true });
  mkdirSync(srcData, { recursive: true });
  copyFileSync(
    resolve(FIXTURES_DIR, 'pascal-run-custom-main.db'),
    resolve(srcData, 'quilltap.db'),
  );
  copyFileSync(
    resolve(FIXTURES_DIR, 'pascal-run-custom-mount.db'),
    resolve(srcData, 'quilltap-mount-index.db'),
  );
  writeFileSync(resolve(srcData, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

  // 2. Vault A's Tools rows: the folder, its links, and their files/documents.
  const folders = readRows(
    cli,
    src,
    `SELECT * FROM doc_mount_folders WHERE mountPointId = '${PASCAL_VAULT_A}' AND name = 'Tools'`,
  );
  const links = readRows(
    cli,
    src,
    `SELECT * FROM doc_mount_file_links WHERE mountPointId = '${PASCAL_VAULT_A}' AND relativePath LIKE 'Tools/%'`,
  );
  const fileIds = links.map((l) => `'${String(l['fileId'])}'`).join(',');
  const files = fileIds
    ? readRows(cli, src, `SELECT * FROM doc_mount_files WHERE id IN (${fileIds})`)
    : [];
  const documents = fileIds
    ? readRows(cli, src, `SELECT * FROM doc_mount_documents WHERE fileId IN (${fileIds})`)
    : [];
  if (folders.length !== 1 || links.length === 0) {
    throw new Error(
      `pascal-tools-fixture: expected 1 Tools folder + links in vault A, got ${folders.length}/${links.length}`,
    );
  }

  // 3. Re-insert into the shared instance, remapped onto Aria's vault.
  const tables: Array<[string, Row[]]> = [
    ['doc_mount_folders', folders],
    ['doc_mount_files', files],
    ['doc_mount_documents', documents],
    ['doc_mount_file_links', links],
  ];
  for (const [table, rows] of tables) {
    if (rows.length === 0) {
      continue;
    }
    const cols = Object.keys(rows[0]);
    const values = rows
      .map((row) => `(${cols.map((c) => lit(remapVault(row[c]))).join(', ')})`)
      .join(',\n');
    runWrite(
      cli,
      INSTANCE_DIR,
      `INSERT OR REPLACE INTO ${table} (${cols.map((c) => `"${c}"`).join(', ')}) VALUES ${values};`,
    );
  }

  rmSync(src, { recursive: true, force: true });
}

/** The one remap: vault A's mount id → Aria's salon vault. */
function remapVault(v: unknown): unknown {
  if (typeof v !== 'string') {
    return v;
  }
  return v.split(PASCAL_VAULT_A).join(ARIA_VAULT);
}

/** One CLI `--json` raw-SQL read against `dir`'s mount partition. */
function readRows(cli: string, dir: string, sql: string): Row[] {
  const res = spawnSync(cli, ['db', '--data-dir', dir, '--mount-points', '--json', sql], {
    env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (res.status !== 0) {
    throw new Error(`pascal-tools-fixture read failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
  return JSON.parse(res.stdout) as Row[];
}

function runWrite(cli: string, dir: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', dir, '--mount-points', '--write', sql], {
    env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (res.status !== 0) {
    throw new Error(
      `pascal-tools-fixture write failed (${sql.slice(0, 120)}…):\n${res.stdout}\n${res.stderr}`,
    );
  }
}

/** A SQL literal for a CLI-JSON value (TEXT/NUMBER/NULL — no blobs ride this path). */
function lit(v: unknown): string {
  if (v === null || v === undefined) {
    return 'NULL';
  }
  if (typeof v === 'number') {
    return Number.isFinite(v) ? String(v) : 'NULL';
  }
  return `'${String(v).replace(/'/g, "''")}'`;
}
