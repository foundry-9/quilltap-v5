/**
 * @jest-environment node
 *
 * P4.D65 archive RE-ENCRYPTION oracle (tier 2): drives v4's REAL
 * `reencryptArchiveBundles` (`lib/characters/archive-reencrypt.ts`) over a
 * FRESH copy of the committed `character-archive-{main,mount}.db` pair per
 * case — the second phase of a passphrase change (spec §4.2c), which v4 runs
 * from the unlock route after the `.dbkey` re-wrap.
 *
 * ── WHY THE BUNDLES ARE PLANTED, NOT ARCHIVED ────────────────────────────────
 * One case DOES archive Sable for real, so the sweep meets a bundle the archive
 * service itself wrote. The rest plant `files` rows with hand-made bytes,
 * because the shapes that matter here are ones `archiveCharacter` cannot
 * produce: a PLAINTEXT pre-encryption bundle, a bundle sealed under a
 * DIFFERENT passphrase (one that survived an earlier half-finished change), and
 * a row whose storage key names nothing. Those three are the whole reason the
 * result carries per-file failures instead of throwing.
 *
 * ── WHAT IS EMITTED PER CASE ─────────────────────────────────────────────────
 *   result   — the `ArchiveReencryptResult` (total / reencrypted / failures)
 *   opens    — per bundle, whether its persisted bytes open under the NEW
 *              passphrase, and whether they carry an encryption header. The
 *              ciphertext itself can never be compared (fresh salt + IV per
 *              write, on both sides); what is comparable is which passphrase
 *              the artifact now answers to, and that is the whole contract.
 *   files    — the `files` table's post-sweep rows (`size` moves when a bundle
 *              is rewritten; a failed one must be untouched)
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-archive-reencrypt-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/archive-reencrypt-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/character-archive.json"       "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ARCHIVE_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
 *   QT_FIXTURE_ARCHIVE_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-archive-reencrypt.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- archive-reencrypt-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  sable: string;
}

/** The passphrases the sweep moves between. Both are REAL key material — the
 *  empty-string → internal-sentinel resolution is the ROUTE's job, one layer up
 *  (`unlock/route.ts:352`), and is pinned Rust-side at the engine wire. */
const OLD_PASSPHRASE = 'the-old-brass-key';
const NEW_PASSPHRASE = 'a-newer-brass-key';

/** Planted `files` rows — pinned so both sides can name them. */
const F_PLAINTEXT = 'f0000000-0000-4000-8000-0000000000a1';
const F_FOREIGN = 'f0000000-0000-4000-8000-0000000000a2';
const F_NO_BYTES = 'f0000000-0000-4000-8000-0000000000a3';

/** A minimal but REAL bundle body: one envelope line of NDJSON. */
const BUNDLE_NDJSON =
  JSON.stringify({
    kind: '__envelope__',
    format: 'qtap-ndjson',
    version: 1,
    manifest: { exportType: 'characters', counts: {} },
  }) + '\n';

type CaseName =
  | 'sweep_empty'
  | 'sweep_real_bundle'
  | 'sweep_plaintext_bundle'
  | 'sweep_foreign_passphrase'
  | 'sweep_missing_bytes'
  | 'sweep_mixed_library';

