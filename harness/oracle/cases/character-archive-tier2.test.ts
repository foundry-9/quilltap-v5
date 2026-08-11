/**
 * @jest-environment node
 *
 * P4.D65 character-archive ORACLE (tier 2): drives v4's REAL
 * `lib/characters/archive-service.ts` — `archiveCharacter` and
 * `rehydrateCharacter` — over a FRESH copy of the committed
 * `character-archive-{main,mount}.db` pair per case, and emits three things the
 * Rust port is diffed against:
 *
 *   - `result` — the returned result object, or `{error: {name, message}}` for
 *     the refusal arms (the route's status mapping is asserted Rust-side; these
 *     are the CLASSES it dispatches on).
 *   - `state`  — a canonicalized dump of every table the lifecycle can touch,
 *     across BOTH partitions.
 *   - `bundle` — the archive bundle DECRYPTED and parsed. ⚠ The ciphertext is
 *     nondeterministic (fresh salt + IV per bundle), so the bytes on disk can
 *     never be compared; the plaintext export is the comparand, and each side
 *     decrypts with the passphrase it resolves for itself. The fixture instance
 *     has no user passphrase, so both resolve `INTERNAL_PASSPHRASE`.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-character-archive-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/character-archive-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/character-archive.json"       "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARCHIVE_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
 *   QT_FIXTURE_ARCHIVE_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-character-archive.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- character-archive-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  sable: string;
  tor: string;
  quill: string;
  chatSeated: string;
  chatRemoved: string;
  memories: Array<{ id: string; owner: string; content: string }>;
}

/** The tables the archive lifecycle can touch, per partition. */
const MAIN_TABLES = [
  'characters',
  'chats',
  'memories',
  'files',
  'embedding_status',
  'vector_indices',
  'vector_entries',
];
const MOUNT_TABLES = [
  'doc_mount_points',
  'doc_mount_files',
  'doc_mount_documents',
  'doc_mount_folders',
  'doc_mount_file_links',
  'doc_mount_chunks',
  'doc_mount_blobs',
];

/** BLOB → hex, undefined → null; then rows sorted by their canonical JSON. */
function canonicalizeRows(rows: Array<Record<string, unknown>>): Array<Record<string, unknown>> {
  const mapped = rows.map((row) => {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(row).sort()) {
      const v = (row as Record<string, unknown>)[key];
      if (v instanceof Buffer) {
        out[key] = `blob:${v.length}`;
      } else if (v === undefined) {
        out[key] = null;
      } else {
        out[key] = v;
      }
    }
    return out;
  });
  mapped.sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
  return mapped;
}

type CaseName =
  | 'archive_sable'
  | 'archive_sable_twice'
  | 'archive_then_rehydrate'
  | 'archive_tor_no_vault'
  | 'rehydrate_not_archived'
  | 'archive_missing_character'
  | 'rehydrate_no_bundle'
  | 'rehydrate_missing_bundle_file';

