/**
 * Tier-2 oracle — the RECURSIVE vault projection (`projectArrayIntoVaultFolder`
 * + `projectVaultWardrobe`, P4.D279 C1 §4; v4 `4a38222e6`).
 *
 * Opens COPIES of the keystone fixture pair and drives v4's REAL projection
 * over the pre-seeded "Projection Scratch" store (pinned id), mirroring
 * `vault-projection-recursive.test.ts`'s beats and
 * `archived-survives-projection.test.ts`'s "never sweeps a file that is not an
 * item" over a REAL store:
 *
 *   1. `projectVaultWardrobe` of seven items filed across `formal` / `Formal` /
 *      `Formal/Evening` / `Casual` / the root — the existing `Formal/Coat.md`
 *      is rewritten under a casing-only change and NOT deleted, two `Coat`s in
 *      one folder suffix `-1` while `Casual/Coat.md` keeps its name, an item
 *      titled `Instructions` lands on `-1` beside the preserved
 *      `Formal/Instructions.md`, the stale `Old/Hat.md` + `Formal/Old gown.md`
 *      are swept at depth, the note / half-written garment / `images/` caption /
 *      both `instructions.md` survive, the legacy `wardrobe.json` is removed.
 *   2. A move to the root + a drop: the old path is swept, the folder row stays.
 *   3. Generic recursive projections into an EMPTY `Lab/` folder: an empty
 *      target subfolder ensured (the `Recursive vault projection ensured
 *      subfolders` DEBUG), per-folder suffixing with `preserveFileNames` claimed
 *      in every folder, `ownsFile` narrowing the sweep, `excludeSubfolders`
 *      sparing a subtree, a casing-only rewrite kept.
 *   4. The FLAT callers unchanged: top-level listing, one unconditional ensure,
 *      a slashless name as-is, every stale `.md` swept; an empty projection
 *      ensures nothing.
 *   5. `projectVaultWardrobe([])` — the wardrobe empties, the non-items stay.
 *
 * After every op: the store's folder-row paths and link paths (as stored,
 * sorted) and the captured projection lines; at the end the three doc-store
 * tables for the scratch store in the minted-values remap form.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/vault-projection-recursive-tier2.ts \
 *     > /tmp/oracle-vault-projection-recursive.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Spec {
  testPepperBase64: string;
  scratchMountPointId: string;
}

const PINNED_MESSAGES = [
  'Recursive vault projection ensured subfolders',
  'Failed to delete stale vault file during folder projection',
  'Failed to delete legacy wardrobe.json after folder projection',
];

const RECURSIVE = {
  preserveFileNames: ['instructions.md'],
  recursive: { excludeSubfolders: ['images'] },
};

function renderValue(v: unknown): string {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'string') return v;
  if (typeof v === 'boolean' || typeof v === 'number') return String(v);
  return JSON.stringify(v);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-keystone-tier2.json'), 'utf8'),
  ) as Spec;
  const S = spec.scratchMountPointId;

  const mainFixture = process.env.QT_FIXTURE_WKS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WKS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WKS_MAIN and QT_FIXTURE_WKS_MOUNT must point at the seeded fixture .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wks-projection-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'wks-main-work.db');
  const mountWork = join(scratch, 'wks-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  let captured: string[] = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      if (!PINNED_MESSAGES.includes(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      const rendered = Object.entries(fields)
        .map(([k, v]) => ` ${k}=${renderValue(v)}`)
        .join('');
      captured.push(`${data.level.toUpperCase()} ${data.message}${rendered}`);
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { projectArrayIntoVaultFolder } = await import(
    '@/lib/database/repositories/vault-overlay/vault-projection'
  );
  const { projectVaultWardrobe } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-sync'
  );

  await initializeDatabase();
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');

  const folders = (): string[] =>
    (midb.prepare('SELECT path FROM doc_mount_folders WHERE mountPointId = ?').all(S) as Array<{ path: string }>)
      .map((r) => r.path)
      .sort();
  const links = (): string[] =>
    (midb.prepare('SELECT relativePath FROM doc_mount_file_links WHERE mountPointId = ?').all(S) as Array<{ relativePath: string }>)
      .map((r) => r.relativePath)
      .sort();

  const item = (id: string, title: string, folder: string, types: string[], archivedAt: string | null = null) => ({
    id,
    characterId: null,
    title,
    description: null,
    imagePrompt: null,
    types,
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt,
    folder,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
  });
  const F = (n: number) => `f9000000-0000-4000-8000-${String(n).padStart(12, '0')}`;
  const mapper = (file: string) => ({ fileName: file, content: `body of ${file}` });

  const ops: Array<Record<string, unknown>> = [];
  const op = async (label: string, fn: () => Promise<unknown>) => {
    captured = [];
    let result: unknown;
    try {
      await fn();
      result = { ok: true };
    } catch (error) {
      result = { threw: error instanceof Error ? error.message : String(error) };
    }
    ops.push({ label, result, folders: folders(), links: links(), lines: captured.slice() });
  };

  await op('wardrobe: initial (casing, per-folder suffix, preserved, note kept, sweep)', () =>
    projectVaultWardrobe(S, S, [
      item(F(1), 'Coat', 'formal', ['top']),
      item(F(7), 'Gown', 'Formal/Evening', ['top', 'bottom']),
      item(F(8), 'Gloves', 'Formal/Evening', ['accessories']),
      item(F(4), 'Coat', 'Casual', ['top']),
      item(F(5), 'Coat', 'formal', ['top']),
      item(F(6), 'Instructions', 'Formal', ['top']),
      item(F(9), 'Shelved hat', '', ['accessories'], '2026-02-01T00:00:00.000Z'),
    ] as never),
  );
  await op('wardrobe: move Gown to the root, drop Gloves (old path swept, folder row stays)', () =>
    projectVaultWardrobe(S, S, [
      item(F(1), 'Coat', 'formal', ['top']),
      item(F(7), 'Gown', '', ['top', 'bottom']),
      item(F(4), 'Coat', 'Casual', ['top']),
      item(F(5), 'Coat', 'formal', ['top']),
      item(F(6), 'Instructions', 'Formal', ['top']),
      item(F(9), 'Shelved hat', '', ['accessories'], '2026-02-01T00:00:00.000Z'),
    ] as never),
  );
  await op('Lab recursive: an empty target subfolder is ensured', () =>
    projectArrayIntoVaultFolder(S, 'Lab', ['Rainy/Mac.md', 'Rainy/Hat.md'], mapper, S, RECURSIVE),
  );
  await op('Lab recursive: per-folder suffixing, preserved names claimed in every folder', () =>
    projectArrayIntoVaultFolder(
      S,
      'Lab',
      ['Rainy/Mac.md', 'rainy/mac.md', 'Formal/Mac.md', 'Formal/instructions.md', 'Mac.md', 'instructions.md'],
      mapper,
      S,
      RECURSIVE,
    ),
  );
  await op('Lab recursive: ownsFile narrows the sweep', () =>
    projectArrayIntoVaultFolder(S, 'Lab', [], mapper, S, {
      ...RECURSIVE,
      ownsFile: (doc) => !doc.relativePath.toLowerCase().endsWith('mac.md'),
    }),
  );
  await op('Lab recursive: excludeSubfolders spares a subtree', () =>
    projectArrayIntoVaultFolder(S, 'Lab', [], mapper, S, {
      preserveFileNames: ['instructions.md'],
      recursive: { excludeSubfolders: ['rainy'] },
    }),
  );
  await op('Lab recursive: a casing-only rewrite is not deleted', () =>
    projectArrayIntoVaultFolder(S, 'Lab', ['rainy/mac.md'], mapper, S, RECURSIVE),
  );
  await op('Prompts flat: top-level only, one ensure, slashless name as-is, every stale .md swept', () =>
    projectArrayIntoVaultFolder(S, 'Prompts', ['Default.md', 'default.md'], mapper, S),
  );
  await op('Prompts2 flat: an empty projection ensures nothing', () =>
    projectArrayIntoVaultFolder(S, 'Prompts2', [], mapper, S),
  );
  await op('wardrobe: empty projection — items swept, non-items stay', () => projectVaultWardrobe(S, S, []));

  const dumpTable = (table: string, orderBy: string, where: string) => {
    const columns = (midb.pragma(`table_info(${table})`) as Array<{ name: string }>).map((c) => c.name);
    const rawRows = midb.prepare(`SELECT t.* FROM ${table} t ${where}`).all(S) as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };
  const tables = {
    documents: dumpTable(
      'doc_mount_documents',
      'contentSha256',
      'JOIN doc_mount_file_links l ON l.fileId = t.fileId WHERE l.mountPointId = ?',
    ),
    links: dumpTable('doc_mount_file_links', 'relativePath', 'WHERE t.mountPointId = ?'),
    folders: dumpTable('doc_mount_folders', 'path', 'WHERE t.mountPointId = ?'),
  };

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'vault-projection-recursive-tier2', ops, tables }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vault-projection-recursive-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
