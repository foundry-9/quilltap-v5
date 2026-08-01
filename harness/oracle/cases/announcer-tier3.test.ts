/**
 * @jest-environment node
 *
 * P4.9E2A ANNOUNCER tier-3 (mocked-LLM) ORACLE: drives v4's REAL
 * `generateCharacterVoicedAnnouncement` (`lib/services/announcer/
 * character-voiced.ts`) over a FRESH copy of the committed post-office fixture
 * per case, with the provider boundary canned, and emits:
 *
 *   - the `CharacterVoicedAnnouncementResult`,
 *   - the RECORDED provider call (`provider|model|temperature|messages`) — the
 *     assembled system prompt + user message ARE the pinned evidence, and the
 *     Rust `CannedCompletionProvider` is keyed on exactly this string
 *     (memory note `tier3-completion-oracle`),
 *   - a post-run `memories` dump, because this "persists nothing" service DOES
 *     write: `searchMemoriesSemantic` bumps `lastAccessedAt` on whatever it
 *     returned. The dump is clock-free (`bumped` is `lastAccessedAt > the seed
 *     timestamp`), so it diffs without normalization.
 *
 * No embeddings are seeded, so the vector pool is empty on both sides and the
 * search falls through to the deterministic text path. `generateEmbeddingForUser`
 * is canned anyway so the query-embed step cannot reach the network.
 *
 * **Must run under `TZ=UTC`** (the house convention for clock-touching oracles).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-ann-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/announcer-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/post-office-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PO_MAIN=$V5W/crates/quilltap-web/tests/fixtures/post-office-main.db \
 *   QT_FIXTURE_PO_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/post-office-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-announcer-tier3.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- announcer-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

const CONN = 'c0000001-0000-4000-8000-000000000001';
const CONN_B = 'c0000002-0000-4000-8000-000000000002';
const DORIAN = 'a1000000-0000-4000-8000-000000000004';
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const CHAT = 'c1000000-0000-4000-8000-000000000001';
const MISSING_CHAT = '99999999-9999-4999-8999-999999999999';
// P4.D37 (a163862c): CHAT PARTICIPANT ids — the whisper audience's currency.
// P_GONE carries `removedAt` (the preview resolves it away without failing);
// P_GHOST is live but its character row is missing, so it names to 'Someone'.
const P_ARIA = 'e1000000-0000-4000-8000-000000000001';
const P_BEA = 'e1000000-0000-4000-8000-000000000002';
const P_CLIO = 'e1000000-0000-4000-8000-000000000003';
const P_GONE = 'e1000000-0000-4000-8000-000000000005';
const P_GHOST = 'e1000000-0000-4000-8000-000000000006';

/** The canned rewrite. The failure case returns an empty string instead. */
const REWRITE =
  'The lamps on the eastern quay are dark, and the fault is oil, not idleness. ' +
  '*He sets the ledger down.* I will have them lit by the second bell.';

const cannedRecorded = new Map<
  string,
  {
    provider: string;
    model: string;
    temperature: number | null;
    messages: Array<{ role: string; content: string }>;
    response: string;
  }
>();

/** Flipped per case to drive the empty-response (failure) arm. */
let respondEmpty = false;

/**
 * The frozen clock (matches NOW_MS in the Rust test). `formatDynamicMemoryHead`
 * renders a RELATIVE age ("4 days ago") and `calculateEffectiveWeight` decays by
 * time, so both sides must read the same `Date.now()` or the assembled user
 * message can never match.
 */
