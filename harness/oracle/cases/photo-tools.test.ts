/**
 * @jest-environment node
 *
 * Differential ORACLE for the photo doc-edit tool handlers (W4.9b + P4.D108;
 * v4 `lib/tools/handlers/doc-edit/photo-handlers.ts` — keep_image /
 * list_images / attach_image / describe_image). Drives v4's REAL
 * `executeDocEditTool` + `formatDocEditResults` over the shared two-DB
 * fixture, ONE fresh copy per case (keep_image WRITES).
 *
 * P4.D108 adds two op kinds: `describe_image` handler ops (the auto-describe
 * MODULE is jest-mocked whole — v4's own mock level; `autoDescribe` picks the
 * canned behavior, incl. the race arm that writes the description first) and
 * `auto_describe` module ops (v4's REAL `autoDescribeChatImageAttachment`
 * runs with `generateImageDescription` mocked at the §C2 seam; `dumpModule`
 * dumps the module's three sink tables — files / doc_mount_file_links /
 * doc_mount_chunks).
 *
 * Model boundary pinned: `generateEmbeddingForUser` is jest.mocked to the corpus
 * canned vectors (keyed by exact query text; failures in cannedFailures throw an
 * EmbeddingError → the semantic branch's silent fallback to plain listing). Date
 * is frozen per KEEP op to the op's `keptAt` so saveImageToAlbum's minted
 * `keptAt = new Date().toISOString()` is pinned (list/attach don't mint dates).
 *
 * Side-effect seams (mirroring the Rust NoSideEffects): the two mount-index cache /
 * embedding modules saveImageToAlbum touches are jest.mocked to no-ops
 * (`mount-chunk-cache.invalidateMountPoint`, `embedding-scheduler.enqueueEmbeddingJobsForMountPoint`).
 * saveImageToAlbum runs the REAL `chunkAndInsertExtractedText` (deterministic DB
 * chunking, no model) + linkBlobContent, so the store bytes/rows are real; it does
 * NOT need `QUILLTAP_JOB_CHILD` (leave it UNSET — setting it breaks initializeDatabase
 * and reroutes writes through the forked-child proxy; see [[document-store-oracle-gotchas]]).
 *
 * Real-DB-under-jest (memory-gate-tier3 recipe): resetModules + doMock past
 * jest.setup's global DB mocks + better-sqlite3 -> better-sqlite3-multiple-ciphers.
 *
 * For KEEP cases, dump SIX tables so the Rust harness can diff in the
 * shared-id-map remap form: doc_mount_points / doc_mount_files / doc_mount_blobs /
 * doc_mount_file_links / doc_mount_folders (mount-index) + `files` (main). The photo
 * BYTES land in doc_mount_blobs (via linkBlobContent), NOT doc_mount_documents; the
 * markdown sidecar is the LINK's `extractedText` column. Order by remap-invariant
 * keys (content-derived sha / stable path) so the positional-UUID remap aligns.
 * list/attach cases mint nothing (attach reads only; list reads only) → no dump.
 *
 * Emits one NDJSON line per op: `{ label, resultJson: JSON.stringify(rawResult),
 * formatted: formatDocEditResults(tool, rawResult) }`; keep ops also carry
 * `{ dumps: { doc_mount_points, doc_mount_files, doc_mount_blobs,
 * doc_mount_file_links, doc_mount_folders, files } }`.
 *
 * Run (Node 24, from the v4 checkout; stage the case OUTSIDE any .claude path — v4's
 * jest config ignores /\.claude/ — see [[doc-edit-tool-differential-recipe]]).
 * Multi-case jest oracle → pass --testTimeout=120000 (see [[oracle-jest-testtimeout]]):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5 ; TMPO=/tmp/qt-oracle-run
 *   cd ~/source/quilltap-server
 *   # 1. build the fixture
 *   QT_FIXTURE_PHOTO_MAIN=/tmp/qt-photo-main.db QT_FIXTURE_PHOTO_MOUNT=/tmp/qt-photo-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-photo-tools-fixture.ts
 *   # 2. stage the case + spec OUTSIDE .claude, then run jest
 *   mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5/harness/oracle/cases/photo-tools.test.ts "$TMPO/cases/"
 *   cp $V5/harness/oracle/fixtures/photo-tools.json  "$TMPO/fixtures/"
 *   QT_FIXTURE_PHOTO_MAIN=/tmp/qt-photo-main.db QT_FIXTURE_PHOTO_MOUNT=/tmp/qt-photo-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-photo-tools.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- photo-tools
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// Inlined canonicalizer (jest can't resolve the `.js` ESM specifier of
// ../lib/tier2.ts). BLOBs -> lowercase hex, nulls explicit, everything else as-is,
// rows sorted by `orderBy` (code-unit string order on the canonicalized cell so it
// matches the Rust dump order). Mirrors harness/oracle/lib/tier2.ts.
function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy: string;
}): { table: string; columns: string[]; rows: Array<Record<string, unknown>> } {
  const { table, columns, rawRows, orderBy } = opts;
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface Meta {
  charAVault: string;
  charBVault: string;
  realFileIdByKey: Record<string, string>;
  realBytesByKey: Record<string, number[]>;
  realShaByKey: Record<string, string>;
  bakedByKey: Record<string, { vault: 'A' | 'B'; relativePath: string; linkId: string; keptAt: string }>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  charAId: string;
  charBId: string;
  chatId: string;
  chatMalformedId: string;
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'photo-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_PHOTO_MAIN;
  const mountFixture = process.env.QT_FIXTURE_PHOTO_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_PHOTO_MAIN and QT_FIXTURE_PHOTO_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const meta = JSON.parse(fs.readFileSync(mainFixture + '.meta.json', 'utf8')) as Meta;

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const fid = (key: string): string => meta.realFileIdByKey[key];
  // fileId -> stored bytes (for the readImageBuffer override): the STORED bytes the
  // fixture recorded, so a keep hashes the SAME bytes the baked photo did.
  const bytesByFileId: Record<string, number[]> = {};
  for (const [key, id] of Object.entries(meta.realFileIdByKey)) {
    bytesByFileId[id] = meta.realBytesByKey[key];
  }

  interface Op {
    label: string;
    tool: 'keep_image' | 'list_images' | 'attach_image' | 'describe_image' | 'auto_describe';
    /** 'A' | 'B'; the acting character. */
    who: 'A' | 'B';
    /** Override chatId (the malformed-scene chat); defaults to spec.chatId. */
    chatId?: string;
    args: Record<string, unknown>;
    /** For a keep op: freeze Date to this ISO so the minted keptAt is pinned.
     * Module (`auto_describe`) ops reuse it to pin descriptionUpdatedAt etc. */
    keptAt?: string;
    /** True for the keep ops → dump the six tables. */
    dump?: boolean;
    /** describe_image ops: how to mock `@/lib/photos/auto-describe-attachment`
     * (v4's OWN mock level for the handler — the module is mocked whole).
     * 'throws' (default) proves tiers 1/2 never reach vision; 'vision' returns
     * a canned description; 'race' writes the description first (the winner)
     * then answers already-described; 'failed' answers describe-failed. */
    autoDescribe?: 'throws' | 'vision' | 'race' | 'failed';
    /** auto_describe (module-level) ops: how to mock
     * `generateImageDescription` (the §C2 seam — everything above it runs
     * REAL). 'description' answers a canned description (with surrounding
     * whitespace, pinning the trim); 'unsupported' answers v4's no-profile
     * result; 'throws' proves the precheck short-circuits never reach it. */
    moduleMock?: 'description' | 'throws' | 'unsupported';
    /** auto_describe ops: dump files + doc_mount_file_links + doc_mount_chunks
     * (the module's three sinks). */
    dumpModule?: boolean;
  }

  const CANNED_VISION_DESCRIPTION = 'A copper kettle steams on a windowsill at dusk.';
  const RACE_WINNER_DESCRIPTION = 'The winner wrote this description first.';
  const MODULE_RAW_DESCRIPTION = '  A described mystery: two moths circle a lantern.  ';

  const ops: Op[] = [
    // ---- keep_image (write; dump the six tables; Date frozen to keptAt) ----
    {
      label: 'keep_fresh',
      tool: 'keep_image',
      who: 'A',
      chatId: spec.chatId,
      args: { uuid: fid('fresh'), caption: 'a fresh keep', tags: ['test'] },
      keptAt: '2026-04-01T12:00:00.000Z',
      dump: true,
    },
    {
      label: 'keep_duplicate',
      tool: 'keep_image',
      who: 'A',
      chatId: spec.chatId,
      args: { uuid: fid('sunset') },
      keptAt: '2026-04-01T12:30:00.000Z',
    },
    {
      label: 'keep_malformed_scene',
      tool: 'keep_image',
      who: 'A',
      chatId: spec.chatMalformedId,
      args: { uuid: fid('fresh') },
      keptAt: '2026-04-02T12:00:00.000Z',
      dump: true,
    },
    // ---- list_images (read-only) ----
    { label: 'list_plain', tool: 'list_images', who: 'A', args: {} },
    { label: 'list_plain_tag', tool: 'list_images', who: 'A', args: { tags: ['sea'] } },
    { label: 'list_plain_savedby', tool: 'list_images', who: 'A', args: { saved_by: 'Aurora' } },
    { label: 'list_plain_pagination', tool: 'list_images', who: 'A', args: { limit: 1, offset: 1 } },
    { label: 'list_semantic', tool: 'list_images', who: 'A', args: { query: 'sea sunset lighthouse' } },
    { label: 'list_semantic_peer', tool: 'list_images', who: 'A', args: { query: 'ancient library candles' } },
    { label: 'list_semantic_fail', tool: 'list_images', who: 'A', args: { query: 'please make the embedding fail' } },
    // ---- attach_image (read-only) ----
    { label: 'attach_by_linkid', tool: 'attach_image', who: 'A', args: { uuid: meta.bakedByKey.sunset.linkId } },
    { label: 'attach_by_fileid', tool: 'attach_image', who: 'A', args: { uuid: fid('sunset') } },
    { label: 'attach_cross_vault', tool: 'attach_image', who: 'A', args: { uuid: meta.bakedByKey.peer.linkId } },
    {
      label: 'attach_missing',
      tool: 'attach_image',
      who: 'A',
      args: { uuid: '99999999-9999-4999-8999-999999999999' },
    },
    // ---- describe_image (P4.D108; v4 a14a1811 bug 92 — the looking verb) ----
    // Tier 1: a stored description is served WITHOUT a vision call ('described'
    // also carries BOTH generation prompts, so a tier reorder is observable).
    { label: 'describe_stored', tool: 'describe_image', who: 'A', args: { uuid: fid('described') } },
    // Tier 2: revisedPrompt beats prompt; 'fresh' is UNKEPT (no album link) —
    // the no-album-membership rule's pin.
    { label: 'describe_generation_revised', tool: 'describe_image', who: 'A', args: { uuid: fid('fresh') } },
    // Tier 2 fallback: an empty revisedPrompt falls to generationPrompt.
    { label: 'describe_generation_prompt_only', tool: 'describe_image', who: 'A', args: { uuid: fid('promptonly') } },
    // An album link uuid resolves via sha256 to the image-v2 sister.
    { label: 'describe_album_link', tool: 'describe_image', who: 'A', args: { uuid: meta.bakedByKey.sunset.linkId } },
    // Tier 3: nothing on file → the vision call (module mocked whole, v4's level).
    { label: 'describe_vision', tool: 'describe_image', who: 'A', args: { uuid: fid('blank') }, autoDescribe: 'vision' },
    // The race arm: another writer described it between our read and the call.
    { label: 'describe_race', tool: 'describe_image', who: 'A', args: { uuid: fid('blank') }, autoDescribe: 'race' },
    // The vision describer produced nothing.
    { label: 'describe_failed', tool: 'describe_image', who: 'A', args: { uuid: fid('blank') }, autoDescribe: 'failed' },
    // Unresolvable uuid → the No-image sentence.
    { label: 'describe_missing', tool: 'describe_image', who: 'A', args: { uuid: '99999999-9999-4999-8999-999999999999' } },
    // A non-image FileEntry → its sentence.
    { label: 'describe_not_image', tool: 'describe_image', who: 'A', args: { uuid: fid('plaintext') } },
    // ---- autoDescribeChatImageAttachment (module level; generateImageDescription
    // mocked at the §C2 seam, everything above it REAL) ----
    // The three sinks: files.description + the blank uploads link + chunks.
    { label: 'autodescribe_writes', tool: 'auto_describe', who: 'A', args: { uuid: fid('blank') }, moduleMock: 'description', keptAt: '2026-04-03T09:00:00.000Z', dumpModule: true },
    // A baked image: the kept vault link (markdown extractedText) is untouched;
    // only the blank uploads link takes the description.
    { label: 'autodescribe_kept', tool: 'auto_describe', who: 'A', args: { uuid: fid('forest') }, moduleMock: 'description', keptAt: '2026-04-03T09:30:00.000Z', dumpModule: true },
    // already-described short-circuits BEFORE the vision call.
    { label: 'autodescribe_already', tool: 'auto_describe', who: 'A', args: { uuid: fid('described') }, moduleMock: 'throws' },
    // A non-description result → describe-failed, nothing persisted.
    { label: 'autodescribe_failed', tool: 'auto_describe', who: 'A', args: { uuid: fid('blank') }, moduleMock: 'unsupported', keptAt: '2026-04-03T10:00:00.000Z', dumpModule: true },
    // not-found / not-image.
    { label: 'autodescribe_notfound', tool: 'auto_describe', who: 'A', args: { uuid: '88888888-8888-4888-8888-888888888888' }, moduleMock: 'throws' },
    { label: 'autodescribe_notimage', tool: 'auto_describe', who: 'A', args: { uuid: fid('plaintext') }, moduleMock: 'throws' },
  ];

  const lines: string[] = [];
  const RealDate = Date;

  for (const op of ops) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-photo-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'photo-main.db');
    const mountWork = join(scratch, 'photo-mount.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    // jest.setup mocks the character-vault bridge to a fixed { mountPointId:
    // 'mock-vault-mount' } — un-mock it so getCharacterVaultStore resolves the REAL
    // minted vault via the real characters/docMountPoints repos ([[jest-real-db-oracle]]).
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );
    // jest.setup mocks fileStorageManager.downloadFile to a FIXED buffer, so
    // saveImageToAlbum's readImageBuffer would return the same bytes for every
    // image — the dedup guard (per-mount photos/ sha) would then never distinguish
    // the images. Override images-v2's readImageBuffer to return the FIXTURE-recorded
    // STORED bytes per fileId (what was baked), so a keep of an already-baked image
    // hashes the SAME bytes and collides (ALREADY_SAVED). The Rust canned
    // FileBytesStore returns the SAME per-fileId bytes. getImageById /
    // ingestImageBuffer stay REAL (the keep path resolves the FileEntry through them).
    jest.doMock('@/lib/images-v2', () => {
      const actual = jest.requireActual('@/lib/images-v2');
      return {
        __esModule: true,
        ...actual,
        readImageBuffer: async (fileId: string) => {
          const arr = bytesByFileId[fileId];
          if (!arr) throw new Error(`no canned bytes for fileId ${fileId}`);
          return Buffer.from(arr);
        },
      };
    });
    // Side-effect seams (mount-index cache invalidation + embedding enqueue) → no-op.
    jest.doMock('@/lib/mount-index/mount-chunk-cache', () => {
      const actual = jest.requireActual('@/lib/mount-index/mount-chunk-cache');
      return { ...actual, invalidateMountPoint: () => {} };
    });
    jest.doMock('@/lib/mount-index/embedding-scheduler', () => {
      const actual = jest.requireActual('@/lib/mount-index/embedding-scheduler');
      return { ...actual, enqueueEmbeddingJobsForMountPoint: async () => 0 };
    });
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      const { EmbeddingError } = actual;
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string) => {
          if (spec.cannedFailures.includes(text)) {
            throw new EmbeddingError(`canned failure for input (${text.length} chars)`);
          }
          const vec = spec.cannedEmbeddings[text];
          if (!vec) {
            throw new EmbeddingError(`no canned embedding registered for input (${text.length} chars)`);
          }
          return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
        },
      };
    });

    // describe_image ops: mock the auto-describe MODULE whole (v4's own mock
    // level for the handler differential — the Rust side cans the same seam).
    // ⚠ `jest.doMock` registrations SURVIVE `jest.resetModules()`, so every
    // iteration must (re-)register BOTH modules explicitly — a stale factory
    // from a previous op would otherwise leak into this one.
    if (op.tool !== 'describe_image') {
      jest.doMock('@/lib/photos/auto-describe-attachment', () =>
        jest.requireActual('@/lib/photos/auto-describe-attachment'),
      );
    }
    if (op.tool !== 'auto_describe') {
      jest.doMock('@/lib/chat/file-attachment-fallback', () =>
        jest.requireActual('@/lib/chat/file-attachment-fallback'),
      );
    }
    if (op.tool === 'describe_image') {
      const mode = op.autoDescribe ?? 'throws';
      jest.doMock('@/lib/photos/auto-describe-attachment', () => ({
        __esModule: true,
        autoDescribeChatImageAttachment: async (input: {
          fileEntryId: string;
          userId: string;
          repos: { files: { update: (id: string, patch: unknown) => Promise<unknown> } };
        }) => {
          if (mode === 'throws') {
            throw new Error('auto-describe must not be called for this op');
          }
          if (mode === 'vision') {
            return { describedFileEntry: true, linksUpdated: 1, description: CANNED_VISION_DESCRIPTION };
          }
          if (mode === 'race') {
            // The winner: persist the description, then answer already-described.
            await input.repos.files.update(input.fileEntryId, { description: RACE_WINNER_DESCRIPTION });
            return { describedFileEntry: false, linksUpdated: 0, description: null, skipReason: 'already-described' };
          }
          return { describedFileEntry: false, linksUpdated: 0, description: null, skipReason: 'describe-failed' };
        },
      }));
    }
    // auto_describe (module-level) ops: mock generateImageDescription at the
    // §C2 seam; the REAL module (precheck, bytes, persist, chunk pass) runs.
    if (op.tool === 'auto_describe') {
      const mode = op.moduleMock ?? 'description';
      jest.doMock('@/lib/chat/file-attachment-fallback', () => {
        const actual = jest.requireActual('@/lib/chat/file-attachment-fallback');
        return {
          __esModule: true,
          ...actual,
          generateImageDescription: async (file: { filename: string; mimeType: string }) => {
            if (mode === 'throws') {
              throw new Error('generateImageDescription must not be called for this op');
            }
            if (mode === 'unsupported') {
              return {
                type: 'unsupported',
                error: 'No image description profile available. Configure one in Settings → Chat Settings → Image Description Profile',
                processingMetadata: { originalFilename: file.filename, originalMimeType: file.mimeType },
              };
            }
            return {
              type: 'image_description',
              imageDescription: MODULE_RAW_DESCRIPTION,
              processingMetadata: {
                usedImageDescriptionLLM: true,
                originalFilename: file.filename,
                originalMimeType: file.mimeType,
              },
            };
          },
        };
      });
    }

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { executeDocEditTool, formatDocEditResults } = await import(
      '@/lib/tools/handlers/doc-edit-handler'
    );
    await initializeDatabase();

    // Freeze Date to the keep op's keptAt (saveImageToAlbum mints keptAt =
    // new Date().toISOString()). list/attach mint no dates → real clock.
    let dateFrozen = false;
    if (op.keptAt) {
      const fixedIso = op.keptAt;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      global.Date = class extends RealDate {
        constructor(...a: unknown[]) {
          if (a.length === 0) {
            super(fixedIso);
          } else {
            // @ts-expect-error forward variadic args to the real Date ctor
            super(...a);
          }
        }
        static now(): number {
          return RealDate.parse(fixedIso);
        }
      } as unknown as DateConstructor;
      dateFrozen = true;
    }

    try {
      const characterId = op.who === 'A' ? spec.charAId : spec.charBId;
      const context = {
        userId: spec.userId,
        chatId: op.chatId ?? spec.chatId,
        characterId,
      };
      let resultJson: string;
      let formatted: string;
      if (op.tool === 'auto_describe') {
        // Drive the REAL module (generateImageDescription mocked above).
        const { autoDescribeChatImageAttachment } = await import('@/lib/photos/auto-describe-attachment');
        const { getRepositories } = await import('@/lib/repositories/factory');
        const moduleResult = await autoDescribeChatImageAttachment({
          fileEntryId: String(op.args.uuid),
          userId: spec.userId,
          repos: getRepositories(),
        });
        resultJson = JSON.stringify(moduleResult);
        formatted = '';
      } else {
        const rawResult = await executeDocEditTool(op.tool, op.args, context as never);
        resultJson = JSON.stringify(rawResult);
        formatted = formatDocEditResults(op.tool, rawResult);
      }

      const record: Record<string, unknown> = { label: op.label, resultJson, formatted };

      if (op.dump || op.dumpModule) {
        const midb = getRawMountIndexDatabase();
        if (!midb) throw new Error('mount-index DB handle unavailable for dump');
        const dumpMount = (table: string, orderBy: string) => {
          const columns = (
            midb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
          ).map((c) => c.name);
          const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<
            Record<string, unknown>
          >;
          return canonicalizeRows({ table, columns, rawRows, orderBy });
        };
        const dumpMain = async (table: string, orderBy: string) => {
          const columns = (
            (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
          ).map((c) => c.name);
          const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<
            Record<string, unknown>
          >;
          return canonicalizeRows({ table, columns, rawRows, orderBy });
        };
        // Order by remap-invariant keys so the positional-UUID remap aligns both sides.
        if (op.dump) {
          record.dumps = {
            doc_mount_points: dumpMount('doc_mount_points', 'id'),
            doc_mount_files: dumpMount('doc_mount_files', 'sha256'),
            doc_mount_blobs: dumpMount('doc_mount_blobs', 'sha256'),
            doc_mount_file_links: dumpMount('doc_mount_file_links', 'relativePath'),
            doc_mount_folders: dumpMount('doc_mount_folders', 'path'),
            files: await dumpMain('files', 'id'),
          };
        } else {
          // Module ops: the auto-describe three-sink tables (chunks ordered by
          // content — remap-invariant; ids are minted).
          record.dumps = {
            files: await dumpMain('files', 'id'),
            doc_mount_file_links: dumpMount('doc_mount_file_links', 'relativePath'),
            doc_mount_chunks: dumpMount('doc_mount_chunks', 'content'),
          };
        }
      }

      lines.push(JSON.stringify(record));
    } finally {
      if (dateFrozen) global.Date = RealDate;
      // Let fire-and-forget promises settle before closing.
      await new Promise((resolve) => setTimeout(resolve, 50));
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`photo-tools oracle wrote ${outPath} (${lines.length} ops)\n`);
}

test('photo-tools oracle', async () => {
  await main();
});
