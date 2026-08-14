/**
 * @jest-environment node
 *
 * P4.9G4 `.qtap` IMPORT-EXECUTE ORACLE (tier 2): drives v4's REAL
 * `executeImport` (`lib/import/quilltap-import/execute.ts:41`) — and, for the
 * route arms, v4's REAL `POST /api/v1/system/tools?action=import-execute`
 * handler — over a FRESH copy of the committed `system-data-*` fixture family
 * per case, then dumps EVERY table in all three partitions.
 *
 * ── THE PAYLOADS ARE REAL EXPORTS ────────────────────────────────────────────
 * The merged payload is built by running v4's own `createNdjsonStream` over the
 * fixture for each of the fifteen export types (`7189a968`) and merging the assembled `data`
 * objects (memories ride the characters export via includeMemories). The emitted
 * `exportData` is exactly what the Rust differential replays, so both sides
 * provably import identical input.
 *
 * ── WHAT IS EMITTED PER CASE ─────────────────────────────────────────────────
 *   kind 'execute': exportData + options + result (the ImportResult) +
 *                   preState/state (row dumps of main/mountIndex/llmLogs)
 *   kind 'route'  : the request body + {status, body} — and, for the arms that
 *                   import, state dumps too
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysimportexec-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/system-import-execute.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-import-execute.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-import-execute
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
}

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // [P4.D46] jest.setup.ts globally mocks these three — with
  // `getDefaultEmbeddingProfile: () => null`, a canned uploads bridge, and a
  // canned storage manager — which made this oracle LIE about v4's `7189a968`
  // behavior: every import answered the no-profile warning and enqueued
  // nothing (the P4.20/P4.36 stale-mock class, caught here because the
  // fixture HAS a default profile and the first regen still bailed out).
  // The real modules must drive: the embedding enqueue, the file importers'
  // uploads/project bridges, and `streamFiles`' byte reads.
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  // The registry is EMPTY under jest (loadPlugins never runs), but production
  // has every bundled plugin registered — and the export's plugin-config
  // redaction resolves manifests through it. Serve the REAL manifest.json
  // straight from plugins/dist; anything else resolves null (the
  // withhold-everything arm).
  jest.doMock('@/lib/plugins/registry', () => {
    const actual = jest.requireActual('@/lib/plugins/registry');
    const rfs = require('node:fs');
    const rpath = require('node:path');
    return {
      __esModule: true,
      ...actual,
      getPlugin: (name: string) => {
        const manifestPath = rpath.join(process.cwd(), 'plugins', 'dist', name, 'manifest.json');
        if (!rfs.existsSync(manifestPath)) return null;
        return { manifest: JSON.parse(rfs.readFileSync(manifestPath, 'utf8')) };
      },
    };
  });
  // …and with the real queue in play, `enqueueJob` calls
  // `ensureProcessorRunning()` and the dispatcher starts CLAIMING the freshly
  // enqueued rows mid-dump (PENDING → PROCESSING raced the state snapshot on
  // the first regen). Still the wake — a job row's terminal state here is
  // PENDING, exactly what the production enqueue leaves before the pump runs.
  // (The `QUILLTAP_JOB_CHILD=1` pin is NOT usable for this: it flips the
  // whole sqlite client readonly, by design.)
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/processor'),
    ensureProcessorRunning: () => {},
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
  }));
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
      },
    };
  });
}

/**
 * Drain pending fire-and-forget tails (v4's best-effort `refreshStats` after a
 * bridge write, etc.) before a state dump. Without this the dump catches v4
 * mid-air — the first files-case regen showed the project store's rollups
 * un-refreshed purely because its write was the LAST one before the dump —
 * and, per the standing fire-and-forget rule, a stray promise would otherwise
 * poison the NEXT case. Production settles milliseconds later; the settled
 * state is the one worth pinning.
 */
async function settle(): Promise<void> {
  for (let i = 0; i < 25; i++) await new Promise((r) => setImmediate(r));
}

/** Drain a `ReadableStream<Uint8Array>` into its raw bytes. */
async function drainBytes(stream: ReadableStream<Uint8Array>): Promise<Buffer> {
  const reader = stream.getReader();
  const chunks: Buffer[] = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (value) chunks.push(Buffer.from(value));
  }
  return Buffer.concat(chunks);
}

function streamOf(bytes: Buffer): ReadableStream<Uint8Array> {
  return new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array(bytes));
      controller.close();
    },
  });
}

/** v4 `loadQtapFromUpload` reproduced over a buffer. */
async function loadQtap(bytes: Buffer): Promise<{ manifest: unknown; data: Record<string, unknown[]> }> {
  const { peekFormat, readNdjsonLines } = await import('@/lib/import/ndjson-reader');
  const { assembleExportFromStream } = await import('@/lib/import/quilltap-import-stream');
  const { format, stream } = await peekFormat(streamOf(bytes));
  if (format !== 'ndjson') throw new Error('expected an NDJSON export');
  return (await assembleExportFromStream(readNdjsonLines(stream))) as never;
}

/** Every table in one partition, in rowid (insertion) order; BLOBs → sha256. */
function dumpPartition(db: import('better-sqlite3').Database): Record<string, unknown> {
  const tables = (
    db
      .prepare(
        `SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name`,
      )
      .all() as Array<{ name: string }>
  ).map((r) => r.name);
  const out: Record<string, unknown> = {};
  for (const t of tables) {
    const rows = db.prepare(`SELECT * FROM "${t}"`).all() as Array<Record<string, unknown>>;
    out[t] = rows.map((row) => {
      const o: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(row)) {
        o[k] = Buffer.isBuffer(v)
          ? `sha256:${createHash('sha256').update(v).digest('hex')}`
          : (v ?? null);
      }
      return o;
    });
  }
  return out;
}

/**
 * Build the merged all-type payload from v4's OWN exports over the live fixture.
 * One export per type, `scope: 'all'`; memories ride the characters export.
 */
/**
 * [P4.D62] A `characters` export on its own — which, since `01e481f6`, carries
 * each character's whole VAULT (mount point + folders + documents + blobs).
 * That is what the `preserveIds` arms need: a bundle whose ids reach every
 * table the archive/rehydrate path claims.
 */
async function buildCharactersExport(
  userId: string,
  selectedIds?: string[],
): Promise<{ manifest: unknown; data: Record<string, unknown[]> }> {
  const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
  const bytes = await drainBytes(
    createNdjsonStream(userId, {
      type: 'characters',
      scope: selectedIds ? 'selected' : 'all',
      selectedIds,
      includeMemories: true,
      preserveIds: true,
    } as never),
  );
  return loadQtap(bytes);
}

// ── The fixture ids the hand-built preserveIds payloads lean on ─────────────
// Lorian's vault, and two CONTENT rows that live OUTSIDE it. Bug 54's whole
// point is that a content row can be present with the right bytes while having
// no link in the rehydrate target's vault — so these two are what make the
// sha-first classifier discriminating rather than shadowed by the
// link-in-target fallback.
const LORIAN_VAULT = 'c5e2a72d-b5f1-4486-8325-8308f783c90e';
/** `doc_mount_files` row linked ONLY in Riya's vault, with its true hash. */
const RIYA_ONLY_CONTENT = '06d2d56a-d8d4-46ba-953b-d13b7141f7bf';
const RIYA_ONLY_CONTENT_SHA =
  'd3a87080d6e3ee8b060ccf3ccbffe0dd0aa029e6d219c436fc96f5c2231ef7bf';
