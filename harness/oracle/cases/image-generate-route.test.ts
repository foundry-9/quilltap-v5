/**
 * @jest-environment node
 *
 * P4.6ai IMAGE-GENERATE ROUTE-ENVELOPE ORACLE: drives v4's REAL
 * `POST /api/v1/image-profiles/[id]?action=generate` route handler over a FRESH
 * copy of the shared image-generation two-DB fixture per case, recording each
 * response `{status, body}` so the ported `imageProfileGenerate` un-refusal
 * (`api::image_profiles::image_profile_generate` over the injected W4.9a runner)
 * can be diffed field-by-field on the `{success, data, expandedPrompt, metadata}`
 * envelope. This is the ROUTE wrapper's differential (the deep save + store writes
 * are separately pinned by `image-generation.test.ts`); the corpus keeps prompts
 * PLACEHOLDER-FREE + danger OFF, so NO cheap-LLM/completion call fires — only the
 * image provider is canned.
 *
 * MODEL SEAMS (the `image_generation_tier3` mold — pinned identically both sides):
 *   - `createImageProvider` (`@/lib/llm/plugin-factory`) → a fake provider whose
 *     `generateImage(params, key)` KEYS on `provider|model|JSON.stringify(params)`,
 *     RECORDS the key (`kind:"cannedImage"` row) so the Rust CannedImageProvider
 *     replays the SAME key, returning `params.n` canned images (revisedPrompt echoes
 *     the prompt) — so the `count` > 1 path threads end-to-end.
 *   - `getImageGenerationModels` + `getImageProviderConstraints`
 *     (`@/lib/plugins/provider-registry`) → canned OPENAI size-strategy support
 *     (identical to the Rust `orientation_data_for` closure; the route passes NO
 *     orientation → the square default, size 1024x1024).
 *   - `convertToWebP` + `transcodeToWebP` → PASS-THROUGH (bytes/mime/filename
 *     unchanged) so the store bytes match the Rust PassthroughTranscoder.
 *   - `findApiKeyByIdAndUserId` → the canned apiKeyId->key map.
 *   - moderation registry `getDefaultProvider` → null (unused: danger OFF).
 *   - `logLLMCall` → no-op (the route envelope is fire-and-forget-independent; the
 *     Rust side writes its own scratch llm-logs, never diffed here).
 *   - the Lantern store bridge + character-vault bridge are UN-MOCKED so the REAL
 *     `getLanternBackgroundsStore` resolves the provisioned mount (the save lands).
 *   - Date.now() frozen (spec.frozenNowMs) so the provider filename
 *     `generated_<ts>.<ext>` is pinned.
 *
 *   - `executeImageGenerationTool` (`@/lib/tools/handlers/image-generation-handler`)
 *     → WRAPPED, not replaced (P4.96): the real handler still runs, and the input
 *     object the route assembled from its validated body is recorded verbatim.
 *     The route envelope carries none of v4's five shaping fields, so this is the
 *     only comparand that can see them.
 *
 * Emits one NDJSON line per RECORDED canned image call (kind:"cannedImage"), one
 * per case (kind:"result", { name, status, body }), and one per case
 * (kind:"toolInput", { name, input }) — `input` is `null` when the route refused
 * before the tool. The minted `files.id` (+ its `/api/v1/images|files/<id>`
 * url/filepath) is uuid-normalized in the harness.
 *
 * Run (Node 24, from the v4 checkout; stage OUTSIDE any .claude path — v4's jest
 * ignores /\.claude/; multi-case → --testTimeout=120000):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-image-generation-fixture.ts
 *   TMPO=/tmp/qt-imggenroute-oracle; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $WT/harness/oracle/cases/image-generate-route.test.ts "$TMPO/cases/"
 *   cp $WT/harness/oracle/fixtures/image-generation.json     "$TMPO/fixtures/"
 *   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-image-generate-route.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "image-generate-route.test"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
  userId: string;
  profileId: string;
  apiKeys: Record<string, string>;
}

// The shared fixture's chat ids (built by build-image-generation-fixture.ts).
const CHAT_PLAIN = 'aaaa0001-0000-4000-8000-000000000001';
const CHAT_ORIENT = 'aaaa0002-0000-4000-8000-000000000002';
const BOGUS_PROFILE = 'e0000000-0000-4000-8000-0000000000ff';

// P4.85 item 5. `z.string().min(1).max(4000)` counts CODE POINTS since Zod 4.5.
// 3983 + 16 BMP + one astral = 4000 code points but 4001 UTF-16 units — the
// exact string on which a UTF-16 count and a code-point count disagree. Built,
// never spelled, and built the SAME way on both sides.
const ASTRAL_AT_MAX = `A moonlit tower ${'a'.repeat(3983)}\u{1F702}`;
const ASTRAL_OVER_MAX = `A moonlit tower ${'a'.repeat(3984)}\u{1F702}`;

interface Case {
  name: string;
  id: string;
  body: Record<string, unknown>;
}

// ---- OrientationSupport (identical to the Rust orientation_data_for) ----
const OPENAI_SUPPORT = {
  strategy: 'size',
  portrait: { size: '1024x1792' },
  landscape: { size: '1792x1024' },
  square: { size: '1024x1024' },
};

/** P4.D138: a canned per-model `loraSupport`, so the shared params builder's
 *  cap + trigger-phrase append are MEASURABLE on this path. Two adapters, no
 *  scale block (the host's DEFAULT_LORA_SCALE applies). */
