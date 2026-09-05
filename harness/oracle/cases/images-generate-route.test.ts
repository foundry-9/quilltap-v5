/**
 * @jest-environment node
 *
 * P4.76 — the `POST /api/v1/images?action=generate` ORACLE. Drives v4's REAL
 * `handleGenerateImage` (`app/api/v1/images/route.ts:177-408`) over a FRESH copy
 * of the committed images fixture per case, recording the response, the ORDERED
 * provider calls, the ORDERED Concierge classification calls, and the
 * post-mutation `files` + mount-link rows.
 *
 * This is v4's OWN route-level generation, not the Salon tool: a `scanImagePrompts`
 * gate with NO chat, a reroute that takes the FIRST `isDangerousCompatible`
 * profile rather than the Concierge desk's, and no orientation at all.
 *
 * ## The seams (pinned identically on the Rust side)
 *
 * * **`createImageProvider`** → a recorder. It is BEHAVIOURAL, not keyed: it
 *   answers `params.n` images built from the index, and RECORDS
 *   `{provider, apiKey, params}` per call. The recorded params are the comparand
 *   (a keyed canned map would answer "miss" instead of showing the divergence),
 *   and the recorded `apiKey` is what proves an AUTO_ROUTE reroute switched
 *   profiles rather than merely switching names.
 * * **`classifyContent`** → canned per case, and RECORDED with the
 *   `CheapLLMSelection` v4 built. That selection is otherwise invisible: the
 *   route builds it from `allProfiles` + `cheapLLMSettings` and hands it
 *   straight to the classifier.
 * * **the danger settings** are patched per case with a raw UPDATE on the
 *   working copy — the fixture stores v4's `mode: 'OFF'` default, and seeding a
 *   second user per mode would change every other family's row counts.
 * * **`Date`** is FROZEN (`spec.frozenNowMs`): the stored filename is
 *   `generated_<Date.now()>_<index>_<sha8>.webp`, so an unfrozen clock makes
 *   every generate row unstable. (That is also the mutation proof: un-freeze it
 *   and every row must redden.)
 * * `logLLMCall` is a no-op — the classification is mocked above it, but the
 *   route's own imports pull the module in.
 *
 * Provider bytes are `image/webp` on purpose: `convertToWebP` PASSES those
 * through, so the stored bytes — and therefore the sha, the `_<sha8>_` inside
 * the filename, and the byte length — are identical on both sides and are real
 * comparands rather than blanked ones. The one `image/png` case exists to
 * exercise the transcode POLICY (D19: compare the mime, never sharp's bytes).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-images-generate-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/images-generate-route.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/images-collection.json" "$TMPO/fixtures/"
 *   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db"  /tmp/qt-imgcol-main.db
 *   cp "$V5W/crates/quilltap-web/tests/fixtures/images-mount.db" /tmp/qt-imgcol-mount.db
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_FIXTURE_IMGCOL_MAIN=/tmp/qt-imgcol-main.db \
 *   QT_FIXTURE_IMGCOL_MOUNT=/tmp/qt-imgcol-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-images-generate.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- images-generate-route
 */

import { copyFileSync, mkdtempSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import * as fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
  userId: string;
  uploadsMountPointId: string;
  lanternMountPointId: string;
}

const IMAGES = 'http://localhost/api/v1/images?action=generate';

// The fixture's pinned ids (lockstep with `build-images-collection-fixture.ts`).
const PROFILE_MAIN = 'aaaa0000-0000-4000-8000-000000000001'; // OPENAI, isDefault
const PROFILE_UNCENSORED = 'aaaa0000-0000-4000-8000-000000000002'; // GROK, isDangerousCompatible
const PROFILE_NOIMAGE = 'aaaa0000-0000-4000-8000-000000000003'; // OLLAMA, no image capability
const MISSING_PROFILE = 'aaaa0000-0000-4000-8000-0000000000ff';
const CHAR_TAG = 'c1000000-0000-4000-8000-000000000003';
const THEME_TAG = 'ee000000-0000-4000-8000-000000000001';

/** A real 1x1 PNG — sharp must decode it for the transcode arm to mean anything. */
const PNG_1X1 = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
);

type ProviderMode = 'webp' | 'png' | 'throw' | 'nodata';