/** `doc_mount_blobs` row in the Uploads mount, with its true hash. */
const UPLOADS_BLOB = 'b585b1ed-f355-4c48-b597-9ef47812ef8e';
const UPLOADS_BLOB_SHA =
  '1065ee92208d16fbd02af1041248cb76ee38cdb68c7c2d89e4facac052b41d05';
/** [P4.D65] The fixture's only roleplay template — `House Style`. */
const FIXTURE_TEMPLATE_ID = 'a4000000-0000-4000-8000-000000000001';

/**
 * [Bug 54] A bundle claiming a content row and a blob row that BOTH already
 * exist outside the rehydrate target's vault. With `matching` the carried
 * hashes are the live ones — dedup, so skip-if-present sanctions them and the
 * import proceeds; with `matching: false` they are a same-id/different-bytes
 * clash and the whole import refuses.
 *
 * `linkId` / the blob's `fileId` are deliberately ids nothing owns, so the
 * link and content checks pass on absence and the ONLY thing under test is the
 * sha-first classifier.
 */
function sharedContentPayload(matching: boolean): { manifest: unknown; data: Record<string, unknown> } {
  const wrong = '0000000000000000000000000000000000000000000000000000000000000000';
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'document-stores',
      createdAt: '2026-03-01T00:00:00.000Z',
      appVersion: 'test',
      settings: { includeMemories: false, scope: 'all', selectedIds: [], preserveIds: true },
      counts: {},
    },
    data: {
      mountPoints: [],
      folders: [],
      documents: [
        {
          mountPointId: LORIAN_VAULT,
          relativePath: 'notes/shared.md',
          fileName: 'shared.md',
          fileType: 'markdown',
          content: 'shared',
          contentSha256: matching ? RIYA_ONLY_CONTENT_SHA : wrong,
          plainTextLength: 6,
          lastModified: '2026-03-01T00:00:00.000Z',
          folderId: null,
          fileId: RIYA_ONLY_CONTENT,
          linkId: 'ca000000-0000-4000-8000-000000000001',
          linkGroupId: null,
        },
      ],
      blobs: [
        {
          mountPointId: LORIAN_VAULT,
          relativePath: 'images/shared.bin',
          originalFileName: 'shared.bin',
          originalMimeType: 'application/octet-stream',
          storedMimeType: 'application/octet-stream',
          sizeBytes: 3,
          sha256: matching ? UPLOADS_BLOB_SHA : wrong,
          description: '',
          descriptionUpdatedAt: null,
          fileId: 'ca000000-0000-4000-8000-000000000002',
          linkId: 'ca000000-0000-4000-8000-000000000003',
          blobId: UPLOADS_BLOB,
          extractedText: null,
          extractedTextSha256: null,
          extractionStatus: 'none',
          extractionError: null,
          dataBase64: Buffer.from('abc').toString('base64'),
        },
      ],
      projectLinks: [],
    },
  };
}

/**
 * [WP B1 / F4] A hand-built bundle whose every id is FREE — one character with
 * a memory and a whole vault (mount point, a folder and a NESTED folder, a text
 * document, a blob). Under `preserveIds` every one of those rows must land at
 * the carried id, and reconciliation must repoint the character at the imported
 * vault and tear down the scaffold `create()` provisioned.
 *
 * Hand-built rather than taken from the fixture's own characters export,
 * because that export's five managed-field links all share ONE empty content
 * row which also carries a blob — so the assembled bundle claims the same
 * `linkId` under both `documents` and `blobs`, and the preflight's
 * repeat-detection refuses it before any of this is reached. (That refusal IS
 * v4's behaviour and is pinned by `execute_preserve_ids_refuse_existing`; it is
 * simply not what these arms are for.)
 *
 * Re-imported a second time under `skip-if-present` (see the case list), where
 * every id now exists INSIDE the target and must be skipped rather than refused.
 */
function preserveIdsVaultPayload(): { manifest: unknown; data: Record<string, unknown> } {
  const CHAR = 'cc000000-0000-4000-8000-000000000001';
  const VAULT = 'cc000000-0000-4000-8000-000000000002';
  const FOLDER = 'cc000000-0000-4000-8000-000000000003';
  const NESTED = 'cc000000-0000-4000-8000-000000000004';
  const DOC_FILE = 'cc000000-0000-4000-8000-000000000005';
  const DOC_LINK = 'cc000000-0000-4000-8000-000000000006';
  const BLOB_FILE = 'cc000000-0000-4000-8000-000000000007';
  const BLOB_LINK = 'cc000000-0000-4000-8000-000000000008';
  const BLOB = 'cc000000-0000-4000-8000-000000000009';
  const MEMORY = 'cc000000-0000-4000-8000-00000000000a';
  const PROPS_FILE = 'cc000000-0000-4000-8000-00000000000b';
  const PROPS_LINK = 'cc000000-0000-4000-8000-00000000000c';
  const content = '# Ancients\n\nThe first age.\n';
  // A character vault is only usable with its managed-field sidecar; a real
  // export always carries one, and without it the repoint below lands the
  // character on a store the read overlay refuses ("properties.json missing").
  const properties = JSON.stringify(
    {
      pronouns: null,
      aliases: [],
      title: null,
      firstMessage: null,
      talkativeness: 0.5,
      canChooseOutfit: false,
    },
    null,
    2,
  );
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'characters',
      createdAt: '2026-03-01T00:00:00.000Z',
      appVersion: 'test',
      settings: { includeMemories: true, scope: 'all', selectedIds: [], preserveIds: true },
      counts: {},
    },
    data: {
      characters: [
        {
          id: CHAR,
          name: 'Ulugh Beg',
          identity: 'An astronomer.',
          description: 'Patient.',
          personality: 'Exacting.',
          characterDocumentMountPointId: VAULT,
          avatarOverrides: [],
          tags: [],
        },
      ],
      memories: [
        {
          id: MEMORY,
          characterId: CHAR,
          content: 'The tables were finished at Samarkand.',
          summary: 'the tables',
          importance: 0.7,
          source: 'AUTO',
          keywords: [],
          tags: [],
        },
      ],
      mountPoints: [
        {
          id: VAULT,
          name: 'Ulugh Beg Character Vault',
          basePath: '',
          mountType: 'database',
          storeType: 'character',
          includePatterns: [],
          excludePatterns: [],
          enabled: true,
        },
      ],
      folders: [
        { id: FOLDER, mountPointId: VAULT, parentId: null, name: 'lore', path: 'lore' },
        {
          id: NESTED,
          mountPointId: VAULT,
          parentId: FOLDER,
          name: 'ancients',
          path: 'lore/ancients',
        },
      ],
      documents: [
        {
          mountPointId: VAULT,
          relativePath: 'properties.json',
          fileName: 'properties.json',
          fileType: 'json',
          content: properties,
          contentSha256: createHash('sha256').update(properties, 'utf8').digest('hex'),
          plainTextLength: properties.length,
          lastModified: '2026-03-01T00:00:00.000Z',
          folderId: null,
          fileId: PROPS_FILE,
          linkId: PROPS_LINK,
          linkGroupId: null,
        },
        {
          mountPointId: VAULT,
          relativePath: 'lore/ancients/first-age.md',
          fileName: 'first-age.md',
          fileType: 'markdown',
          content,
          contentSha256: createHash('sha256').update(content, 'utf8').digest('hex'),
          plainTextLength: content.length,
          lastModified: '2026-03-01T00:00:00.000Z',
          folderId: NESTED,
          fileId: DOC_FILE,
          linkId: DOC_LINK,
          linkGroupId: null,
        },
      ],
      blobs: [
        {
          mountPointId: VAULT,
          relativePath: 'images/portrait.bin',
          originalFileName: 'portrait.bin',
          originalMimeType: 'application/octet-stream',
          storedMimeType: 'application/octet-stream',
          sizeBytes: 9,
          sha256: createHash('sha256').update(Buffer.from('astrolabe')).digest('hex'),
          description: '',
          descriptionUpdatedAt: null,
          fileId: BLOB_FILE,
          linkId: BLOB_LINK,
          blobId: BLOB,
          extractedText: null,
          extractedTextSha256: null,
          extractionStatus: 'none',
          extractionError: null,
          dataBase64: Buffer.from('astrolabe').toString('base64'),
        },
      ],
      projectLinks: [],
    },
  };
}

