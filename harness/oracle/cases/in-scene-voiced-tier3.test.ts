/**
 * @jest-environment node
 *
 * P4.D180 IN-SCENE VOICE-REHEARSAL tier-3 (mocked-LLM) ORACLE: drives v4's REAL
 * `generateInSceneVoicedLine` (`lib/services/announcer/in-scene-voiced.ts`) AND
 * v4's REAL `handleImpersonationVoicePreview`
 * (`app/api/v1/chats/[id]/actions/impersonation-voice-preview.ts`) over a FRESH
 * copy of the committed `in-scene-voiced-{main,mount}.db` fixture per case, with
 * the provider boundary canned, and emits:
 *
 *   - the `InSceneVoicedLineResult` (service rows) or the status + body
 *     (action rows),
 *   - the RECORDED provider call — `provider|model|temperature|messages` AND
 *     `maxTokens`. The assembled system prompt, the shaped transcript and the
 *     user message ARE the pinned evidence, and the Rust
 *     `CannedCompletionProvider` is keyed on exactly the first string
 *     (memory note `tier3-completion-oracle`). **`maxTokens` is recorded as its
 *     own field precisely because it is NOT in that key** — a wrong ceiling is
 *     otherwise invisible (measured on the announcer family: 2048 → 99 stays
 *     green).
 *   - a post-run `memories` dump, because this "persists nothing" service DOES
 *     write: `searchMemoriesSemantic` bumps `lastAccessedAt` on whatever it
 *     returned. Clock-free (`bumped` is `lastAccessedAt > the seed timestamp`).
 *
 * No embeddings are seeded, so the vector pool is empty on both sides and the
 * search falls through to the deterministic text path. `generateEmbeddingForUser`
 * is canned anyway so the query-embed step cannot reach the network.
 *
 * `logLLMCall` is no-op'd wholesale by `jest.setup`
 * (`jest-setup-llm-logging-service-mocked`), so the rehearsal's `llm_logs` row
 * is NOT a comparand here — it is proven by the web-venue wire test instead.
 *
 * **Must run under `TZ=UTC`** (the house convention for clock-touching oracles).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-isv-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/in-scene-voiced-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/in-scene-voiced.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ISV_MAIN=$V5W/crates/quilltap-web/tests/fixtures/in-scene-voiced-main.db \
 *   QT_FIXTURE_ISV_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/in-scene-voiced-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-in-scene-voiced.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- in-scene-voiced-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  userIdNoDefault: string;
  seedTimestamp: string;
  frozenNowMs: number;
}

const VESPER = 'a1000000-0000-4000-8000-000000000001';

const CONN_SEAT = 'c0000000-0000-4000-8000-000000000001';
const CONN_OVERRIDE = 'c0000000-0000-4000-8000-000000000002';

const CHAT_STACK = 'c1000000-0000-4000-8000-000000000001';
const CHAT_NOSTACK = 'c1000000-0000-4000-8000-000000000002';
const CHAT_NOHIST = 'c1000000-0000-4000-8000-000000000003';
const CHAT_UNC = 'c1000000-0000-4000-8000-000000000004';
const CHAT_UNC_OK = 'c1000000-0000-4000-8000-000000000005';
const CHAT_NOPROF = 'c1000000-0000-4000-8000-000000000006';
const MISSING_CHAT = '99999999-9999-4999-8999-999999999999';

const P_VESPER = 'e1000000-0000-4000-8000-000000000001';
const P_BRAM = 'e1000000-0000-4000-8000-000000000002';
const P_ESME = 'e1000000-0000-4000-8000-000000000005';
const P_GHOST = 'e1000000-0000-4000-8000-000000000007';
const P_MISSING = 'e1000000-0000-4000-8000-0000000000de';

const SP_DEFAULT = '52000000-0000-4000-8000-000000000002';

/** The canned rewrite. The failure case returns an empty string instead. */
const REWRITE =
  '*She sets the ledger down, square to the edge of the table.* "The lamps are out, ' +
  'and the fault is oil, not idleness. I will have them lit by the second bell."';

const cannedRecorded = new Map<
  string,
  {
    provider: string;
    model: string;
    temperature: number | null;
    maxTokens: number | null;
    messages: Array<{ role: string; content: string }>;
    response: string;
  }
>();

/** Flipped per case to drive the empty-response (failure) arm. */
let respondEmpty = false;

/** The frozen clock (matches NOW_MS in the Rust test). */
const NOW_MS = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

