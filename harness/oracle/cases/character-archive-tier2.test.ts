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
 * The committed fixture is built by `build-character-archive-fixture.ts` and
 * then EXTENDED in place, twice: `extend-character-archive-twice-linked-blob.ts`
 * (v4 Bug 57) and `extend-character-archive-profile-and-avatars.ts` (the default
 * embedding profile, the avatar-override face, the standalone avatar
 * thumbnail). Each carries its own recipe; extend by mutation, never rebuild.
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
  // §3 review (archive-round-2): the re-embed half of rehydrate and the
  // import's one-EMBEDDING_GENERATE-per-restored-memory both write here —
  // without this table a port that never enqueued a job would pass.
  'background_jobs',
  // P4.D65-remainder: the fixture now carries a DEFAULT embedding profile (it
  // is what makes `background_jobs` a positive comparand rather than a
  // tripwire), and a bundle import is one of the few paths that can mint
  // profiles — so the table is dumped to pin that this one never does.
  'embedding_profiles',
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
  | 'rehydrate_missing_bundle_file'
  // ── P4.D65 resumed: the surfaces an archived character's TOMBSTONE changes ──
  // These three ride this fixture rather than the files/system families because
  // the material they need — a real ARCHIVE bundle and the character holding it
  // — exists only once `archiveCharacter` has actually run.
  | 'files_delete_bundle_held'
  | 'files_delete_bundle_force'
  | 'files_delete_bundle_unheld'
  | 'export_entities_after_archive'
  | 'export_all_after_archive'
  // ── P4.D65-remainder (order item 6): the §3 review's corpus-undriven arms ──
  | 'archive_key_unavailable'
  | 'archive_rehydrate_passphrase_mismatch'
  | 'archive_prune_incomplete'
  | 'rehydrate_pre_revision_avatar';

/**
 * The instance-passphrase seam, as a mutable box.
 *
 * v4 reads it through two module-level functions — `getRuntimePassphrase()`
 * (the passphrase this process last proved) and `getHasUserPassphrase()` (does
 * a USER passphrase protect this instance at all) — and `archive-crypto.ts`
 * binds both at import. The box lets a single case move the seam BETWEEN two
 * operations, which is the only way to reach the mismatch arm: archive under
 * one passphrase, rehydrate under another, exactly as a passphrase change
 * leaves an older bundle. v5 takes the same pair as an explicit
 * `PassphraseSource` argument, so the two sides stay driven by the same values.
 *
 * The default — `{cached: null, hasUser: false}` — is the fixture instance's
 * real state and what every pre-existing case runs on: `INTERNAL_PASSPHRASE`.
 */
const passphraseSeam: { cached: string | null; hasUser: boolean } = {
  cached: null,
  hasUser: false,
};

/** The two passphrases the mismatch arm archives and rehydrates under. */
const OLD_PASSPHRASE = 'the-passphrase-that-was-in-effect';
const NEW_PASSPHRASE = 'the-passphrase-in-effect-now';