/** The rehydrate target the second run of `execute_preserve_ids_vault` names. */
const PRESERVE_IDS_VAULT_TARGET = {
  mode: 'skip-if-present',
  targetCharacterId: 'cc000000-0000-4000-8000-000000000001',
  targetVaultMountPointId: 'cc000000-0000-4000-8000-000000000002',
};

/**
 * [WP B1] The same id claimed under TWO kinds inside one bundle — the
 * "(also seen as …)" refusal shape. Neither id exists in the instance, so the
 * repeat is the only thing that can refuse it.
 */
function repeatedIdPayload(): { manifest: unknown; data: Record<string, unknown> } {
  const shared = 'cb000000-0000-4000-8000-000000000001';
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'tags',
      createdAt: '2026-03-01T00:00:00.000Z',
      appVersion: 'test',
      settings: { includeMemories: false, scope: 'all', selectedIds: [], preserveIds: true },
      counts: {},
    },
    data: {
      characters: [
        {
          id: shared,
          name: 'Collider',
          identity: '',
          description: '',
          personality: '',
          avatarOverrides: [],
          tags: [],
        },
      ],
      tags: [{ id: shared, name: 'collider', nameLower: 'collider' }],
    },
  };
}

async function buildMergedExport(
  userId: string,
): Promise<{ manifest: unknown; data: Record<string, unknown[]> }> {
  const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
  // Fourteen of the fifteen `7189a968` types, in v4's declaration order.
  // `files` is deliberately EXCLUDED: its assembled data carries the general
  // folder tree under the same `folders` key the document-stores type uses
  // for doc-store folder rows, and this test-only merge would FUSE the two
  // namespaces — exactly the hazard v4's own `buildExportDataForType` note
  // documents, and a state no real single-type `.qtap` can produce. The files
  // type gets its own `execute_files_*` cases below.
  const types = [
    'characters',
    'chats',
    'roleplay-templates',
    'prompt-templates',
    'connection-profiles',
    'image-profiles',
    'embedding-profiles',
    'tags',
    'projects',
    'groups',
    'document-stores',
    'provider-models',
    'plugin-configs',
    'instance-settings',
  ];
  let manifest: unknown;
  const data: Record<string, unknown[]> = {};
  for (const type of types) {
    const bytes = await drainBytes(
      createNdjsonStream(userId, {
        type,
        scope: 'all',
        includeMemories: type === 'characters',
      } as never),
    );
    const assembled = await loadQtap(bytes);
    if (!manifest) manifest = assembled.manifest;
    for (const [k, v] of Object.entries(assembled.data)) {
      if (!Array.isArray(v)) continue;
      data[k] = data[k] ? [...data[k], ...v] : v;
    }
  }
  return { manifest, data };
}

/** The files-only export (`{files, folders}`), assembled from v4's own writer. */
async function buildFilesExport(
  userId: string,
): Promise<{ manifest: unknown; data: Record<string, unknown[]> }> {
  const { createNdjsonStream } = await import('@/lib/export/ndjson-writer');
  const bytes = await drainBytes(
    createNdjsonStream(userId, { type: 'files', scope: 'all', includeMemories: false } as never),
  );
  const assembled = await loadQtap(bytes);
  return { manifest: assembled.manifest, data: assembled.data as never };
}

/**
 * Deterministically rewrite every UUID in the payload (the cross-instance arm),
 * except the ids in `preserve`.
 *
 * [P4.33] `preserve` carries the payload's own `mountPoints[].id`. Since the
 * ruling made a store's identity its ID, rewriting those would no longer mean
 * "the same archive, from a foreign instance" for the document-store family —
 * it would mean "four stores this instance has never seen", which v4 claims by
 * NAME and v5 creates alongside. That is a genuine divergence, but pinning it
 * here would cost the whole-state equality this case exists for across the other
 * nine entity types; it gets its own dedicated `store_identity_*` arms instead,
 * over small hand-built archives where the end-state is legible. Everything else
 * — characters, chats, memories, projects, groups, tags, the profiles — still
 * arrives with foreign ids, which is what this case was built to prove.
 */
function rewriteIds(payload: unknown, preserve: string[] = []): unknown {
  let text = JSON.stringify(payload);
  const uuidRe = /[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}/g;
  const keep = new Set(preserve);
  const seen: string[] = [];
  for (const m of text.match(uuidRe) ?? []) {
    if (!seen.includes(m) && !keep.has(m)) seen.push(m);
  }
  seen.forEach((id, i) => {
    const replacement = `aa000000-0000-4000-8000-${String(i).padStart(12, '0')}`;
    text = text.split(id).join(replacement);
  });
  return JSON.parse(text);
}

/**
 * [40319484] The hand-built hard-link-group payload.
 *
 * Two documents carry the SAME exported `linkGroupId`, so the import's second
 * pass must re-bind them into one group; a third carries a group id nothing else
 * shares, so it must arrive as an inert group of ONE (left un-linked — a group
 * of one is not a link); a fourth carries none at all.
 *
 * Deliberately imported TWICE (see `executeTwiceCase`) under `duplicate`, which
 * mints a second store: the exported group id is only a grouping TOKEN, and
 * re-binding mints a fresh id, so the two copies must end up in two independent
 * groups rather than fused into one.
 */
function linkGroupPayload(): { manifest: unknown; data: Record<string, unknown> } {
  const MP = 'af1c0000-0000-4000-8000-000000000001';
  const GRP = 'af1c0000-0000-4000-8000-0000000000a1';
  const LONE = 'af1c0000-0000-4000-8000-0000000000a2';
  const doc = (rel: string, content: string, linkGroupId: string | null) => ({
    mountPointId: MP,
    relativePath: rel,
    fileName: rel,
    fileType: 'markdown',
    content,
    contentSha256: createHash('sha256').update(content, 'utf8').digest('hex'),
    plainTextLength: content.length,
    lastModified: '2026-01-01T00:00:00.000Z',
    folderId: null,
    linkGroupId,
  });
  const shared = '# Shared\n\nOne file, two paths.';
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'document-stores',
      createdAt: '2026-01-01T00:00:00.000Z',
      appVersion: '4.0.0',
      settings: { includeMemories: false, scope: 'all', selectedIds: [] },
      counts: {},
    },
    data: {
      // `executeImport` reads `data.mountPoints` / `.folders` / `.documents` /
      // `.blobs` / `.projectLinks` (execute.ts:350) — NOT the `documentStore*`
      // names the RESULT counts use.
      mountPoints: [
        {
          id: MP,
          name: 'Linked Store',
          basePath: '',
          mountType: 'database',
          storeType: 'documents',
          includePatterns: [],
          excludePatterns: [],
          enabled: true,
        },
      ],
      folders: [],
      documents: [
        doc('pair-a.md', shared, GRP),
        doc('pair-b.md', shared, GRP),
        doc('lonely.md', '# Lonely\n\nMy group members stayed home.', LONE),
        doc('plain.md', '# Plain\n\nNever linked at all.', null),
      ],
      blobs: [],
      projectLinks: [],
    },
  };
}

