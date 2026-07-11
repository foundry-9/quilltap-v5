/**
 * @jest-environment node
 *
 * P4.6m (unit 4) tier-2 ORACLE: drives v4's REAL
 * `writeCharacterAvatarToVault({ kind: 'main' })`
 * (`lib/file-storage/character-vault-bridge.ts`) — the delete-then-insert avatar
 * write the ST-import multipart leg performs — over a FRESH copy of the
 * committed characters fixture (Aria, who already has an `images/avatar.webp`,
 * so the delete-then-insert replacement path is exercised).
 *
 * The Rust port (`quilltap_core::services::image_job_storage::
 * write_main_avatar_to_vault`) is diffed against this. The stored WebP BYTES are
 * the declared codec seam (sharp vs the host codec), so `sha256`/`fileId`/the raw
 * blob are blanked; the link row's stable fields (relativePath / fileName /
 * originalMimeType / storedMimeType) and the blob's decoded metadata
 * (width / height / format) are diffed exactly.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-avatar-write-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/character-avatar-write-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"                   "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-avatar-write.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- character-avatar-write-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
}

const ARIA = 'a1000000-0000-4000-8000-000000000001';
// A committed 16×16 RGB PNG (both codecs transcode it to a 16×16 WebP).
const AVATAR_PNG_B64 =
  'iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAFklEQVR4nGM4YaNBEmIY1TCqYfhqAAAeBCwQMwPSngAAAABJRU5ErkJggg==';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'characters.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CHARACTERS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CHARACTERS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-avatar-write-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );

  const work = mkdtempSync(join(scratch, 'aw-'));
  copyFileSync(fixtures.main, join(work, 'main.db'));
  copyFileSync(fixtures.mount, join(work, 'mount.db'));
  process.env.SQLITE_PATH = join(work, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    const { writeCharacterAvatarToVault, getCharacterVaultStore } = await import(
      '@/lib/file-storage/character-vault-bridge'
    );
    const png = Buffer.from(AVATAR_PNG_B64, 'base64');
    const written = await writeCharacterAvatarToVault({
      characterId: ARIA,
      kind: 'main',
      filename: 'hero.png',
      content: png,
      contentType: 'image/png',
    });

    const vault = await getCharacterVaultStore(ARIA);
    const { getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const midb = getRawMountIndexDatabase() as unknown as {
      prepare: (s: string) => {
        all: (...a: unknown[]) => unknown;
        get: (...a: unknown[]) => unknown;
      };
    };
    // The single link at the canonical main path (delete-then-insert replaced
    // Aria's pre-existing avatar). storedMimeType/sha256 live on the blob.
    const links = midb
      .prepare(
        'SELECT l.relativePath, l.fileName, l.originalMimeType, b.storedMimeType, b.sha256 ' +
          'FROM doc_mount_file_links l JOIN doc_mount_blobs b ON b.fileId = l.fileId ' +
          "WHERE l.mountPointId = ? AND l.relativePath = 'images/avatar.webp' ORDER BY l.id",
      )
      .all(vault.mountPointId) as Array<Record<string, unknown>>;

    // The stored blob's decoded metadata (dims/format — the seam-safe check).
    const blobRow = midb
      .prepare(
        'SELECT b.data AS data FROM doc_mount_blobs b ' +
          'JOIN doc_mount_file_links l ON l.fileId = b.fileId ' +
          "WHERE l.mountPointId = ? AND l.relativePath = 'images/avatar.webp' LIMIT 1",
      )
      .get(vault.mountPointId) as { data: Buffer } | undefined;

    const sharpPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/sharp',
    );
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const sharp = require(sharpPath);
    let meta: { width?: number; height?: number; format?: string; nonEmpty?: boolean } = {};
    if (blobRow?.data) {
      const m = await sharp(blobRow.data).metadata();
      meta = {
        width: m.width,
        height: m.height,
        format: m.format,
        nonEmpty: blobRow.data.length > 0,
      };
    }

    const payload = {
      name: 'avatar_write_main',
      links,
      linkCount: links.length,
      storedMimeType: written.storedMimeType,
      meta,
    };
    fs.writeFileSync(outPath, JSON.stringify(payload) + '\n');
    process.stderr.write(`avatar-write oracle wrote ${outPath}\n`);
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

test('character-avatar-write-tier2 oracle', async () => {
  await main();
});