const CANNED_LORA_SUPPORT = { maxLoras: 2, sourceKinds: ['url', 'hf-repo'] };
function modelsFor(
  provider: string,
): Array<{ id: string; orientationSupport: unknown; loraSupport?: unknown }> | null {
  if (provider === 'OPENAI')
    return [{ id: 'dall-e-3', orientationSupport: OPENAI_SUPPORT, loraSupport: CANNED_LORA_SUPPORT }];
  return null;
}
function constraintsFor(provider: string): { orientationSupport: unknown } | null {
  if (provider === 'OPENAI') return { orientationSupport: OPENAI_SUPPORT };
  return null;
}

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

const PNG_B64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M8AAAMBAQAY3Y2wAAAAAElFTkSuQmCC';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'image-generation.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_IMGGEN_MAIN;
  const mountFixture = process.env.QT_FIXTURE_IMGGEN_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_IMGGEN_MAIN and QT_FIXTURE_IMGGEN_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const cases: Case[] = [
    // Happy path tied to a chat, count 1.
    { name: 'generate_happy_chat', id: spec.profileId, body: { prompt: 'A serene mountain lake at dawn, mist over the water', chatId: CHAT_PLAIN, count: 1 } },
    // No chat (linkedTo = [], no Lantern post) — the chatId-absent envelope.
    { name: 'generate_no_chat', id: spec.profileId, body: { prompt: 'A quiet forest path', count: 1 } },
    // count > 1 — the provider returns 2 images; metadata.count = 2.
    { name: 'generate_count2', id: spec.profileId, body: { prompt: 'A tall waterfall in a canyon', chatId: CHAT_ORIENT, count: 2 } },
    // Missing profile → notFound('Image profile') 404.
    { name: 'generate_profile_404', id: BOGUS_PROFILE, body: { prompt: 'anything', count: 1 } },
    // The route's OWN schema (`generateImageSchema.parse`), after the 404 and
    // before the tool: `count` over its max → the context handler's
    // `validationError` (400). v5 used to hand this to the tool's schema.
    { name: 'generate_count_over_max', id: spec.profileId, body: { prompt: 'A lighthouse at night', count: 20 } },
    // …and an EMPTY prompt fails `z.string().min(1)` at the same gate.
    { name: 'generate_prompt_empty', id: spec.profileId, body: { prompt: '', count: 1 } },
    // P4.85 item 5 — the ASTRAL boundary. Zod ≥ 4.5 measures `.max(4000)` in
    // CODE POINTS, so this prompt (3999 BMP + one astral = 4000 code points,
    // 4001 UTF-16 units) is ACCEPTED and runs the tool. v5 counted UTF-16
    // units here where its sibling `images_generate` route already counted
    // code points, so the two disagreed on exactly this string.
    { name: 'generate_prompt_astral_at_max', id: spec.profileId, body: { prompt: ASTRAL_AT_MAX, count: 1 } },
    // One code point over: refused at the same gate, in code points too — so
    // the fix cannot be "stop counting".
    { name: 'generate_prompt_astral_over_max', id: spec.profileId, body: { prompt: ASTRAL_OVER_MAX, count: 1 } },

    // ------------------------------------------------------------------
    // P4.96 — v4's five optional shaping fields (`generateImageSchema`
    // `route.ts:25-29`), threaded into `executeImageGenerationTool`
    // (`:246-255`). Each happy row sets ONE field so the `toolInput`
    // side-channel row below names exactly which key moved; the `all_five`
    // row proves they compose. v5 carried NONE of them until this lane —
    // the variant stopped at `prompt/chatId/count`.
    // ------------------------------------------------------------------
    { name: 'generate_size', id: spec.profileId, body: { prompt: 'A cliffside monastery', count: 1, size: '1024x1536' } },
    { name: 'generate_quality_hd', id: spec.profileId, body: { prompt: 'A brass orrery', count: 1, quality: 'hd' } },
    // `d8d2890ee` widened this key from `z.enum(['standard','hd'])` to the
    // shared eight-tier `imageQualitySchema`; `max` is only reachable through
    // that widening.
    { name: 'generate_quality_max', id: spec.profileId, body: { prompt: 'A glass conservatory', count: 1, quality: 'max' } },
    { name: 'generate_style_natural', id: spec.profileId, body: { prompt: 'A harbour at slack tide', count: 1, style: 'natural' } },
    { name: 'generate_aspect_ratio_ok', id: spec.profileId, body: { prompt: 'A long viaduct', count: 1, aspectRatio: '16:9' } },
    // MEASURED, not assumed: the ROUTE's `aspectRatio` is `z.string()` — any
    // string — while the TOOL's is `z.enum(['1:1','3:4','4:3','9:16','16:9'])`
    // (`lib/tools/image-generation-tool.ts:57-60`). So `3:2` passes the route,
    // reaches `executeImageGenerationTool` (the toolInput row proves it), and
    // is refused there by the tool's own `safeParse` — reported through v4's
    // one blanket sentence, which names `prompt` and means "the whole object".
    // The route must NOT pre-gate it, or this row would 400 with the Zod
    // envelope instead.
    { name: 'generate_aspect_ratio_off_tool_enum', id: spec.profileId, body: { prompt: 'A long viaduct', count: 1, aspectRatio: '3:2' } },
    { name: 'generate_negative_prompt', id: spec.profileId, body: { prompt: 'A milliner at work', count: 1, negativePrompt: 'no hats' } },
    { name: 'generate_all_five', id: spec.profileId, body: { prompt: 'A zeppelin over the fens', chatId: CHAT_PLAIN, count: 1, size: '1024x1536', quality: 'high', style: 'vivid', aspectRatio: '16:9', negativePrompt: 'no wires' } },

    // ---- the refusal arms: `.optional()` is NOT `.nullable()`, `z.string()`
    // rejects every non-string, `z.enum`/`imageQualitySchema` reject an
    // unknown member. v5 answered 201 having silently ignored the key.
    { name: 'generate_quality_unknown', id: spec.profileId, body: { prompt: 'A kite', count: 1, quality: 'ultra' } },
    { name: 'generate_style_unknown', id: spec.profileId, body: { prompt: 'A kite', count: 1, style: 'sketch' } },
    { name: 'generate_size_number', id: spec.profileId, body: { prompt: 'A kite', count: 1, size: 1024 } },
    { name: 'generate_size_null', id: spec.profileId, body: { prompt: 'A kite', count: 1, size: null } },
    { name: 'generate_negative_prompt_object', id: spec.profileId, body: { prompt: 'A kite', count: 1, negativePrompt: {} } },
    { name: 'generate_aspect_ratio_bool', id: spec.profileId, body: { prompt: 'A kite', count: 1, aspectRatio: true } },
    // The PRE-EXISTING serde-typed class, recorded as it stands: `count` is
    // `Option<i64>` on the dispatch variant, so the WEB EDGE refuses a string
    // before the handler is reached. The handler itself reproduces v4.
    { name: 'generate_count_string', id: spec.profileId, body: { prompt: 'A kite', count: '2' } },
    // `chatId: z.uuid().optional()` — v5 never gated it at all.
    { name: 'generate_chat_id_not_uuid', id: spec.profileId, body: { prompt: 'A kite', count: 1, chatId: 'not-a-uuid' } },
    // Zod collects EVERY failing key into one issue array, in schema order
    // (`quality` is declared before `style`).
    { name: 'generate_two_bad_fields', id: spec.profileId, body: { prompt: 'A kite', count: 1, quality: 'ultra', style: 'sketch' } },
    // The remaining `count` arms of `z.int().min(1).max(10)`: under the floor,
    // and a non-integer (Zod 4 reports the int check as its own issue kind).
    { name: 'generate_count_zero', id: spec.profileId, body: { prompt: 'A kite', count: 0 } },
    { name: 'generate_count_fractional', id: spec.profileId, body: { prompt: 'A kite', count: 1.5 } },
    // `prompt: z.string()` against a number — the handler's own arm. (The
    // dispatch variant types `prompt` as a required `String`, so the WEB EDGE
    // refuses this earlier; the recorded pre-existing narrowing, unchanged.)
    { name: 'generate_prompt_number', id: spec.profileId, body: { prompt: 5, count: 1 } },
    // …and across three DIFFERENT issue kinds, in the schema's DECLARATION
    // order (prompt, chatId, count, size, quality, style, aspectRatio,
    // negativePrompt) — not the body's key order, which is reversed here.
    { name: 'generate_three_bad_ordered', id: spec.profileId, body: { size: 1, count: 99, prompt: '' } },
    // Guard order: v4 reads the body only AFTER `findById`, so a missing
    // profile with a garbage body is a 404, never a 400.
    { name: 'generate_404_beats_400', id: BOGUS_PROFILE, body: { prompt: 5, quality: 'ultra', size: null } },
  ];

  const lines: string[] = [];
  const RealDate = Date;
  // P4.96: the input `executeImageGenerationTool` was handed for the case in
  // flight (reset per case; `null` when the route refused before the tool).
  let recordedToolInput: unknown = null;
  const recordedImages = new Map<
    string,
    { provider: string; model: string; key: string; images: Array<{ data: string; mimeType?: string; revisedPrompt?: string }> }
  >();

  for (const c of cases) {
    recordedToolInput = null;
    const scratch = mkdtempSync(join(tmpdir(), 'qt-imggenroute-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main.db');
    const mountWork = join(scratch, 'mount.db');
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
    jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
    // Auth + readiness (the createAuthenticatedParamsHandler middleware).
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
    // The REAL Lantern store bridge + character-vault bridge (so the save lands).
    jest.doMock('@/lib/file-storage/lantern-store-bridge', () =>
      jest.requireActual('@/lib/file-storage/lantern-store-bridge'),
    );
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );

    // Image provider seam (record the exact key + return `params.n` canned images).
    jest.doMock('@/lib/llm/plugin-factory', () => {
      const actual = jest.requireActual('@/lib/llm/plugin-factory');
      return {
        __esModule: true,
        ...actual,
        createImageProvider: (provider: string) => ({
          generateImage: async (params: Record<string, unknown>, _key: string) => {
            const key = `${provider}|${params.model}|${JSON.stringify(params)}`;
            const prompt = String(params.prompt ?? '');
            const n = Math.max(1, Number(params.n ?? 1));
            const images = Array.from({ length: n }, () => ({
              data: PNG_B64,
              mimeType: 'image/png',
              revisedPrompt: `revised: ${prompt.slice(0, 48)}`,
            }));
            if (!recordedImages.has(key)) {
              recordedImages.set(key, { provider, model: String(params.model), key, images });
            }
            return { images };
          },
        }),
      };
    });

    // Orientation registry seam.
    jest.doMock('@/lib/plugins/provider-registry', () => {
      const actual = jest.requireActual('@/lib/plugins/provider-registry');
      return {
        __esModule: true,
        ...actual,
        getImageGenerationModels: (name: string) => modelsFor(name),
        getImageProviderConstraints: (name: string) => constraintsFor(name),
      };
    });

    // WebP transcode seams (pass-through both).
    jest.doMock('@/lib/files/webp-conversion', () => ({
      __esModule: true,
      convertToWebP: async (buffer: Buffer, mimeType: string, filename: string) => ({
        buffer,
        mimeType,
        filename,
        width: null,
        height: null,
      }),
    }));
    jest.doMock('@/lib/mount-index/blob-transcode', () => {
      const actual = jest.requireActual('@/lib/mount-index/blob-transcode');
      const { sha256OfBuffer } = jest.requireActual('@/lib/utils/sha256');
      return {
        __esModule: true,
        ...actual,
        transcodeToWebP: async (data: Buffer, originalMimeType: string) => ({
          data,
          storedMimeType: originalMimeType,
          sizeBytes: data.length,
          sha256: sha256OfBuffer(data),
        }),
      };
    });

    jest.doMock('@/lib/services/api-key.service', () => {
      const actual = jest.requireActual('@/lib/services/api-key.service');
      return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-cheap-key' };
    });
    // The route envelope never depends on the fire-and-forget llm-logs write — no-op it
    // so the oracle needs no llm-logs partition (the Rust side writes its own, undiffed).
    jest.doMock('@/lib/services/llm-logging.service', () => ({
      __esModule: true,
      logLLMCall: async () => undefined,
    }));
    // P4.96 SIDE CHANNEL (the P4.92 shape): the route envelope alone cannot
    // see the five shaping fields — `{success, data, expandedPrompt, metadata}`
    // carries none of them. So the tool entry point is WRAPPED, not replaced:
    // the real `executeImageGenerationTool` still runs (every envelope row is
    // unchanged by this mock), and the input object the route assembled is
    // recorded verbatim as a `kind:"toolInput"` row. That object is exactly
    // what the Rust `ImageGenerationToolInput` must carry.
    jest.doMock('@/lib/tools/handlers/image-generation-handler', () => {
      const actual = jest.requireActual('@/lib/tools/handlers/image-generation-handler');
      return {
        __esModule: true,
        ...actual,
        executeImageGenerationTool: async (input: unknown, context: unknown) => {
          recordedToolInput = JSON.parse(JSON.stringify(input ?? null));
          return actual.executeImageGenerationTool(input, context);
        },
      };
    });
    jest.doMock('@/lib/plugins/moderation-provider-registry', () => ({
      __esModule: true,
      moderationProviderRegistry: {
        isInitialized: () => true,
        getAllProviders: () => [],
        getDefaultProvider: () => null,
      },
    }));

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRepositories } = await import('@/lib/repositories/factory');

    await initializeDatabase();
    const repos = getRepositories();
    (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, userId: string) => {
      const key = spec.apiKeys[id];
      if (!key) return null;
      return { id, userId, label: 'canned', provider: 'OPENAI', key_value: key, isActive: true, createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z' };
    };

    // Freeze Date.now() so the provider filename `generated_<ts>.<ext>` is pinned.
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
      const route = (await import('@/app/api/v1/image-profiles/[id]/route')) as {
        POST: (req: unknown, ctx: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
      };
      const url = `http://localhost/api/v1/image-profiles/${c.id}?action=generate`;
      const resp = await route.POST(mockRequest(url, c.body), { params: Promise.resolve({ id: c.id }) });
      const body = await resp.json();
      lines.push(JSON.stringify({ kind: 'result', name: c.name, status: resp.status, body }));
      lines.push(JSON.stringify({ kind: 'toolInput', name: c.name, input: recordedToolInput }));
    } finally {
      global.Date = RealDate;
      await new Promise((resolve) => setTimeout(resolve, 50));
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  for (const entry of recordedImages.values()) lines.push(JSON.stringify({ kind: 'cannedImage', ...entry }));

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`image-generate-route oracle wrote ${outPath} (${lines.length} lines)\n`);
}

test('image-generate-route oracle', async () => {
  await main();
});