function applyMocks(spec: Spec, sessionUserId: string): void {
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
  // Without this the vault bridge is mocked to a fake mount and EVERY vault read
  // lists an empty folder — the subprompt would silently never resolve
  // (`jest-oracle-character-vault-bridge-is-mocked`).
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));

  // The provider boundary: record every call keyed exactly as the Rust
  // CannedCompletionProvider looks it up, PLUS the maxTokens the key omits.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        sendMessage: async (
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
            maxTokens?: number;
          },
          _apiKey: string,
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const response = respondEmpty ? '' : REWRITE;
          const temperature = (params.temperature as number | undefined) ?? null;
          const maxTokens = (params.maxTokens as number | undefined) ?? null;
          const key = `${provider}|${params.model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature,
              maxTokens,
              messages,
              response,
            });
          }
          return { content: response, finishReason: 'stop', usage: undefined };
        },
      }),
    };
  });
  jest.doMock('@/lib/plugins/provider-validation', () => {
    const actual = jest.requireActual('@/lib/plugins/provider-validation');
    return { __esModule: true, ...actual, requiresApiKey: () => false };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
      getApiKeyForProfile: async () => 'test-key',
    };
  });
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async () => ({
        embedding: new Float32Array([1, 0, 0, 0]),
        model: 'canned',
        provider: 'canned',
        dimensions: 4,
      }),
    };
  });
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: async () => undefined };
  });
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: sessionUserId } }),
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

async function readMemoryBumps(seedTimestamp: string): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const db = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => Array<Record<string, unknown>> };
  };
  return db
    .prepare(
      'SELECT id, characterId, ' +
        'CASE WHEN lastAccessedAt IS NOT NULL AND lastAccessedAt > ? THEN 1 ELSE 0 END AS bumped ' +
        'FROM memories ORDER BY id',
    )
    .all(seedTimestamp);
}

interface CaseSpec {
  name: string;
  empty?: boolean;
  /** Drive v4's REAL action handler instead of the service directly. */
  viaRoute?: boolean;
  /** The session user — `userIdNoDefault` drives the no-profile 400. */
  noDefaultUser?: boolean;
  chatId: string;
  participantId: string;
  seedMarkdown: unknown;
  connectionProfileId?: unknown;
  systemPromptId?: unknown;
  /** SERVICE rows only: what the caller resolved (the route does this itself). */
  serviceProfileId?: string;
  serviceSystemPromptId?: string | null;
  serviceSubpromptIds?: string[];
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

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  cannedRecorded.clear();
  respondEmpty = c.empty ?? false;
  applyMocks(spec, c.noDefaultUser ? spec.userIdNoDefault : spec.userId);

  const work = mkdtempSync(join(scratch, 'isv-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  // ⚠ v4's `providerRegistry` is initialized by startup, which no jest oracle
  // runs — so `formatMessagesForProvider` would read the NO-PLUGIN fallback and
  // prefix every assistant turn with `[Name]` where production v4 sends the
  // native `name` field instead (`jest-oracle-empty-provider-registry`). v5
  // reads its built-in manifests, which are always populated, so without this
  // the oracle records a v4 behaviour that never occurs and the differential
  // fails against a CORRECT port. Measured, not guessed: the first regen
  // diverged on exactly the seat's own assistant lines.
  {
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    const { createRequire } = await import('node:module');
    const req = createRequire(join(process.cwd(), 'noop.js'));
    const plugins = ['qtap-plugin-openai', 'qtap-plugin-openai-compatible'].map((d) => {
      const m = req(join(process.cwd(), 'plugins', 'dist', d, 'index.js'));
      return m.plugin || m.default?.plugin || m.default;
    });
    await initializeProviderRegistry(plugins);
  }

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  const RealDate = Date;
  const iso = new RealDate(NOW_MS).toISOString();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(iso);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return NOW_MS;
    }
  } as unknown as DateConstructor;

  try {
    if (c.viaRoute) {
      const route = (await import('@/app/api/v1/chats/[id]/route')) as unknown as {
        POST: (...a: unknown[]) => Promise<{ status: number; json: () => Promise<unknown> }>;
      };
      const body: Record<string, unknown> = {
        participantId: c.participantId,
        seedMarkdown: c.seedMarkdown,
      };
      if (c.connectionProfileId !== undefined) body.connectionProfileId = c.connectionProfileId;
      if (c.systemPromptId !== undefined) body.systemPromptId = c.systemPromptId;
      const resp = await route.POST(
        mockRequest(
          `http://localhost/api/v1/chats/${c.chatId}?action=impersonation-voice-preview`,
          body,
        ),
        { params: Promise.resolve({ id: c.chatId }) },
      );
      return {
        name: c.name,
        status: resp.status,
        body: await resp.json(),
        canned: [...cannedRecorded.values()],
        tables: { memories: await readMemoryBumps(spec.seedTimestamp) },
      };
    }

    const { getRepositories } = await import('@/lib/repositories/factory');
    const repos = getRepositories();
    const chat = await repos.chats.findById(c.chatId);
    if (!chat) throw new Error(`fixture chat missing for ${c.name}`);
    const participant = (chat.participants as Array<{ id: string; characterId: string }>).find(
      (p) => p.id === c.participantId,
    );
    if (!participant) throw new Error(`fixture participant missing for ${c.name}`);
    const character = await repos.characters.findById(participant.characterId);
    const profile = await repos.connections.findById(c.serviceProfileId ?? CONN_SEAT);
    if (!character || !profile) throw new Error(`fixture row missing for ${c.name}`);

    const { resolveSelectedSubprompts } = await import('@/lib/subprompts/subprompts');
    const subprompts = await resolveSelectedSubprompts(
      character.id,
      c.serviceSubpromptIds ?? [],
    );

    const { generateInSceneVoicedLine } = await import(
      '@/lib/services/announcer/in-scene-voiced'
    );
    const result = await generateInSceneVoicedLine({
      chat,
      participant,
      character,
      profile,
      seedMarkdown: c.seedMarkdown as string,
      systemPromptId: c.serviceSystemPromptId ?? null,
      subprompts,
      userId: spec.userId,
    });

    return {
      name: c.name,
      result,
      canned: [...cannedRecorded.values()],
      tables: { memories: await readMemoryBumps(spec.seedTimestamp) },
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

/**
 * A draft whose UTF-16 length is TWICE its scalar count — the astral arm.
 *
 * ⚠ It has to be THIS long. The cheap-LLM executor floors every ceiling at 2048
 * (v4 `effectiveMaxTokens`), so a shorter astral draft is floored on both sides
 * and a `chars().count()` port would pass. 3,000 emoji = 6,000 UTF-16 units →
 * 3,000; a scalar port computes 1,500 → floored to 2,048. The two are visibly
 * different at the provider, which is where this is recorded.
 */
const ASTRAL_SEED = '\u{1F4DC}'.repeat(3000);
/** Plain text past the floor: 6,000 units → 3,000. */
const LONG_SEED = 'The lamps are out on the eastern quay. '.repeat(154);
/** Past the 4,096 cap: 12,000 units → 6,000 → capped. */
const HUGE_SEED = 'The lamps are out on the eastern quay. '.repeat(316);

const CASES: CaseSpec[] = [
  // ── SERVICE rows ─────────────────────────────────────────────────────────
  {
    // The full path: the compiled stack is present at the current version, so
    // `subprompts` is passed as NULL even though the seat selected one.
    name: 'service_with_precompiled_stack',
    chatId: CHAT_STACK,
    participantId: P_VESPER,
    seedMarkdown: '  Tell them the lamps are out and I am seeing to it.  ',
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
    serviceSubpromptIds: ['measure'],
  },
  {
    // No stack → the read-through fallback, and the SAME selected subprompt
    // now reaches the builder. Diffing this against the row above is the only
    // way `subprompts: precompiled ? null : subprompts` is measurable.
    name: 'service_without_precompiled_stack',
    chatId: CHAT_NOSTACK,
    participantId: P_VESPER,
    seedMarkdown: '  Tell them the lamps are out and I am seeing to it.  ',
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
    serviceSubpromptIds: ['measure'],
  },
  {
    // A seed matching NO memory → the recall block is absent and nothing is
    // bumped.
    name: 'service_no_recall_hits',
    chatId: CHAT_NOSTACK,
    participantId: P_VESPER,
    seedMarkdown: 'Zzyzx qwertyuiop.',
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
  },
  {
    // `hasHistoryAccess: false` + a Host status announcement mid-transcript:
    // the presence-window leg drops the line from the stretch she was away for.
    name: 'service_presence_windows',
    chatId: CHAT_NOHIST,
    participantId: P_VESPER,
    seedMarkdown: 'The rings are loose and the smith is late.',
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
  },
  {
    // An astral draft: `maxTokensForSeed` counts UTF-16 units.
    name: 'service_astral_seed_budget',
    chatId: CHAT_NOSTACK,
    participantId: P_VESPER,
    seedMarkdown: ASTRAL_SEED,
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
  },
  {
    // Plain text past the executor's 2048 floor — the budget genuinely scales.
    name: 'service_long_seed_budget',
    chatId: CHAT_NOSTACK,
    participantId: P_VESPER,
    seedMarkdown: LONG_SEED,
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
  },
  {
    // …and past the 4096 cap.
    name: 'service_huge_seed_caps',
    chatId: CHAT_NOSTACK,
    participantId: P_VESPER,
    seedMarkdown: HUGE_SEED,
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
  },
  {
    // The empty-completion failure arm.
    name: 'service_empty_completion',
    empty: true,
    chatId: CHAT_NOSTACK,
    participantId: P_VESPER,
    seedMarkdown: 'The lamps are out.',
    serviceProfileId: CONN_SEAT,
    serviceSystemPromptId: SP_DEFAULT,
  },
  // ── ACTION rows — v4's REAL handler through the REAL route ───────────────
  { name: 'route_ok_seat_profile', viaRoute: true, chatId: CHAT_STACK, participantId: P_VESPER, seedMarkdown: 'Tell them the lamps are out and I am seeing to it.' },
  { name: 'route_ok_override_profile', viaRoute: true, chatId: CHAT_STACK, participantId: P_VESPER, seedMarkdown: 'The lamps are out.', connectionProfileId: CONN_OVERRIDE },
  { name: 'route_ok_system_prompt_override', viaRoute: true, chatId: CHAT_STACK, participantId: P_VESPER, seedMarkdown: 'The lamps are out.', systemPromptId: SP_DEFAULT },
  { name: 'route_missing_chat', viaRoute: true, chatId: MISSING_CHAT, participantId: P_VESPER, seedMarkdown: 'The lamps are out.' },
  // ⚠ The chat 404 must beat the body parse: a NON-UUID participantId on a
  // missing chat still answers 404, not the Zod envelope.
  { name: 'route_missing_chat_beats_bad_body', viaRoute: true, chatId: MISSING_CHAT, participantId: 'not-a-uuid', seedMarkdown: '' },
  { name: 'route_zod_blank_seed', viaRoute: true, chatId: CHAT_STACK, participantId: P_VESPER, seedMarkdown: '' },
  { name: 'route_zod_non_string_seed', viaRoute: true, chatId: CHAT_STACK, participantId: P_VESPER, seedMarkdown: 42 },
  { name: 'route_zod_non_uuid_profile', viaRoute: true, chatId: CHAT_STACK, participantId: P_VESPER, seedMarkdown: 'The lamps are out.', connectionProfileId: 'not-a-uuid' },
  // ⚠ The parse must beat the participant find: a non-uuid participantId on a
  // MISSING seat answers the Zod envelope, not 404.
  { name: 'route_zod_beats_missing_participant', viaRoute: true, chatId: CHAT_STACK, participantId: 'not-a-uuid', seedMarkdown: 'The lamps are out.' },
  { name: 'route_missing_participant', viaRoute: true, chatId: CHAT_STACK, participantId: P_MISSING, seedMarkdown: 'The lamps are out.' },
  { name: 'route_absent_seat', viaRoute: true, chatId: CHAT_STACK, participantId: P_ESME, seedMarkdown: 'The lamps are out.' },
  { name: 'route_not_impersonated', viaRoute: true, chatId: CHAT_STACK, participantId: P_BRAM, seedMarkdown: 'The lamps are out.' },
  { name: 'route_missing_character', viaRoute: true, chatId: CHAT_STACK, participantId: P_GHOST, seedMarkdown: 'The lamps are out.' },
  { name: 'route_character_default_profile', viaRoute: true, chatId: CHAT_NOPROF, participantId: P_VESPER, seedMarkdown: 'The lamps are out.' },
  { name: 'route_instance_default_profile', viaRoute: true, chatId: CHAT_NOPROF, participantId: P_BRAM, seedMarkdown: 'The lamps are out.' },
  { name: 'route_no_profile_anywhere', viaRoute: true, noDefaultUser: true, chatId: CHAT_NOPROF, participantId: P_BRAM, seedMarkdown: 'The lamps are out.' },
  { name: 'route_uncensored_reroutes', viaRoute: true, chatId: CHAT_UNC, participantId: P_VESPER, seedMarkdown: 'Say the part you were not going to say.' },
  { name: 'route_uncensored_compatible_does_not_reroute', viaRoute: true, chatId: CHAT_UNC_OK, participantId: P_VESPER, seedMarkdown: 'Say the part you were not going to say.' },
  { name: 'route_failure_default_sentence', viaRoute: true, empty: true, chatId: CHAT_STACK, participantId: P_VESPER, seedMarkdown: 'The lamps are out.' },
];

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`in-scene-voiced oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'in-scene-voiced.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_ISV_MAIN ?? '',
    mount: process.env.QT_FIXTURE_ISV_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-isv-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out: string[] = [];
  for (const c of CASES) {
    out.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, out.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${CASES.length} in-scene-voiced rows to ${outPath}\n`);
}

it('emits the in-scene-voiced tier-3 oracle', async () => {
  await main();
}, 600_000);
