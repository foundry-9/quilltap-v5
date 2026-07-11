/**
 * @jest-environment node
 *
 * P4.6m (unit 2) tier-2 ORACLE: drives v4's REAL `saveToCharacterGallery`
 * (`lib/photos/character-gallery-service.ts`) — the bytes-entry write spine the
 * quilltap-web multipart-upload leg calls — over a FRESH copy of the committed
 * characters fixture per case. Emits the freshly-written `photos/` link dump
 * (deterministic under a frozen clock; content-addressed fileId) for the OK
 * cases, and the thrown message for the refusal/400 arms.
 *
 * The Rust port (`quilltap_core::photos::character_gallery_service::
 * save_to_character_gallery`) is diffed against these. The write spine is also
 * exercised by the standing `photo_save_link` case (via the linkId leg); this
 * case pins the UPLOAD-specific filename→path branches (dotless slug vs the
 * dotted sanitize) plus the dedup guard and the two 400-keyword arms.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-photo-upload-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/character-photo-upload-tier2.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"                   "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-photo-upload.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- character-photo-upload-tier2
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const ARIA = 'a1000000-0000-4000-8000-000000000001';
const FIXED_KEPT_AT = '2026-04-01T12:00:00.000Z';

interface UploadCase {
  name: string;
  /** base64 image bytes. */
  data: string;
  filename: string;
  mimeType: string;
  caption?: string | null;
  tags?: string[];
  /** For the dedup arm: upload once, then again with the same bytes. */
  twice?: boolean;
}

async function runCase(
  spec: Spec,
  c: UploadCase,
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
  // Un-mock the character-vault-bridge (jest.setup stubs it) so the gallery
  // resolves the REAL vault mount point against the real DB.
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );

  const work = mkdtempSync(join(scratch, 'upload-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  const RealDate = Date;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(FIXED_KEPT_AT);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return RealDate.parse(FIXED_KEPT_AT);
    }
  } as unknown as DateConstructor;

  try {
    const { saveToCharacterGallery } = await import('@/lib/photos/character-gallery-service');
    const { getCharacterVaultStore } = await import('@/lib/file-storage/character-vault-bridge');
    const buffer = Buffer.from(c.data, 'base64');
    const input = {
      characterId: ARIA,
      data: buffer,
      filename: c.filename,
      mimeType: c.mimeType,
      caption: c.caption ?? null,
      tags: c.tags,
      repos: getRepositories(),
    };

    let error: string | undefined;
    let body: unknown;
    try {
      if (c.twice) await saveToCharacterGallery(input);
      const out = await saveToCharacterGallery({ ...input, repos: getRepositories() });
      body = out;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }

    const vault = await getCharacterVaultStore(ARIA);
    let savedLinks: unknown = [];
    if (vault) {
      const { getRawMountIndexDatabase } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      const midb = getRawMountIndexDatabase() as unknown as {
        prepare: (s: string) => { all: (...a: unknown[]) => unknown };
      };
      savedLinks = midb
        .prepare(
          "SELECT relativePath, fileId, originalMimeType, extractedText, description " +
            "FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath LIKE 'photos/%' " +
            'ORDER BY relativePath',
        )
        .all(vault.mountPointId);
    }
    // Blank the minted linkId in the OK body (the only nondeterministic field).
    if (body && typeof body === 'object' && 'linkId' in (body as Record<string, unknown>)) {
      (body as Record<string, unknown>).linkId = '<linkId>';
    }
    return { name: c.name, ...(error ? { error } : { body }), savedLinks };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

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

  const scratch = mkdtempSync(join(tmpdir(), 'qt-photo-upload-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const A = Buffer.from('upload-image-bytes-Alpha').toString('base64');
  const B = Buffer.from('upload-image-bytes-Bravo').toString('base64');
  const C = Buffer.from('upload-image-bytes-Charlie').toString('base64');
  const D = Buffer.from('upload-image-bytes-Delta').toString('base64');

  const cases: UploadCase[] = [
    // Dotless filename → the timestamped slug branch.
    {
      name: 'upload_dotless',
      data: A,
      filename: 'freshupload',
      mimeType: 'image/png',
      caption: 'A fine portrait',
      tags: ['airship', 'adventure'],
    },
    // Dotted filename → the sanitizeLeafName branch (case preserved).
    {
      name: 'upload_dotted',
      data: B,
      filename: 'Snapshot.PNG',
      mimeType: 'image/png',
      caption: null,
      tags: [],
    },
    // Dedup guard: the same bytes twice in the same vault → refusal.
    { name: 'upload_dedup', data: C, filename: 'dup.png', mimeType: 'image/png', twice: true },
    // 400 arms.
    { name: 'err_empty', data: '', filename: 'empty.png', mimeType: 'image/png' },
    { name: 'err_nonimage', data: D, filename: 'notes.txt', mimeType: 'text/plain' },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`photo-upload oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('character-photo-upload-tier2 oracle', async () => {
  await main();
});