/**
 * P4.31 → P4.33 — a database store carrying FOLDERS, for the overwrite-clear
 * arm.
 *
 * `importDocumentStores`' overwrite branch clears documents, blobs, chunks and
 * files (`import-document-stores.ts:62-67`) but NOT `doc_mount_folders`, so a
 * re-import into the same store keeps the previous run's folder rows and then
 * fails to re-insert its own against the `(mountPointId, path)` unique index.
 * **v5 clears them too** — the ruled divergence (human, 2026-08-04: "overwrite
 * means overwrite"), under the standing 2026-08-03 backup/restore ruling.
 * Imported TWICE so the second run is the one that hits the overwrite branch,
 * with a DIFFERENT folder set the second time so the husks are visible.
 */
function folderOverwritePayload(
  variant: 'alpha' | 'gamma',
): { manifest: unknown; data: Record<string, unknown> } {
  const MP = 'af1e0000-0000-4000-8000-000000000001';
  const FOLDER_ALPHA = 'af1e0000-0000-4000-8000-0000000000f1';
  const FOLDER_BETA = 'af1e0000-0000-4000-8000-0000000000f2';
  const FOLDER_GAMMA = 'af1e0000-0000-4000-8000-0000000000f3';
  const doc = (rel: string, content: string, folderId: string | null) => ({
    mountPointId: MP,
    relativePath: rel,
    fileName: rel.split('/').pop(),
    fileType: 'markdown',
    content,
    contentSha256: createHash('sha256').update(content, 'utf8').digest('hex'),
    plainTextLength: content.length,
    lastModified: '2026-01-01T00:00:00.000Z',
    folderId,
    linkGroupId: null,
  });
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'document-stores',
      createdAt: '2026-01-01T00:00:00.000Z',
      appVersion: '4.0.0',
      settings: { includeMemories: false, scope: 'all', selectedIds: [] },
      counts: {},
    },
    data: {
      mountPoints: [
        {
          id: MP,
          name: 'Folder Store',
          basePath: '',
          mountType: 'database',
          storeType: 'documents',
          includePatterns: [],
          excludePatterns: [],
          enabled: true,
        },
      ],
      folders:
        variant === 'alpha'
          ? [
              { id: FOLDER_ALPHA, mountPointId: MP, parentId: null, name: 'alpha', path: 'alpha' },
              {
                id: FOLDER_BETA,
                mountPointId: MP,
                parentId: FOLDER_ALPHA,
                name: 'beta',
                path: 'alpha/beta',
              },
            ]
          : [
              { id: FOLDER_GAMMA, mountPointId: MP, parentId: null, name: 'gamma', path: 'gamma' },
            ],
      documents:
        variant === 'alpha'
          ? [
              doc('alpha/one.md', '# One\n\nIn alpha.', FOLDER_ALPHA),
              doc('alpha/beta/two.md', '# Two\n\nIn beta.', FOLDER_BETA),
            ]
          : [doc('gamma/three.md', '# Three\n\nIn gamma.', FOLDER_GAMMA)],
      blobs: [],
      projectLinks: [],
    },
  };
}

/**
 * [P4.33] The store-IDENTITY archives: one database store, one document, and an
 * id that is either the archive's own or a stranger's.
 *
 * Every store here is named `Identity Store…` so the dump can find them without
 * knowing which ids either engine minted, and so the fixture's own four stores
 * stay out of it.
 *
 * ⚠ The hand-built payloads in this file all draw from the `af1*` id space,
 * which the `system-data` fixture does not use. They used to draw from
 * `ab/ac/ad000000-0000-4000-8000-00000000000N`, and the fixture holds rows at
 * exactly those ids — harmless while both engines minted their own store ids
 * and the payload id was only ever a literal, but the moment v5 started
 * PRESERVING an archive's store id (P4.33), the link-group payload's mount
 * point collided with a fixture wardrobe item and the collision surfaced inside
 * a document's YAML front matter. Keep hand-built ids out of the fixture's
 * space.
 */
const IDENTITY_ID_1 = 'af1d0000-0000-4000-8000-000000000001';
const IDENTITY_ID_2 = 'af1d0000-0000-4000-8000-000000000002';

/**
 * [P4.D65] The `duplicate` × `preserveIds` corner P4.D62 banked — measured
 * rather than assumed, and the measurement DISPROVED the premise.
 *
 * The bank read "no oracle arm covers duplicate+preserveIds+name-match". It
 * cannot: every importer's conflict block is entered on an **id** match (the
 * "(imported)" rename happens *inside* it), and under `preserveIds` the
 * preflight checks all sixteen kinds and refuses the whole import on the first
 * id it finds already present. So a conflict strategy is never consulted at all
 * while `preserveIds` is on — which is exactly what v4's own profiles importer
 * calls "unreachable in practice", and why the `DUPLICATE_MINTS` discipline is
 * faithful-port cruft rather than live behaviour.
 *
 * Three arms make that claim measurable instead of asserted, all over this one
 * payload — a roleplay template and two tags carrying names the fixture already
 * uses (`House Style`, `Companion`, `Rival`), so a name-driven conflict would
 * have somewhere to fire if one existed:
 *
 *   - FREE ids + `skip` — the plain create forks on `preserveIds` and CLAIMS
 *     the carried ids;
 *   - FREE ids + `duplicate` — byte-identical to the above, because no id
 *     collides and the strategy never runs. Names alone do not conflict;
 *   - an EXISTING id + `duplicate` — refused at the preflight, before the
 *     strategy. This is the arm that pins the unreachability: if v4 ever
 *     narrows the preflight, it flips.
 *
 * The normalizer is what makes "claimed" visible: a payload id is an ORIGIN and
 * is compared literally, while a genuinely minted one collapses to
 * `<minted-N>`.
 *
 * Hand-built rather than a rewrite of the fixture's own characters export
 * because that bundle's five managed-field links share one content row, so the
 * same id is claimed under two kinds and the in-bundle repeat refuses long
 * before anything else is reached (already pinned by
 * `execute_preserve_ids_skip_if_present`).
 */
function duplicateNameMatchPayload(
  templateId = 'ad000000-0000-4000-8000-000000000001',
): { manifest: unknown; data: Record<string, unknown> } {
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'all',
      createdAt: '2026-01-01T00:00:00.000Z',
      appVersion: '4.0.0',
      settings: { includeMemories: false, scope: 'all', selectedIds: [], preserveIds: true },
      counts: {},
    },
    data: {
      roleplayTemplates: [
        {
          id: templateId,
          name: 'House Style',
          systemPrompt: 'Carried under a name this instance already uses.',
          annotationButtons: [],
        },
      ],
      tags: [
        { id: 'ad000000-0000-4000-8000-000000000011', name: 'Companion', nameLower: 'companion' },
        { id: 'ad000000-0000-4000-8000-000000000012', name: 'Rival', nameLower: 'rival' },
      ],
    },
  };
}

function identityStorePayload(
  id: string,
  name: string,
  docPath: string,
): { manifest: unknown; data: Record<string, unknown> } {
  const content = `# ${docPath}\n\nWritten into ${name}.`;
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'document-stores',
      createdAt: '2026-01-01T00:00:00.000Z',
      appVersion: '4.0.0',
      settings: { includeMemories: false, scope: 'all', selectedIds: [] },
      counts: {},
    },
    data: {
      mountPoints: [
        {
          id,
          name,
          basePath: '',
          mountType: 'database',
          storeType: 'documents',
          includePatterns: [],
          excludePatterns: [],
          enabled: true,
        },
      ],
      folders: [],
      documents: [
        {
          mountPointId: id,
          relativePath: docPath,
          fileName: docPath,
          fileType: 'markdown',
          content,
          contentSha256: createHash('sha256').update(content, 'utf8').digest('hex'),
          plainTextLength: content.length,
          lastModified: '2026-01-01T00:00:00.000Z',
          folderId: null,
          linkGroupId: null,
        },
      ],
      blobs: [],
      projectLinks: [],
    },
  };
}