const NOW_MS = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  // The chat route pulls the markdown renderer (unified/ESM) transitively.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));

  // The provider boundary: record every call keyed exactly as the Rust
  // CannedCompletionProvider looks it up, and answer the canned rewrite.
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
          },
          _apiKey: string,
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const response = respondEmpty ? '' : REWRITE;
          const temperature = (params.temperature as number | undefined) ?? null;
          const key = `${provider}|${params.model}|${temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, { provider, model: params.model, temperature, messages, response });
          }
          return { content: response, finishReason: 'stop', usage: undefined };
        },
      }),
    };
  });
  // API-key seams (host-side on the Rust port — the Rust boundary starts at the
  // provider call).
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
  // The query embed: canned so it never reaches the network. The vector store is
  // empty either way, so both sides take the text-search fallback.
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
  // logLLMCall would need the llm-logs partition, which this fixture family does
  // not carry; the Rust executor is constructed WITHOUT logging to match.
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: async () => undefined };
  });
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
}

/**
 * The memories table's access-bump evidence, clock-free: `bumped` is whether
 * `lastAccessedAt` moved past the fixture's seed timestamp.
 */
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
  /** Drive v4's REAL route handler instead of the service directly. */
  viaRoute?: boolean;
  chatId: string;
  characterId: string;
  profileId: string;
  seedMarkdown: string;
  systemPromptId?: string;
  /** P4.D37: the resolved whisper audience, for the SERVICE-level arms. */
  audienceNames?: string[];
  /** P4.D37: the requested audience, for the ROUTE-level arms (participant ids). */
  targetParticipantIds?: string[];
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
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'ann-'));
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
      // The whole route: v4's handleAnnouncementPreview over the real service.
      const route = (await import('@/app/api/v1/chats/[id]/route')) as unknown as {
        POST: (...a: unknown[]) => Promise<{ status: number; json: () => Promise<unknown> }>;
      };
      const resp = await route.POST(
        mockRequest(`http://localhost/api/v1/chats/${c.chatId}?action=announcement-preview`, {
          seedMarkdown: c.seedMarkdown,
          characterId: c.characterId,
          connectionProfileId: c.profileId,
          ...(c.targetParticipantIds !== undefined
            ? { targetParticipantIds: c.targetParticipantIds }
            : {}),
        }),
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
    const character = await repos.characters.findById(c.characterId);
    const profile = await repos.connections.findById(c.profileId);
    if (!character || !profile) throw new Error(`fixture row missing for ${c.name}`);

    const { generateCharacterVoicedAnnouncement } = await import(
      '@/lib/services/announcer/character-voiced'
    );
    const result = await generateCharacterVoicedAnnouncement({
      chatId: c.chatId,
      character,
      profile,
      seedMarkdown: c.seedMarkdown,
      systemPromptId: c.systemPromptId,
      userId: spec.userId,
      audienceNames: c.audienceNames,
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

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`announcer-tier3 oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'post-office-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PO_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PO_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ann-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    {
      // The full path: off-scene Dorian, a roster of four present participants, a
      // seed whose words hit two of his three memories.
      name: 'rewrite_offscene_with_recall',
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: '  Tell them the lamps on the eastern quay are out and I am seeing to it.  ',
    },
    {
      // A seed that matches NO memory → the recall block is absent from the user
      // message and no `lastAccessedAt` is bumped.
      name: 'rewrite_no_recall_hits',
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'Zzyzx qwertyuiop.',
    },
    {
      // An unknown chat → `buildRoster` returns '' → the alone presence line.
      name: 'rewrite_unknown_chat_no_roster',
      chatId: MISSING_CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'The lamps are out.',
    },
    {
      // A speaking character who IS a participant: they are filtered out of their
      // own roster. Also a different profile (OPENAI/gpt-4o-mini, no baseUrl).
      name: 'rewrite_speaker_is_participant',
      chatId: CHAT,
      characterId: ARIA,
      profileId: CONN_B,
      seedMarkdown: 'The lamps are out on the eastern quay.',
    },
    {
      // The empty-completion failure arm: v4 returns
      // `{success:false, proposedMarkdown:'', error:'The LLM returned no content.'}`.
      name: 'rewrite_empty_completion',
      empty: true,
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'The lamps are out.',
    },
    // The two ROUTE-level arms — v4's real `handleAnnouncementPreview` wrapping
    // the service, so the response envelope (and the `badRequest(result.error)`
    // arm) is diffed rather than asserted.
    {
      name: 'route_preview_ok',
      viaRoute: true,
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'Tell them the lamps on the eastern quay are out and I am seeing to it.',
    },
    {
      name: 'route_preview_failure',
      viaRoute: true,
      empty: true,
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'Tell them the lamps on the eastern quay are out and I am seeing to it.',
    },
    // ── P4.D37 (a163862c): the audience reaches the rewrite ─────────────────
    // The assembled user message IS the evidence: the whisper presence line
    // REPLACES the roster block (buildRoster is not even called), and the name
    // list carries v4's singular/plural and Oxford-comma forms.
    {
      name: 'rewrite_whisper_single',
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'The lamps are out.',
      audienceNames: ['Aria'],
    },
    {
      name: 'rewrite_whisper_pair',
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'The lamps are out.',
      audienceNames: ['Aria', 'Bea'],
    },
    {
      name: 'rewrite_whisper_three_oxford_comma',
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'The lamps are out.',
      audienceNames: ['Aria', 'Bea', 'Clio'],
    },
    {
      // Blank names are filtered before the branch, so one real name is singular.
      name: 'rewrite_whisper_blank_name_filtered',
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'The lamps are out.',
      audienceNames: ['   ', 'Aria'],
    },
    {
      // Filter everything away and the announcement is public again — the ROSTER
      // arm, not an empty whisper line.
      name: 'rewrite_whisper_all_blank_falls_back_to_roster',
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'The lamps are out.',
      audienceNames: ['   ', ''],
    },
    // The route arms: participant ids in, resolved names out.
    {
      name: 'route_preview_whisper_named',
      viaRoute: true,
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'Tell them the lamps on the eastern quay are out and I am seeing to it.',
      targetParticipantIds: [P_ARIA, P_BEA],
    },
    {
      // THE ASYMMETRY: the preview does NOT refuse dangling ids. A removed
      // participant contributes no name, the audience resolves empty, and the
      // rehearsal carries on with the room's roster.
      name: 'route_preview_unknown_target_does_not_refuse',
      viaRoute: true,
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'Tell them the lamps on the eastern quay are out and I am seeing to it.',
      targetParticipantIds: [P_GONE],
    },
    {
      // A live participant whose character row is missing: a valid target that
      // cannot be named, so the rewrite is addressed to 'Someone'.
      name: 'route_preview_ghost_target_is_someone',
      viaRoute: true,
      chatId: CHAT,
      characterId: DORIAN,
      profileId: CONN,
      seedMarkdown: 'Tell them the lamps on the eastern quay are out and I am seeing to it.',
      targetParticipantIds: [P_GHOST, P_CLIO],
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`announcer-tier3 oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('announcer-tier3 oracle', async () => {
  await main();
});