function sha256(buf: Buffer): string {
  return createHash('sha256').update(buf).digest('hex');
}

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
  // jest.setup stubs the storage manager to a literal string; the sweep reads
  // and rewrites real bytes.
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/memory/memory-gate', () => jest.requireActual('@/lib/memory/memory-gate'));

  const work = mkdtempSync(join(scratch, 'reenc-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  mkdirSync(join(work, 'data'), { recursive: true });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = work;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { fileStorageManager } = await import('@/lib/file-storage/manager');
  const { reencryptArchiveBundles } = await import('@/lib/characters/archive-reencrypt');
  const { archiveCharacter } = await import('@/lib/characters/archive-service');
  const { INTERNAL_PASSPHRASE } = await import('@/lib/startup/dbkey');
  const { encryptArchive, decryptArchive, isEncryptedArchive } = await import(
    '@/lib/characters/archive-crypto'
  );

  await initializeDatabase();
  const repos = getRepositories();

  /** Plant an ARCHIVE `files` row, optionally with bytes on disk. */
  const plant = async (id: string, bytes: Buffer | null, filename: string): Promise<void> => {
    const storageKey = bytes ? `archives/${id}/${filename}` : `archives/${id}/gone.qtap`;
    if (bytes) {
      await fileStorageManager.uploadRaw({
        storageKey,
        content: bytes,
        contentType: 'application/octet-stream',
      });
    }
    await repos.files.create(
      {
        userId: spec.userId,
        linkedTo: [],
        tags: [],
        sha256: bytes ? sha256(bytes) : sha256(Buffer.alloc(0)),
        originalFilename: filename,
        mimeType: 'application/octet-stream',
        size: bytes ? bytes.length : 0,
        source: 'GENERATED',
        category: 'ARCHIVE',
        storageKey,
      } as never,
      { id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
    );
  };

  const payload: Record<string, unknown> = { name };
  // Every case sweeps OLD → NEW except the real-bundle one: `archiveCharacter`
  // seals under whatever `resolveArchivePassphrase()` answers, and this fixture
  // instance has no user passphrase, so its bundle is sealed under the internal
  // sentinel. Sweeping it from OLD would (correctly) report a mismatch and
  // prove nothing about the happy path.
  let oldPassphrase = OLD_PASSPHRASE;
  try {
    switch (name) {
      case 'sweep_empty':
        break;
      case 'sweep_real_bundle':
        // The one case whose bundle the archive service really wrote — proof
        // the sweep and the writer agree about the on-disk layout.
        await archiveCharacter(spec.userId, spec.sable);
        oldPassphrase = INTERNAL_PASSPHRASE;
        break;
      case 'sweep_plaintext_bundle':
        await plant(F_PLAINTEXT, Buffer.from(BUNDLE_NDJSON, 'utf-8'), 'pre-encryption.qtap');
        break;
      case 'sweep_foreign_passphrase':
        await plant(
          F_FOREIGN,
          Buffer.from(encryptArchive(Buffer.from(BUNDLE_NDJSON, 'utf-8'), 'a-third-key')),
          'foreign.qtap',
        );
        break;
      case 'sweep_missing_bytes':
        await plant(F_NO_BYTES, null, 'vanished.qtap');
        break;
      case 'sweep_mixed_library':
        // The point of the sweep's design: one bad bundle must not stop the
        // others. Plant the failing one FIRST so a short-circuiting port is
        // caught by the survivors, not merely by the ordering.
        await plant(
          F_FOREIGN,
          Buffer.from(encryptArchive(Buffer.from(BUNDLE_NDJSON, 'utf-8'), 'a-third-key')),
          'foreign.qtap',
        );
        await plant(F_PLAINTEXT, Buffer.from(BUNDLE_NDJSON, 'utf-8'), 'pre-encryption.qtap');
        await plant(F_NO_BYTES, null, 'vanished.qtap');
        break;
    }

    payload.oldPassphrase = oldPassphrase;
    payload.result = await reencryptArchiveBundles(oldPassphrase, NEW_PASSPHRASE);

    // Which passphrase does each persisted artifact answer to now?
    const rows = (await rawQuery(
      "SELECT id, originalFilename, size, storageKey FROM files WHERE category = 'ARCHIVE' ORDER BY id",
      [],
    )) as Array<Record<string, unknown>>;
    const opens: unknown[] = [];
    for (const row of rows) {
      const entry = await repos.files.findById(row.id as string);
      let encrypted: boolean | null = null;
      let opensWithNew: boolean | null = null;
      let opensWithOld: boolean | null = null;
      try {
        const bytes = entry ? await fileStorageManager.downloadFile(entry) : null;
        if (bytes) {
          encrypted = isEncryptedArchive(bytes);
          const tryOpen = (pass: string): boolean => {
            try {
              return encrypted ? decryptArchive(bytes, pass).length > 0 : false;
            } catch {
              return false;
            }
          };
          opensWithNew = tryOpen(NEW_PASSPHRASE);
          opensWithOld = tryOpen(oldPassphrase);
        }
      } catch {
        // A row with no bytes cannot be opened at all — that IS the answer.
      }
      opens.push({
        originalFilename: row.originalFilename,
        encrypted,
        opensWithNew,
        opensWithOld,
      });
    }
    payload.opens = opens;
    payload.files = rows.map((r) => ({
      originalFilename: r.originalFilename,
      size: r.size,
    }));
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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-archive-reencrypt-'));
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseName[] = [
    'sweep_empty',
    'sweep_real_bundle',
    'sweep_plaintext_bundle',
    'sweep_foreign_passphrase',
    'sweep_missing_bytes',
    'sweep_mixed_library',
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    outLines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`archive-reencrypt oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('archive-reencrypt tier-2 oracle', async () => {
  await main();
});