/**
 * kind 'execute_store_identity' (P4.33): replay a scripted sequence of imports
 * — with the occasional hand rename between them, which is how a user redirects
 * v4's name matching — then dump every `Identity Store…` store with the
 * documents that ended up in it.
 *
 * Raw ids are emitted; the Rust side classifies them against the archives'
 * own ids, so the comparison is "which store did the import land on", not
 * "which uuid did this engine mint".
 */
function storeIdentityCase(name: string, steps: Array<Record<string, unknown>>) {
  return {
    name,
    run: async (spec: Spec) => {
      const { executeImport } = await import('@/lib/import/quilltap-import/execute');
      const { getRawMountIndexDatabase } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      const results: unknown[] = [];
      for (const step of steps) {
        if (step.op === 'import') {
          results.push(await executeImport(spec.userId, step.data as never, step.options as never));
        } else if (step.op === 'rename') {
          getRawMountIndexDatabase()!
            .prepare('UPDATE doc_mount_points SET name = ? WHERE name = ?')
            .run(step.to as string, step.from as string);
        } else {
          throw new Error(`unknown step op ${String(step.op)}`);
        }
      }
      const midb = getRawMountIndexDatabase()!;
      const stores = midb
        .prepare(
          `SELECT id, name, storeType, mountType, enabled
             FROM doc_mount_points WHERE name LIKE 'Identity Store%' ORDER BY name, id`,
        )
        .all() as unknown[];
      const docs = midb
        .prepare(
          `SELECT p.name AS storeName, l.mountPointId AS storeId, l.relativePath AS relativePath
             FROM doc_mount_file_links l
             JOIN doc_mount_points p ON p.id = l.mountPointId
            WHERE p.name LIKE 'Identity Store%'
            ORDER BY p.name, p.id, l.relativePath`,
        )
        .all() as unknown[];
      return { kind: 'execute_store_identity', steps, results, stores, docs };
    },
  };
}

/** The hand-built legacy-folds payload (scenario string, outfitPresets,
 *  annotationButtons, missing supportsImageUpload, the three memory arms). */
function legacyFoldsPayload(): { manifest: unknown; data: Record<string, unknown> } {
  const CHAR = 'ab000000-0000-4000-8000-000000000001';
  const W_TOP = 'ab000000-0000-4000-8000-000000000011';
  const W_HAT = 'ab000000-0000-4000-8000-000000000012';
  return {
    manifest: {
      format: 'quilltap-export',
      version: '1.0',
      exportType: 'characters',
      createdAt: '2026-01-01T00:00:00.000Z',
      appVersion: '4.0.0',
      settings: { includeMemories: true, scope: 'all', selectedIds: [] },
      counts: {},
    },
    data: {
      characters: [
        {
          id: CHAR,
          name: 'Legacy Larkspur',
          identity: 'A porter of parcels most peculiar.',
          description: 'Weathered, whimsical.',
          personality: 'Punctilious.',
          scenario: 'The old harbour office, at closing time.',
          outfitPresets: [
            {
              id: 'ab000000-0000-4000-8000-000000000021',
              characterId: CHAR,
              name: 'Harbour Rounds',
              description: 'The daily rounds outfit.',
              slots: { top: W_TOP, bottom: null, footwear: null, accessories: W_HAT },
              createdAt: '2026-01-01T00:00:00.000Z',
              updatedAt: '2026-01-01T00:00:00.000Z',
            },
          ],
          wardrobeItems: [
            {
              id: W_TOP,
              characterId: CHAR,
              title: 'Oilskin Coat',
              description: 'Keeps the weather honest.',
              imagePrompt: null,
              types: ['top'],
              componentItemIds: [],
              appropriateness: null,
              isDefault: false,
              replace: false,
              migratedFromClothingRecordId: null,
              archivedAt: null,
              createdAt: '2026-01-01T00:00:00.000Z',
              updatedAt: '2026-01-01T00:00:00.000Z',
            },
            {
              id: W_HAT,
              characterId: CHAR,
              title: 'Brass-Buttoned Cap',
              description: null,
              imagePrompt: null,
              types: ['accessories'],
              componentItemIds: [],
              appropriateness: null,
              isDefault: false,
              replace: false,
              migratedFromClothingRecordId: null,
              archivedAt: null,
              createdAt: '2026-01-01T00:00:00.000Z',
              updatedAt: '2026-01-01T00:00:00.000Z',
            },
          ],
          pluginData: { 'weather-eye': { forecast: 'squally' } },
        },
      ],
      roleplayTemplates: [
        {
          id: 'ab000000-0000-4000-8000-000000000031',
          name: 'Legacy Buttons',
          systemPrompt: 'Mind the buttons.',
          annotationButtons: [
            { label: 'Narration', abbrev: 'Nar', prefix: '*', suffix: '*' },
            { label: 'Out of Character', abbrev: 'OOC', prefix: '// ' },
          ],
          pluginName: 'legacy-plugin',
        },
      ],
      connectionProfiles: [
        {
          id: 'ab000000-0000-4000-8000-000000000041',
          name: 'Legacy OpenAI',
          provider: 'OPENAI',
          modelName: 'gpt-4',
        },
        {
          id: 'ab000000-0000-4000-8000-000000000042',
          name: 'Legacy DeepSeek',
          provider: 'DEEPSEEK',
          modelName: 'deepseek-chat',
        },
      ],
      memories: [
        {
          id: 'ab000000-0000-4000-8000-000000000051',
          characterId: CHAR,
          content: 'The tide tables, memorized.',
          summary: 'tide tables',
          embedding: [0.6, 0.8],
        },
        {
          id: 'ab000000-0000-4000-8000-000000000052',
          characterId: CHAR,
          content: 'A stringified Float32Array survives no round-trip.',
          summary: 'embedding trap',
          embedding: { '0': 0.6, '1': 0.8 },
        },
        {
          id: 'ab000000-0000-4000-8000-000000000053',
          characterId: 'ab000000-0000-4000-8000-0000000000ff',
          content: 'Orphaned.',
          summary: 'orphan',
        },
      ],
    },
  };
}

interface CaseOut {
  name: string;
  [k: string]: unknown;
}