/** A minimal NextRequest stand-in — the same shape the files/system oracles use. */
function mockRequest(url: string, method = 'GET', body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

async function runCase(
  spec: Spec,
  name: CaseName,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  // Every case starts on the fixture instance's REAL passphrase state; only the
  // two passphrase arms move it, and only after this reset.
  passphraseSeam.cached = null;
  passphraseSeam.hasUser = false;

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
  // ⚠ A STALE ORACLE MOCK, found by consequence (P4.20 / P4.36 class).
  // jest.setup stubs `getDefaultEmbeddingProfile` to resolve `null`, so the
  // import's `enqueueImportedMemoryEmbeddings` took its no-default-profile
  // branch NO MATTER WHAT the database held — v4 was being starved of a profile
  // that exists, warning about it in the rehydrate result, and enqueueing zero
  // memory jobs. The real function is four lines over the same repository the
  // mount-chunk scheduler already reads unmocked.
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  // The job PUMP is a host-cadence seam v5 does not port into the core
  // (`job-runner-fork-ipc-non-port`), and v4's `enqueueJob` kicks it via
  // `ensureProcessorRunning()`, which forks `child-entry.ts` — a process that
  // cannot even resolve its imports under jest. Neutralize the kick; the QUEUED
  // ROWS are the comparand, and letting them be picked up would race the dump.
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/processor'),
    ensureProcessorRunning: () => {},
  }));
  // The instance-passphrase seam (see `passphraseSeam`). Both modules keep all
  // their real exports — `INTERNAL_PASSPHRASE` in particular, which the
  // no-passphrase resolution returns — and swap only the two readers.
  jest.doMock('@/lib/startup/passphrase-cache', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/startup/passphrase-cache'),
    getRuntimePassphrase: () => passphraseSeam.cached,
  }));
  jest.doMock('@/lib/startup/dbkey', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/startup/dbkey'),
    getHasUserPassphrase: () => passphraseSeam.hasUser,
  }));
  // The route-driven arms go through `buildRequestContext`, which resolves the
  // operator from the session; jest.setup resolves it to `null` (→ a bare 500).
  // Same seam the files/system oracles neutralize.
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  // jest.setup's `startupState` stub omits the members the route context
  // consults (`areMigrationsComplete`), so fall through to the real object with
  // the readiness answers pinned — the files/system oracles' exact seam.
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
        areMigrationsComplete: () => true,
      },
    };
  });

  // The extender-minted ids, read from the committed sidecar rather than
  // transcribed: a re-pin in `extend-character-archive-profile-and-avatars.ts`
  // must not be able to leave the two differential sides driving different rows.
  const meta = JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as {
    avatarThumbnailFileId: string;
  };

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
  /** Non-null only where the sealing passphrase differs from the current one. */
  const sealPassphrase: string | null =
    name === 'archive_rehydrate_passphrase_mismatch' ? OLD_PASSPHRASE : null;
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

      // ── The files-delete ARCHIVE_BUNDLE_HELD guard (delete.ts:34–44) ──
      // A held bundle is the only copy of the pruned material, so deleting it
      // strands the tombstone forever. All three legs run against a bundle the
      // archive really wrote, so `characterId` in the refusal envelope is a
      // real holder rather than a planted row.
      case 'files_delete_bundle_held':
      case 'files_delete_bundle_force':
      case 'files_delete_bundle_unheld': {
        const archived = (await archiveCharacter(spec.userId, spec.sable)) as {
          archiveFileId: string | null;
        };
        const fileId = archived.archiveFileId ?? '';
        if (name === 'files_delete_bundle_unheld') {
          // Rehydrating clears `archiveFileId`, so the bundle survives as an
          // ordinary orphaned ARCHIVE file — nobody holds it and the guard must
          // stand aside. Without this leg the guard could refuse EVERY ARCHIVE
          // delete and still pass.
          await rehydrateCharacter(spec.userId, spec.sable);
        }
        const query = name === 'files_delete_bundle_force' ? '?force=true' : '';
        const route = await import('@/app/api/v1/files/[id]/route');
        payload.result = await respond(
          await (route as never as {
            DELETE: (r: unknown, c: unknown) => Promise<unknown>;
          }).DELETE(mockRequest(`http://localhost/api/v1/files/${fileId}${query}`, 'DELETE'), {
            params: Promise.resolve({ id: fileId }),
          }),
        );
        break;
      }

      // ── The export picker's archived filter (system/tools/route.ts:488) ──
      // An archived character must not be offered for export: the tombstone
      // would carry only the pruned vault, and the full bundle already exists.
      case 'export_entities_after_archive': {
        await archiveCharacter(spec.userId, spec.sable);
        const route = await import('@/app/api/v1/system/tools/route');
        payload.result = await respond(
          await (route as never as { GET: (r: unknown) => Promise<unknown> }).GET(
            mockRequest(
              'http://localhost/api/v1/system/tools?action=export-entities&type=characters',
            ),
          ),
        );
        break;
      }

      // ── The three-key export carry, NON-NULL leg (shared contract rule 4) ──
      // The `.qtap` writer does NOT filter archived characters (only the picker
      // does), so a deliberate scope:'all' export taken AFTER an archive is the
      // one place the three archive columns reach an export record carrying
      // real values. The absent-COLUMN leg is pinned by the old-schema
      // `system-data-*` family; this is the present-and-non-null leg.
      case 'export_all_after_archive': {
        await archiveCharacter(spec.userId, spec.sable);
        const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
        const stream = await createNdjsonStream(spec.userId, {
          type: 'characters',
          scope: 'all',
          includeMemories: false,
          preserveIds: false,
        } as never);
        const chunks: Uint8Array[] = [];
        const reader = (stream as ReadableStream<Uint8Array>).getReader();
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          if (value) chunks.push(value);
        }
        const text = Buffer.concat(chunks.map((c) => Buffer.from(c))).toString('utf-8');
        // Only the character records matter here; the manifest's clock and the
        // rest of the envelope are already byte-proven by the export family.
        payload.result = {
          characters: text
            .split('\n')
            .filter((l) => l.trim().length > 0)
            .map((l) => JSON.parse(l) as { kind?: string; data?: unknown })
            .filter((r) => r.kind === 'character')
            .map((r) => r.data),
        };
        break;
      }

      // ── The passphrase-unavailable refusal (§4.2c) ──
      // A USER passphrase protects the instance and this process has never seen
      // it. `archiveCharacter` resolves the passphrase BEFORE anything is
      // written, so the state dump doubles as the proof that a refused archive
      // leaves no bundle, no tombstone and no flipped seat behind.
      case 'archive_key_unavailable':
        passphraseSeam.hasUser = true;
        passphraseSeam.cached = null;
        payload.result = await capture(() => archiveCharacter(spec.userId, spec.sable));
        break;

      // ── The passphrase-CHANGE diagnosis ──
      // Seal under the old passphrase, then rehydrate with the new one in the
      // cache — precisely what a passphrase change leaves behind for a bundle
      // written before it. The header's `keyHash` is what turns this into a
      // named "this archive predates your passphrase change" refusal instead of
      // a bare GCM authentication failure, and the character must stay archived.
      case 'archive_rehydrate_passphrase_mismatch':
        passphraseSeam.hasUser = true;
        passphraseSeam.cached = OLD_PASSPHRASE;
        await archiveCharacter(spec.userId, spec.sable);
        passphraseSeam.cached = NEW_PASSPHRASE;
        payload.result = await capture(() => rehydrateCharacter(spec.userId, spec.sable));
        break;

      // ── `pruneComplete: false` ──
      // The prune is the one phase that may fail without failing the archive:
      // the tombstone is already committed and the bundle already holds
      // everything, so an incomplete prune is reported, logged, and left for a
      // re-run. Reaching it needs a link delete that fails without throwing out
      // of the repository — which is exactly what `deleteWithGC`'s swallow does
      // — so the case plants a BEFORE DELETE trigger that aborts every link
      // delete in the vault. Nothing is deleted on either side; v4 falls through
      // to its undead-links honesty check and v5 to its propagated error, and
      // BOTH report `pruneComplete: false` over an untouched vault.
      case 'archive_prune_incomplete': {
        const midbTrigger = getRawMountIndexDatabase();
        if (!midbTrigger) throw new Error('mount index database unavailable');
        midbTrigger.exec(
          "CREATE TRIGGER qt_block_link_delete BEFORE DELETE ON doc_mount_file_links " +
            "BEGIN SELECT RAISE(ABORT, 'planted: vault link deletes are blocked'); END;",
        );
        payload.result = await capture(() => archiveCharacter(spec.userId, spec.sable));
        break;
      }

      // ── The pre-revision tombstone's avatar thumbnail (§6 step 4) ──
      // `archivedAvatarFileId` is vestigial: the shipped service never writes
      // it, so only a tombstone left by the pre-§4.2a revision carries one. The
      // raw UPDATE is deliberate — the §4.4 guard sanctions exactly one patch on
      // an archived row, so the repository would refuse this. Rehydrate must
      // clear the key in its FOLLOW-UP patch (never in the sanctioned
      // `{archivedAt: null}` one) and delete the standalone thumbnail row the
      // fixture's extender planted.
      case 'rehydrate_pre_revision_avatar': {
        await archiveCharacter(spec.userId, spec.sable);
        await rawQuery('UPDATE characters SET archivedAvatarFileId = ? WHERE id = ?', [
          meta.avatarThumbnailFileId,
          spec.sable,
        ]);
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
      // The comparand is what the bundle SAYS, so it is read back under the
      // passphrase that sealed it. That is the ambient resolution for every case
      // but the mismatch arm, whose whole point is that the current passphrase
      // is no longer the sealing one — reading it back with the ambient one
      // would only re-prove the refusal the `result` comparand already carries.
      const plaintext = isEncryptedArchive(raw)
        ? decryptArchive(raw, sealPassphrase ?? undefined)
        : raw;
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
    'files_delete_bundle_held',
    'files_delete_bundle_force',
    'files_delete_bundle_unheld',
    'export_entities_after_archive',
    'export_all_after_archive',
    'archive_key_unavailable',
    'archive_rehydrate_passphrase_mismatch',
    'archive_prune_incomplete',
    'rehydrate_pre_revision_avatar',
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