interface CannedClassification {
  isDangerous: boolean;
  score: number;
  categories: Array<{ category: string; score: number; label: string }>;
}

interface CaseSpec {
  name: string;
  body: unknown;
  /** Merged over the stored `dangerousContentSettings` before the case runs. */
  danger?: Record<string, unknown>;
  classify?: CannedClassification;
  provider?: ProviderMode;
  /** Remove the Lantern Backgrounds pointer, so the store write throws. */
  dropLantern?: boolean;
}

const SAFE: CannedClassification = { isDangerous: false, score: 0.1, categories: [] };
const DANGEROUS: CannedClassification = {
  isDangerous: true,
  score: 0.93,
  categories: [{ category: 'sexual', score: 0.93, label: 'Sexual content' }],
};

function mockGeneratePost(body: unknown): unknown {
  return {
    method: 'POST',
    url: IMAGES,
    nextUrl: new URL(IMAGES),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

/** The per-case recorders, reset by `runCase`. */
let providerCalls: Array<Record<string, unknown>> = [];
let classifyCalls: Array<Record<string, unknown>> = [];
let providerMode: ProviderMode = 'webp';
let cannedClassification: CannedClassification = SAFE;

function cannedImages(n: number): Array<Record<string, unknown>> {
  const out: Array<Record<string, unknown>> = [];
  for (let i = 0; i < n; i += 1) {
    if (providerMode === 'nodata') {
      out.push({ mimeType: 'image/webp', revisedPrompt: `revised ${i}` });
      continue;
    }
    if (providerMode === 'png') {
      out.push({
        data: PNG_1X1.toString('base64'),
        mimeType: 'image/png',
        revisedPrompt: `revised ${i}`,
      });
      continue;
    }
    // `image/webp` passes `convertToWebP` through untouched, so these bytes —
    // and their sha — are identical on both sides of the differential.
    out.push({
      data: Buffer.from(`QTAP-P476-WEBP-${i}`, 'utf8').toString('base64'),
      mimeType: 'image/webp',
      // Index 1 deliberately carries NO revisedPrompt, so the receipt's
      // undefined-drop (an ABSENT key, never null) is measured.
      ...(i === 1 ? {} : { revisedPrompt: `revised ${i}` }),
    });
  }
  return out;
}

function applyMocks(spec: Spec): void {
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
  // The REAL storage bridges — this route WRITES.
  jest.doMock('@/lib/file-storage/manager', () => jest.requireActual('@/lib/file-storage/manager'));
  jest.doMock('@/lib/file-storage/lantern-store-bridge', () =>
    jest.requireActual('@/lib/file-storage/lantern-store-bridge'),
  );
  jest.doMock('@/lib/mount-index/store-file', () =>
    jest.requireActual('@/lib/mount-index/store-file'),
  );
  jest.doMock('@/lib/files/tag-inheritance', () =>
    jest.requireActual('@/lib/files/tag-inheritance'),
  );
  jest.doMock('@/lib/files/webp-conversion', () =>
    jest.requireActual('@/lib/files/webp-conversion'),
  );
  jest.doMock('@/lib/mount-index/mount-chunk-cache', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/mount-chunk-cache'),
    invalidateMountPoint: jest.fn(),
  }));
  jest.doMock('@/lib/mount-index/embedding-scheduler', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/mount-index/embedding-scheduler'),
    enqueueEmbeddingJobsForMountPoint: jest.fn().mockResolvedValue(undefined),
  }));
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
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

  // The image provider seam — behavioural + recording (see the header).
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createImageProvider: (provider: string, baseUrl?: string) => {
        // v4's factory THROWS for a provider with no `imageGeneration`
        // capability; the route's bare catch turns that into its one 400.
        if (provider === 'OLLAMA' || provider === 'ANTHROPIC' || provider === 'DEEPSEEK') {
          throw new Error(`Provider '${provider}' does not support image generation`);
        }
        return {
          generateImage: async (params: Record<string, unknown>, apiKey: string) => {
            providerCalls.push({ provider, baseUrl: baseUrl ?? null, apiKey, params });
            if (providerMode === 'throw') {
              throw new Error('canned provider failure');
            }
            const n = Math.max(1, Number(params.n ?? 1));
            return { images: cannedImages(n) };
          },
        };
      },
    };
  });

  // The Concierge classification seam — canned + recording.
  jest.doMock('@/lib/services/dangerous-content/gatekeeper.service', () => {
    const actual = jest.requireActual('@/lib/services/dangerous-content/gatekeeper.service');
    return {
      __esModule: true,
      ...actual,
      classifyContent: async (
        content: string,
        selection: unknown,
        userId: string,
        settings: unknown,
        chatId?: string,
      ) => {
        classifyCalls.push({
          content,
          selection,
          userId,
          settings,
          chatId: chatId ?? null,
        });
        return { ...cannedClassification, source: 'llm', providerName: null };
      },
    };
  });

  jest.doMock('@/lib/services/llm-logging.service', () => ({
    __esModule: true,
    logLLMCall: async () => undefined,
  }));
}