async function runCase(
  spec: Spec,
  name: CaseName,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // jest.setup stubs the storage manager to return the literal string "mock
  // file content"; the bundle round-trip needs the REAL disk backend.
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/memory/memory-gate', () => jest.requireActual('@/lib/memory/memory-gate'));

  const work = mkdtempSync(join(scratch, 'arch-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  mkdirSync(join(work, 'data'), { recursive: true });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = work;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { archiveCharacter, rehydrateCharacter } = await import(
    '@/lib/characters/archive-service'
  );
  const { decryptArchive, isEncryptedArchive } = await import('@/lib/characters/archive-crypto');
  const { assembleExportFromStream } = await import('@/lib/import/quilltap-import-stream');
  const { readNdjsonLines } = await import('@/lib/import/ndjson-reader');
  const { fileStorageManager } = await import('@/lib/file-storage/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  // v4 bakes `packageJson.version` into every manifest, so the bundle's BYTE
  // LENGTH depends on it. Emit it so the Rust side can inject the same string
  // and `files.size` becomes a real comparand rather than a version artifact.
  const appVersion = (
    JSON.parse(fs.readFileSync(join(process.cwd(), 'package.json'), 'utf8')) as {
      version: string;
    }
  ).version;
  const payload: Record<string, unknown> = { name, appVersion };
  try {
    const capture = async (fn: () => Promise<unknown>): Promise<unknown> => {
      try {
        return await fn();
      } catch (error) {
        const e = error as Error;
        return { error: { name: e.name, message: e.message } };
      }
    };

    switch (name) {
      case 'archive_sable':
        payload.result = await capture(() => archiveCharacter(spec.userId, spec.sable));
        break;
      case 'archive_sable_twice':
        await archiveCharacter(spec.userId, spec.sable);
        payload.result = await capture(() => archiveCharacter(spec.userId, spec.sable));
        break;
      case 'archive_then_rehydrate':
        await archiveCharacter(spec.userId, spec.sable);
        payload.result = await capture(() => rehydrateCharacter(spec.userId, spec.sable));
        break;
      case 'archive_tor_no_vault':
        payload.result = await capture(() => archiveCharacter(spec.userId, spec.tor));
        break;
      case 'rehydrate_not_archived':
        payload.result = await capture(() => rehydrateCharacter(spec.userId, spec.sable));
        break;
      case 'archive_missing_character':
        payload.result = await capture(() =>
          archiveCharacter(spec.userId, '00000000-0000-4000-8000-00000000dead'),
        );
        break;
      case 'rehydrate_no_bundle':
        // A tombstone that never carried a bundle: flag the row directly, the
        // way a pre-§4.2a archive left it.
        await rawQuery('UPDATE characters SET archivedAt = ? WHERE id = ?', [
          '2026-03-02T00:00:00.000Z',
          spec.sable,
        ]);
        payload.result = await capture(() => rehydrateCharacter(spec.userId, spec.sable));
        break;
      case 'rehydrate_missing_bundle_file': {
        const archived = (await archiveCharacter(spec.userId, spec.sable)) as {
          archiveFileId: string | null;
        };
        if (archived.archiveFileId) {
          await getRepositories().files.delete(archived.archiveFileId);
        }
        payload.result = await capture(() => rehydrateCharacter(spec.userId, spec.sable));
        break;
      }
    }

    // The bundle, DECRYPTED (never the ciphertext — see the header).
    const archiveRows = (await rawQuery(
      "SELECT id, storageKey, sha256, mimeType, size, category, source, folderPath, fileStatus, linkedTo, tags, projectId FROM files WHERE category = 'ARCHIVE'",
      [],
    )) as Array<Record<string, unknown>>;
    const bundles: unknown[] = [];
    for (const row of canonicalizeRows(archiveRows)) {
      const entry = await getRepositories().files.findById(row.id as string);
      if (!entry) continue;
      const raw = await fileStorageManager.downloadFile(entry);
      const plaintext = isEncryptedArchive(raw) ? decryptArchive(raw) : raw;
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(plaintext);
          controller.close();
        },
      });
      const parsed = await assembleExportFromStream(readNdjsonLines(stream));
      bundles.push({ manifest: parsed.manifest, data: parsed.data });
    }
    payload.bundles = bundles;

    // The whole reachable state, both partitions.
    const state: Record<string, unknown> = {};
    for (const table of MAIN_TABLES) {
      const rows = (await rawQuery(`SELECT * FROM "${table}"`, [])) as Array<
        Record<string, unknown>
      >;
      state[table] = canonicalizeRows(rows);
    }
    const midb = getRawMountIndexDatabase();
    for (const table of MOUNT_TABLES) {
      const rows = midb
        ? (midb.prepare(`SELECT * FROM "${table}"`).all() as Array<Record<string, unknown>>)
        : [];
      state[table] = canonicalizeRows(rows);
    }
    payload.state = state;
    return payload;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'character-archive.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_ARCHIVE_MAIN ?? '',
    mount: process.env.QT_FIXTURE_ARCHIVE_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-character-archive-'));
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseName[] = [
    'archive_sable',
    'archive_sable_twice',
    'archive_then_rehydrate',
    'archive_tor_no_vault',
    'rehydrate_not_archived',
    'archive_missing_character',
    'rehydrate_no_bundle',
    'rehydrate_missing_bundle_file',
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    outLines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`character-archive oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('character-archive tier-2 oracle', async () => {
  await main();
});