async function runCase(
  spec: Spec,
  name: string,
  scratch: string,
  fixtures: { main: string; mount: string; llm: string },
  run: (spec: Spec, dumpAll: () => Record<string, unknown>) => Promise<Record<string, unknown>>,
): Promise<CaseOut> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratch, 'sd-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const dumpAll = () => ({
    main: dumpPartition(getRawDatabase()!),
    mountIndex: dumpPartition(getRawMountIndexDatabase()!),
    llmLogs: dumpPartition(getRawLLMLogsDatabase()!),
  });

  try {
    const out = await run(spec, dumpAll);
    return { name, ...out };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

/** kind 'execute': run v4's real `executeImport` and dump before/after. */
function executeCase(
  name: string,
  payload: (spec: Spec) => Promise<unknown> | unknown,
  options: Record<string, unknown>,
) {
  return {
    name,
    run: async (spec: Spec, dumpAll: () => Record<string, unknown>) => {
      const exportData = await payload(spec);
      const preState = dumpAll();
      const { executeImport } = await import('@/lib/import/quilltap-import/execute');
      const result = await executeImport(spec.userId, exportData as never, options as never);
      await settle();
      return { kind: 'execute', exportData, options, result, preState, state: dumpAll() };
    },
  };
}

/**
 * kind 'execute_twice': run v4's REAL `executeImport` over the SAME payload
 * twice, in one instance. The only arm that can prove a re-imported archive does
 * not FUSE with its earlier copy (see `linkGroupPayload`).
 */
/**
 * kind 'execute_folder_overwrite' (P4.31): import [`folderOverwritePayload`]
 * twice with `conflictStrategy: 'overwrite'`, then dump the resulting store's
 * folders by PATH (no ids — both sides mint their own) alongside both result
 * bodies. See the case list for why this does not reuse `execute_twice`.
 */
function folderOverwriteCase() {
  return {
    name: 'execute_folder_overwrite',
    run: async (spec: Spec, _dumpAll: () => Record<string, unknown>) => {
      const first = folderOverwritePayload('alpha');
      const exportData = folderOverwritePayload('gamma');
      const options = {
        conflictStrategy: 'overwrite',
        includeMemories: false,
        includeRelatedEntities: false,
      };
      const { executeImport } = await import('@/lib/import/quilltap-import/execute');
      // Run 1 seeds the store with `alpha` + `alpha/beta`; run 2 overwrites it
      // with a DIFFERENT folder set, which is what makes the gap visible: v4's
      // overwrite clears the documents but not the folders, so `alpha` and
      // `alpha/beta` survive an archive that no longer contains them.
      const result = await executeImport(spec.userId, first as never, options as never);
      const result2 = await executeImport(spec.userId, exportData as never, options as never);

      const { getRawMountIndexDatabase } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      const midb = getRawMountIndexDatabase()!;
      const store = midb
        .prepare('SELECT id FROM doc_mount_points WHERE name = ?')
        .get('Folder Store') as { id: string } | undefined;
      const folders = store
        ? (midb
            .prepare(
              'SELECT path, name FROM doc_mount_folders WHERE mountPointId = ? ORDER BY path, name',
            )
            .all(store.id) as unknown[])
        : [];
      // Each link with the PATH of the folder it resolves to (never the id —
      // both sides mint their own). A link whose `folderId` names no surviving
      // folder row reports `<dangling>`: that is the second-order damage when
      // the folder re-import fails, since the document's remapped folderId has
      // nowhere to land.
      const links = store
        ? (midb
            .prepare(
              `SELECT l.relativePath AS relativePath,
                      CASE WHEN l.folderId IS NULL THEN NULL
                           ELSE COALESCE(f.path, '<dangling>') END AS folderPath
                 FROM doc_mount_file_links l
                 LEFT JOIN doc_mount_folders f ON f.id = l.folderId
                WHERE l.mountPointId = ?
                ORDER BY l.relativePath`,
            )
            .all(store.id) as unknown[])
        : [];
      const storeCount = (
        midb.prepare('SELECT COUNT(*) AS c FROM doc_mount_points WHERE name = ?').get('Folder Store') as {
          c: number;
        }
      ).c;
      return {
        kind: 'execute_folder_overwrite',
        firstData: first,
        exportData,
        options,
        result,
        result2,
        folders,
        links,
        storeCount,
      };
    },
  };
}

function executeTwiceCase(
  name: string,
  payload: (spec: Spec) => Promise<unknown> | unknown,
  options: Record<string, unknown>,
) {
  return {
    name,
    run: async (spec: Spec, dumpAll: () => Record<string, unknown>) => {
      const exportData = await payload(spec);
      const preState = dumpAll();
      const { executeImport } = await import('@/lib/import/quilltap-import/execute');
      const result = await executeImport(spec.userId, exportData as never, options as never);
      const result2 = await executeImport(spec.userId, exportData as never, options as never);
      await settle();
      return {
        kind: 'execute_twice',
        exportData,
        options,
        result,
        result2,
        preState,
        state: dumpAll(),
      };
    },
  };
}

/**
 * kind 'execute_then_rehydrate' [P4.D62]: run v4's REAL `executeImport` over the
 * SAME payload twice with DIFFERENT options — run 1 lands the bundle at its
 * carried ids (refuse-on-collision), run 2 replays it as a rehydrate
 * (skip-if-present), where every id now exists INSIDE the target and must be
 * skipped rather than refused. Nothing may be duplicated by the second run.
 */
function executeThenRehydrateCase(
  name: string,
  payload: (spec: Spec) => Promise<unknown> | unknown,
  options: Record<string, unknown>,
  options2: Record<string, unknown>,
) {
  return {
    name,
    run: async (spec: Spec, dumpAll: () => Record<string, unknown>) => {
      const exportData = await payload(spec);
      const preState = dumpAll();
      const { executeImport } = await import('@/lib/import/quilltap-import/execute');
      const result = await executeImport(spec.userId, exportData as never, options as never);
      const result2 = await executeImport(spec.userId, exportData as never, options2 as never);
      await settle();
      return {
        kind: 'execute_then_rehydrate',
        exportData,
        options,
        options2,
        result,
        result2,
        preState,
        state: dumpAll(),
      };
    },
  };
}

function mockRequest(url: string, method: string, body: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

/** kind 'route': drive v4's REAL tools route POST handler (the JSON leg). */
function routeCase(
  name: string,
  body: (spec: Spec) => Promise<unknown> | unknown,
  opts?: { dumpState?: boolean },
) {
  return {
    name,
    run: async (spec: Spec, dumpAll: () => Record<string, unknown>) => {
      const requestBody = await body(spec);
      const preState = opts?.dumpState ? dumpAll() : undefined;
      const route = (await import('@/app/api/v1/system/tools/route')) as {
        POST: (req: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
      };
      const resp = await route.POST(
        mockRequest('http://localhost/api/v1/system/tools?action=import-execute', 'POST', requestBody),
      );
      await settle();
      const out: Record<string, unknown> = {
        kind: 'route',
        requestBody,
        status: resp.status,
        body: await resp.json(),
      };
      if (opts?.dumpState) {
        out.preState = preState;
        out.state = dumpAll();
      }
      return out;
    },
  };
}

/**
 * kind 'execute_prepped' (P4.D46, `7189a968`): a NAMED pre-import mutation on
 * the live fixture copy, then v4's REAL `executeImport`. The two preps are the
 * `enqueueImportedMemoryEmbeddings` bail-out arms:
 *   - 'drop-embedding-profiles' → "no default embedding profile is configured"
 *   - 'builtin-default-profile' → "the configured embedding profile is the
 *     built-in TF-IDF one"
 * The Rust side applies the same named mutation to its own fixture copy before
 * importing, so the preState baselines still match byte-for-byte.
 */
function executePreppedCase(
  name: string,
  prep: 'drop-embedding-profiles' | 'builtin-default-profile',
  payload: (spec: Spec) => Promise<unknown> | unknown,
  options: Record<string, unknown>,
) {
  return {
    name,
    run: async (spec: Spec, dumpAll: () => Record<string, unknown>) => {
      const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
      const db = getRawDatabase();
      if (!db) throw new Error('main DB handle unavailable for prep');
      if (prep === 'drop-embedding-profiles') {
        db.exec('DELETE FROM "embedding_profiles"');
      } else {
        db.exec(`UPDATE "embedding_profiles" SET "provider" = 'BUILTIN'`);
      }
      const exportData = await payload(spec);
      const preState = dumpAll();
      const { executeImport } = await import('@/lib/import/quilltap-import/execute');
      const result = await executeImport(spec.userId, exportData as never, options as never);
      await settle();
      return {
        kind: 'execute_prepped',
        prep,
        exportData,
        options,
        result,
        preState,
        state: dumpAll(),
      };
    },
  };
}

/** The multipart options-part arm: v4's `req.formData()` branch with a bad
 *  `options` string. Pins 'Invalid JSON: Failed to parse options'. */
function multipartBadOptionsCase() {
  return {
    name: 'route_multipart_bad_options',
    run: async (spec: Spec) => {
      const legacy = JSON.stringify({
        manifest: {
          format: 'quilltap-export',
          version: '1.0',
          exportType: 'tags',
          createdAt: '2026-01-01T00:00:00.000Z',
          appVersion: '4.0.0',
          settings: { includeMemories: false, scope: 'all', selectedIds: [] },
          counts: {},
        },
        data: { tags: [] },
      });
      const fd = new FormData();
      fd.append('file', new File([legacy], 'x.qtap', { type: 'application/octet-stream' }));
      fd.append('options', 'not json at all');
      const req = {
        method: 'POST',
        url: 'http://localhost/api/v1/system/tools?action=import-execute',
        nextUrl: new URL('http://localhost/api/v1/system/tools?action=import-execute'),
        headers: new Headers({ 'Content-Type': 'multipart/form-data; boundary=x' }),
        formData: async () => fd,
        json: async () => ({}),
      };
      const route = (await import('@/app/api/v1/system/tools/route')) as {
        POST: (req: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
      };
      const resp = await route.POST(req);
      void spec;
      return { kind: 'route', status: resp.status, body: await resp.json() };
    },
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sysimportexec-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // The merged payload is built ONCE, against a throwaway copy of the fixture,
  // and then replayed byte-identically into every strategy arm.
  const merged = await runCase(spec, '_build', scratch, fixtures, async () => ({
    payload: await buildMergedExport(spec.userId),
    filesPayload: await buildFilesExport(spec.userId),
    charactersPayload: await buildCharactersExport(spec.userId),
    lorianPayload: await buildCharactersExport(spec.userId, [
      'a1000000-0000-4000-8000-000000000001',
    ]),
  }));
  const mergedPayload = merged.payload as { manifest: unknown; data: Record<string, unknown[]> };
  const filesPayload = merged.filesPayload as { manifest: unknown; data: Record<string, unknown[]> };
  const charactersPayload = merged.charactersPayload as {
    manifest: unknown;
    data: Record<string, unknown[]>;
  };
  const lorianPayload = merged.lorianPayload as {
    manifest: unknown;
    data: Record<string, unknown[]>;
  };
  const LORIAN = 'a1000000-0000-4000-8000-000000000001';
  const SKIP_IF_PRESENT_LORIAN = {
    mode: 'skip-if-present',
    targetCharacterId: LORIAN,
    targetVaultMountPointId: LORIAN_VAULT,
  };

  const cases = [
    executeCase('execute_skip_all', () => mergedPayload, {
      conflictStrategy: 'skip',
      includeMemories: true,
      includeRelatedEntities: false,
    }),
    executeCase('execute_overwrite_all', () => mergedPayload, {
      conflictStrategy: 'overwrite',
      includeMemories: true,
      includeRelatedEntities: false,
    }),
    executeCase('execute_duplicate_all', () => mergedPayload, {
      conflictStrategy: 'duplicate',
      includeMemories: true,
      includeRelatedEntities: false,
    }),
    executeCase(
      'execute_cross_instance_skip',
      () =>
        rewriteIds(
          mergedPayload,
          ((mergedPayload.data.mountPoints ?? []) as Array<{ id: string }>).map((m) => m.id),
        ),
      {
        conflictStrategy: 'skip',
        includeMemories: true,
        includeRelatedEntities: false,
      },
    ),
    executeCase('execute_legacy_folds', () => legacyFoldsPayload(), {
      conflictStrategy: 'skip',
      includeMemories: true,
      includeRelatedEntities: false,
    }),
    executeTwiceCase('execute_link_groups_twice', () => linkGroupPayload(), {
      conflictStrategy: 'duplicate',
      includeMemories: false,
      includeRelatedEntities: false,
    }),
    // [P4.31] The overwrite-clear's folder gap. Its own kind rather than
    // `execute_twice` because v5 deliberately diverges here, and this family's
    // whole-state normalizer labels minted ids in walk order — dropping rows
    // from one side would shift every label after them. The comparison is
    // instead a focused, id-free dump of the store's folders plus both result
    // bodies.
    folderOverwriteCase(),
    // [P4.33] The four store-IDENTITY arms. `overwrite` throughout except the
    // last, which is the skip arm.
    //
    // (a) A rename on the TARGET — the story from the escalation: v4 loses
    //     track of the store its own archive came from and creates a second
    //     one, while v5 finds it by id and overwrites it (restoring the
    //     archive's name in the bargain).
    storeIdentityCase('store_identity_target_renamed', [
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_1, 'Identity Store', 'one.md'),
        options: { conflictStrategy: 'overwrite', includeMemories: false, includeRelatedEntities: false },
      },
      { op: 'rename', from: 'Identity Store', to: 'Identity Store (renamed by hand)' },
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_1, 'Identity Store', 'one.md'),
        options: { conflictStrategy: 'overwrite', includeMemories: false, includeRelatedEntities: false },
      },
    ]),
    // (a, mirrored) A rename in the ARCHIVE. Same id, new display name: v5
    //     overwrites and renames, v4 sees a stranger and creates.
    storeIdentityCase('store_identity_archive_renamed', [
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_1, 'Identity Store', 'one.md'),
        options: { conflictStrategy: 'overwrite', includeMemories: false, includeRelatedEntities: false },
      },
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_1, 'Identity Store Under A New Name', 'two.md'),
        options: { conflictStrategy: 'overwrite', includeMemories: false, includeRelatedEntities: false },
      },
    ]),
    // (b) + (c) A stranger's archive that merely SHARES the name: v4 claims the
    //     existing store, v5 creates alongside under a uniquified name — and
    //     re-importing that same stranger's archive then finds what it created,
    //     by id. That last step is the convergence property: a foreign archive
    //     imports once and updates thereafter, instead of multiplying.
    storeIdentityCase('store_identity_same_name_new_id', [
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_1, 'Identity Store', 'one.md'),
        options: { conflictStrategy: 'overwrite', includeMemories: false, includeRelatedEntities: false },
      },
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_2, 'Identity Store', 'two.md'),
        options: { conflictStrategy: 'overwrite', includeMemories: false, includeRelatedEntities: false },
      },
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_2, 'Identity Store', 'two.md'),
        options: { conflictStrategy: 'overwrite', includeMemories: false, includeRelatedEntities: false },
      },
    ]),
    // (d) `skip` maps by id too: the second archive wears a different name but
    //     the same id, so v5 recognizes the store and pours its document into
    //     it, where v4 creates a second store to hold it.
    storeIdentityCase('store_identity_skip_by_id', [
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_1, 'Identity Store', 'one.md'),
        options: { conflictStrategy: 'skip', includeMemories: false, includeRelatedEntities: false },
      },
      {
        op: 'import',
        data: identityStorePayload(IDENTITY_ID_1, 'Identity Store With Another Face', 'three.md'),
        options: { conflictStrategy: 'skip', includeMemories: false, includeRelatedEntities: false },
      },
    ]),
    // [P4.D46] The files type, over its own single-type payload (see the
    // `types` note). `skip` is all-skips on a same-instance re-import (the
    // `_bytesMissing` row skips with its warning on every arm); `overwrite`
    // deletes + recreates by id; `duplicate` re-mints; the cross-instance arm
    // exercises the linkedTo drop/keep logic over foreign ids.
    // ── [P4.D62] preserveIds — WP B1 + Bugs 52/54 ─────────────────────────
    // Every id in the bundle already exists here, so refuse-on-collision must
    // refuse on the FIRST check kind and write nothing at all.
    executeCase('execute_preserve_ids_refuse_existing', () => charactersPayload, {
      conflictStrategy: 'skip',
      includeMemories: true,
      includeRelatedEntities: false,
      preserveIds: true,
    }),
    // The same id under two kinds inside one bundle — the "(also seen as …)"
    // shape, which nothing else can produce.
    executeCase('execute_preserve_ids_repeat_in_bundle', () => repeatedIdPayload(), {
      conflictStrategy: 'skip',
      includeMemories: false,
      includeRelatedEntities: false,
      preserveIds: true,
    }),
    // A stranger's bundle into free id space: every row — the character, its
    // memory, the vault mount, both folders, the document and the blob — must
    // land AT the carried id, and reconciliation must repoint the character at
    // its imported vault and tear down the scaffold `create()` provisioned.
    // Then the SAME bundle replayed as a rehydrate: every id now exists inside
    // the target, so skip-if-present must skip rather than refuse, and the
    // second run may not duplicate a single row.
    executeThenRehydrateCase(
      'execute_preserve_ids_vault',
      () => preserveIdsVaultPayload(),
      {
        conflictStrategy: 'skip',
        includeMemories: true,
        includeRelatedEntities: false,
        preserveIds: true,
      },
      {
        conflictStrategy: 'skip',
        includeMemories: true,
        includeRelatedEntities: false,
        preserveIds: true,
        preserveIdsMode: PRESERVE_IDS_VAULT_TARGET,
      },
    ),
    // The fixture's OWN characters bundle replayed as a rehydrate of Lorian:
    // its five managed-field links share one content row that also carries a
    // blob, so the same linkId is claimed under both `documents` and `blobs` —
    // the in-bundle repeat refuses before the target check is reached. Pinned
    // because it is the shape a real vault export of this fixture produces.
    executeCase('execute_preserve_ids_skip_if_present', () => lorianPayload, {
      conflictStrategy: 'skip',
      includeMemories: true,
      includeRelatedEntities: false,
      preserveIds: true,
      preserveIdsMode: SKIP_IF_PRESENT_LORIAN,
    }),
    // …but a collision ANYWHERE ELSE still refuses the whole import: the same
    // rehydrate target with a bundle that also carries the OTHER character.
    executeCase('execute_preserve_ids_skip_foreign_refuses', () => charactersPayload, {
      conflictStrategy: 'skip',
      includeMemories: true,
      includeRelatedEntities: false,
      preserveIds: true,
      preserveIdsMode: SKIP_IF_PRESENT_LORIAN,
    }),
    // [Bug 54] Content-addressed rows the target legitimately owned, whose only
    // surviving links are in a CO-OWNER's vault. Matching bytes are dedup and
    // pass; a same-id/different-bytes row is a real clash and refuses.
    executeCase('execute_preserve_ids_dedup_by_sha', () => sharedContentPayload(true), {
      conflictStrategy: 'skip',
      includeMemories: false,
      includeRelatedEntities: false,
      preserveIds: true,
      preserveIdsMode: SKIP_IF_PRESENT_LORIAN,
    }),
    executeCase('execute_preserve_ids_sha_mismatch_refuses', () => sharedContentPayload(false), {
      conflictStrategy: 'skip',
      includeMemories: false,
      includeRelatedEntities: false,
      preserveIds: true,
      preserveIdsMode: SKIP_IF_PRESENT_LORIAN,
    }),
    // [P4.D65] The `duplicate` × `preserveIds` corner P4.D62 banked — see
    // `duplicateNameMatchPayload` for the premise it disproves and what each of
    // these three arms pins.
    executeCase('execute_preserve_ids_plain_claims_ids', () => duplicateNameMatchPayload(), {
      conflictStrategy: 'skip',
      includeMemories: false,
      includeRelatedEntities: false,
      preserveIds: true,
    }),
    executeCase('execute_preserve_ids_duplicate_free_ids', () => duplicateNameMatchPayload(), {
      conflictStrategy: 'duplicate',
      includeMemories: false,
      includeRelatedEntities: false,
      preserveIds: true,
    }),
    executeCase(
      'execute_preserve_ids_duplicate_existing_id_refuses',
      () => duplicateNameMatchPayload(FIXTURE_TEMPLATE_ID),
      {
        conflictStrategy: 'duplicate',
        includeMemories: false,
        includeRelatedEntities: false,
        preserveIds: true,
      },
    ),
    executeCase('execute_files_skip', () => filesPayload, {
      conflictStrategy: 'skip',
      includeMemories: false,
      includeRelatedEntities: false,
    }),
    executeCase('execute_files_overwrite', () => filesPayload, {
      conflictStrategy: 'overwrite',
      includeMemories: false,
      includeRelatedEntities: false,
    }),
    executeCase('execute_files_duplicate', () => filesPayload, {
      conflictStrategy: 'duplicate',
      includeMemories: false,
      includeRelatedEntities: false,
    }),
    executeCase('execute_files_cross_instance', () => rewriteIds(filesPayload), {
      conflictStrategy: 'skip',
      includeMemories: false,
      includeRelatedEntities: false,
    }),
    // [P4.D46] The embedding-enqueue bail-out arms: a characters+memories slice
    // of the merged payload, imported after a named fixture mutation. `skip`
    // keeps the character rows still (memories always insert), so the state
    // delta is the memories + the (absent) job rows + the warning.
    executePreppedCase(
      'execute_memories_no_profile',
      'drop-embedding-profiles',
      () => ({
        manifest: mergedPayload.manifest,
        data: {
          characters: mergedPayload.data.characters ?? [],
          memories: mergedPayload.data.memories ?? [],
        },
      }),
      { conflictStrategy: 'skip', includeMemories: true, includeRelatedEntities: false },
    ),
    executePreppedCase(
      'execute_memories_builtin_profile',
      'builtin-default-profile',
      () => ({
        manifest: mergedPayload.manifest,
        data: {
          characters: mergedPayload.data.characters ?? [],
          memories: mergedPayload.data.memories ?? [],
        },
      }),
      { conflictStrategy: 'skip', includeMemories: true, includeRelatedEntities: false },
    ),
    routeCase('route_missing_export_data', () => ({})),
    routeCase('route_missing_options', () => ({
      exportData: legacyFoldsPayload(),
    })),
    routeCase('route_bad_strategy', () => ({
      exportData: legacyFoldsPayload(),
      options: { conflictStrategy: 'obliterate' },
    })),
    routeCase('route_data_undefined', () => ({
      exportData: { manifest: (legacyFoldsPayload() as { manifest: unknown }).manifest },
      options: { conflictStrategy: 'skip' },
    })),
    routeCase(
      'route_replace_remap',
      () => ({
        exportData: mergedPayload,
        options: { conflictStrategy: 'replace', importMemories: true },
      }),
      { dumpState: true },
    ),
    multipartBadOptionsCase(),
  ];

  const outLines: string[] = [
    JSON.stringify({ name: '_meta', kind: 'meta', userId: spec.userId }),
  ];
  for (const c of cases) {
    const payload = await runCase(spec, c.name, scratch, fixtures, c.run);
    outLines.push(JSON.stringify(payload));
    process.stderr.write(`  case ${c.name} done\n`);
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `system-import-execute oracle wrote ${outPath} (${outLines.length - 1} cases)\n`,
  );
}

test('system-import-execute oracle', async () => {
  await main();
});