async function dumpTables(spec: Spec): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const main = getRawDatabase() as unknown as { prepare: (s: string) => { all: () => unknown } };
  const mount = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => unknown };
  };
  return {
    // Only the GENERATED rows: the fixture's ten seeded images never move on
    // this route, and listing them would bury the one or three rows a case mints.
    files: main
      .prepare(
        "SELECT id, userId, sha256, originalFilename, mimeType, size, width, height, source, " +
          "category, linkedTo, tags, description, generationPrompt, generationModel, " +
          "generationRevisedPrompt, storageKey, fileStatus FROM files " +
          "WHERE source = 'GENERATED' ORDER BY originalFilename",
      )
      .all(),
    links: mount
      .prepare(
        'SELECT l.relativePath, l.fileName, l.originalMimeType, f.sha256, f.fileSizeBytes, ' +
          'f.fileType FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId ' +
          'WHERE l.mountPointId = ? ORDER BY l.relativePath',
      )
      .all(spec.lanternMountPointId),
  };
}

function buildCases(): CaseSpec[] {
  const prompt = 'A brass observatory at dusk';
  return [
    // ── the happy path ──────────────────────────────────────────────────────
    { name: 'generate_ok', body: { prompt, profileId: PROFILE_MAIN } },
    // `options.n = 3` — three files, `_0_` / `_1_` / `_2_`, each with its own
    // sha8 because the canned bytes differ per index. Index 1 carries no
    // revisedPrompt, so the receipt's undefined-drop is measured here too.
    {
      name: 'generate_count3',
      body: { prompt, profileId: PROFILE_MAIN, options: { n: 3 } },
    },
    // The five `options` overrides that reach the shared params builder.
    {
      name: 'generate_options_all',
      body: {
        prompt,
        profileId: PROFILE_MAIN,
        options: {
          n: 1,
          size: '512x768',
          quality: 'hd',
          style: 'vivid',
          aspectRatio: '2:3',
        },
      },
    },
    // Tags → `linkedTo`, the inherited-tag merge, and the receipt echo (the
    // PARSED objects: unknown keys stripped, `tagType` then `tagId`).
    {
      name: 'generate_with_tags',
      body: {
        prompt,
        profileId: PROFILE_MAIN,
        tags: [{ tagId: CHAR_TAG, tagType: 'CHARACTER', extra: 1 }, { tagType: 'THEME', tagId: THEME_TAG }],
      },
    },
    // A PNG from the provider: `convertToWebP` transcodes, so only the POLICY
    // (the stored mime) is comparable — the bytes are sharp's on one side and
    // the harness codec's on the other (D19).
    {
      name: 'generate_png_transcode',
      body: { prompt, profileId: PROFILE_MAIN },
      provider: 'png',
    },

    // ── the Concierge gate ──────────────────────────────────────────────────
    // DETECT_ONLY + dangerous: classified, logged, but NOT rerouted.
    //
    // ⚠ The mode must be one of v4's THREE (`OFF` / `DETECT_ONLY` /
    // `AUTO_ROUTE`) and every other field in range. MEASURED while writing this
    // corpus: a `chat_settings` row that fails `ChatSettingsSchema` is DROPPED
    // WHOLE by v4's `findByFilter` re-validation (`base.repository.ts:277-285`),
    // so `findByUserId` answers null and the Concierge silently falls back to
    // `DEFAULT_DANGEROUS_CONTENT_SETTINGS` (mode OFF, no scan) — proven with a
    // VALID `mode: 'AUTO_ROUTE'` and an out-of-range `threshold: 5`, which also
    // produced zero classification calls. v5's
    // `db::chat_settings::find_by_user_id` runs no such validation and would
    // read the bad row through. That divergence is REAL and PRE-EXISTING; it
    // belongs to the chat-settings repository, not to this route, and is
    // recorded in the P4.76 lane record rather than pinned here.
    {
      name: 'generate_danger_detect_only',
      body: { prompt, profileId: PROFILE_MAIN },
      danger: { mode: 'DETECT_ONLY' },
      classify: DANGEROUS,
    },
    // AUTO_ROUTE + dangerous: rerouted to the FIRST `isDangerousCompatible`
    // profile — which is NOT the Concierge desk's `uncensoredImageProfileId`
    // (deliberately pointed elsewhere here, so a port that read the desk fails).
    {
      name: 'generate_danger_autoroute',
      body: { prompt, profileId: PROFILE_MAIN },
      danger: {
        mode: 'AUTO_ROUTE',
        uncensoredImageProfileId: PROFILE_NOIMAGE,
      },
      classify: DANGEROUS,
    },
    // AUTO_ROUTE + dangerous, but the REQUESTED profile is the only compatible
    // one — `p.id !== profile.id` excludes it, so the warn arm runs and the
    // original is kept.
    {
      name: 'generate_danger_autoroute_no_target',
      body: { prompt, profileId: PROFILE_UNCENSORED },
      danger: { mode: 'AUTO_ROUTE' },
      classify: DANGEROUS,
    },
    // AUTO_ROUTE but NOT dangerous: classified, no reroute.
    {
      name: 'generate_danger_autoroute_safe',
      body: { prompt, profileId: PROFILE_MAIN },
      danger: { mode: 'AUTO_ROUTE' },
      classify: SAFE,
    },
    // mode OFF: NO classification call at all (the recordings prove it).
    {
      name: 'generate_danger_off',
      body: { prompt, profileId: PROFILE_MAIN },
      danger: { mode: 'OFF' },
      classify: DANGEROUS,
    },
    // scanImagePrompts false: the second conjunct — also no call.
    {
      name: 'generate_scan_disabled',
      body: { prompt, profileId: PROFILE_MAIN },
      danger: { mode: 'AUTO_ROUTE', scanImagePrompts: false },
      classify: DANGEROUS,
    },

    // ── the refusals ────────────────────────────────────────────────────────
    { name: 'generate_profile_missing', body: { prompt, profileId: MISSING_PROFILE } },
    // v4's factory throws → the route's ONE 400, naming the profile's provider.
    { name: 'generate_provider_no_images', body: { prompt, profileId: PROFILE_NOIMAGE } },
    // A provider throw is NOT caught: the middleware's flat 500, nothing written.
    {
      name: 'generate_provider_throws',
      body: { prompt, profileId: PROFILE_MAIN },
      provider: 'throw',
    },
    // `throw new Error('Generated image has no data')` inside the Promise.all.
    {
      name: 'generate_no_image_data',
      body: { prompt, profileId: PROFILE_MAIN },
      provider: 'nodata',
    },
    // The Lantern mount is unprovisioned → v4 throws rather than leaking bytes
    // into `_general/`.
    {
      name: 'generate_lantern_unprovisioned',
      body: { prompt, profileId: PROFILE_MAIN },
      dropLantern: true,
    },

    // ── `generateImageSchema` (every arm answers `Validation error` 400) ─────
    { name: 'zod_prompt_missing', body: { profileId: PROFILE_MAIN } },
    { name: 'zod_prompt_empty', body: { prompt: '', profileId: PROFILE_MAIN } },
    { name: 'zod_prompt_too_long', body: { prompt: 'x'.repeat(4001), profileId: PROFILE_MAIN } },
    { name: 'zod_prompt_wrong_type', body: { prompt: 42, profileId: PROFILE_MAIN } },
    { name: 'zod_profile_missing', body: { prompt } },
    { name: 'zod_profile_not_uuid', body: { prompt, profileId: 'not-a-uuid' } },
    { name: 'zod_profile_null', body: { prompt, profileId: null } },
    { name: 'zod_body_not_object', body: [1, 2, 3] },
    { name: 'zod_count_zero', body: { prompt, profileId: PROFILE_MAIN, options: { n: 0 } } },
    { name: 'zod_count_over_max', body: { prompt, profileId: PROFILE_MAIN, options: { n: 20 } } },
    {
      name: 'zod_count_fractional',
      body: { prompt, profileId: PROFILE_MAIN, options: { n: 1.5 } },
    },
    {
      name: 'zod_count_string',
      body: { prompt, profileId: PROFILE_MAIN, options: { n: '2' } },
    },
    {
      name: 'zod_quality_bad',
      body: { prompt, profileId: PROFILE_MAIN, options: { quality: 'ultra' } },
    },
    {
      name: 'zod_style_bad',
      body: { prompt, profileId: PROFILE_MAIN, options: { style: 'painterly' } },
    },
    {
      name: 'zod_size_wrong_type',
      body: { prompt, profileId: PROFILE_MAIN, options: { size: 512 } },
    },
    // `.optional()` is not `.nullable()`: an explicit null refuses.
    { name: 'zod_options_null', body: { prompt, profileId: PROFILE_MAIN, options: null } },
    { name: 'zod_options_not_object', body: { prompt, profileId: PROFILE_MAIN, options: 7 } },
    { name: 'zod_tags_null', body: { prompt, profileId: PROFILE_MAIN, tags: null } },
    {
      name: 'zod_tags_raw_tagid',
      body: { prompt, profileId: PROFILE_MAIN, tags: [{ tagType: 'THEME', tagId: 5 }] },
    },
    {
      name: 'zod_tags_bad_tagtype',
      body: { prompt, profileId: PROFILE_MAIN, tags: [{ tagType: 'PERSON', tagId: THEME_TAG }] },
    },
    { name: 'zod_tags_not_array', body: { prompt, profileId: PROFILE_MAIN, tags: 'nope' } },
  ];
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  providerCalls = [];
  classifyCalls = [];
  providerMode = c.provider ?? 'webp';
  cannedClassification = c.classify ?? SAFE;
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'imggen-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();

  if (c.danger) {
    // The stored bag merged with this case's overrides, written back raw. The
    // fixture stores v4's `mode: 'OFF'` default, and the resolver reads
    // `chatSettings.dangerousContentSettings`.
    const rows = (await rawQuery(
      'SELECT dangerousContentSettings FROM chat_settings WHERE userId = ?',
      [spec.userId],
    )) as Array<{ dangerousContentSettings: string }>;
    const current = JSON.parse(rows[0].dangerousContentSettings) as Record<string, unknown>;
    await rawQuery('UPDATE chat_settings SET dangerousContentSettings = ? WHERE userId = ?', [
      JSON.stringify({ ...current, ...c.danger }),
      spec.userId,
    ]);
  }
  if (c.dropLantern) {
    await rawQuery('DELETE FROM instance_settings WHERE key = ?', [
      'lanternBackgroundsMountPointId',
    ]);
  }

  const RealDate = Date;
  const frozen = spec.frozenNowMs;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(frozen);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return frozen;
    }
  } as unknown as DateConstructor;

  try {
    const route = (await import('@/app/api/v1/images/route')) as {
      POST: (req: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
    };
    const resp = await route.POST(mockGeneratePost(c.body));
    const body = await resp.json();
    return {
      name: c.name,
      status: resp.status,
      body,
      providerCalls,
      classifyCalls,
      tables: await dumpTables(spec),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON output');
  const fixtureMain = process.env.QT_FIXTURE_IMGCOL_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_IMGCOL_MOUNT;
  if (!fixtureMain || !fixtureMount) {
    throw new Error('QT_FIXTURE_IMGCOL_MAIN and QT_FIXTURE_IMGCOL_MOUNT must be set');
  }

  const specPath = join(__dirname, '..', 'fixtures', 'images-collection.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const scratch = mkdtempSync(join(tmpdir(), 'qt-images-generate-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of buildCases()) {
    outLines.push(
      JSON.stringify(await runCase(spec, c, scratch, { main: fixtureMain, mount: fixtureMount })),
    );
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`images-generate oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('images-generate-route oracle', async () => {
  await main();
});
