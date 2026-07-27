/**
 * @jest-environment node
 *
 * P4.9E3B llm_choose TIER-3 ORACLE: drives v4's REAL add-participant
 * (`?action=add-participant`) and merge-conversation
 * (`?action=merge-conversation`) routes with `llm_choose` outfit selections,
 * the model boundary mocked to a canned reply (`tier3-completion-oracle`), and
 * emits: the response status + a REDUCED body, the LLM request MESSAGES seen
 * by the provider (so the outfit prompt's bytes — character fields, wardrobe
 * listing, scenario — are pinned end-to-end), and the target chat's
 * `equippedOutfit` + a reduced participants dump. Everything minted (ids,
 * timestamps, Host bubbles) is deliberately outside this family — the
 * chat-cast and chat-admin differentials own those bytes.
 *
 * **Must run under `TZ=UTC`.**
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cd-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-dialogs-llm-choose-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-dialogs-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
 *   QT_FIXTURE_CD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-llm-choose-tier3.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-dialogs-llm-choose-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Canned {
  content: string;
  promptTokens: number;
  completionTokens: number;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
  cannedOutfits: Record<string, Canned>;
}

const PIP = 'a2000000-0000-4000-8000-000000000002';
const MERGE_TARGET = 'c2000000-0000-4000-8000-000000000007';
const MERGE_SOURCE = 'c2000000-0000-4000-8000-000000000008';

const RealDate = Date;

let currentReply: Canned | null = null;
let providerThrows = false;
let seenMessages: Array<Array<{ role: string; content: string }>> = [];

function mockRequest(url: string, body?: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
  }));
  // The tier-3 model boundary (the regenerate-title recipe).
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async () => ({
        sendMessage: async (params: { messages: Array<{ role: string; content: string }> }) => {
          seenMessages.push(params.messages.map((m) => ({ role: m.role, content: m.content })));
          if (providerThrows) throw new Error('canned provider failure');
          if (!currentReply) throw new Error('no canned reply for this case');
          return {
            content: currentReply.content,
            usage: {
              promptTokens: currentReply.promptTokens,
              completionTokens: currentReply.completionTokens,
              totalTokens: currentReply.promptTokens + currentReply.completionTokens,
            },
          };
        },
      }),
    };
  });
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'canned-test-key',
    };
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

/** The target chat's outfit + reduced cast (no minted values). */
async function readTables(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();
  const chat = (await repos.chats.findById(chatId)) as Record<string, unknown> | null;
  const equipped = await repos.chats.getEquippedOutfit(chatId);
  const participants = ((chat?.participants as Array<Record<string, unknown>>) ?? []).map(
    (p) => ({
      characterId: p.characterId,
      controlledBy: p.controlledBy,
      status: p.status,
    }),
  );
  return { equippedOutfit: equipped ?? {}, participants };
}

interface CaseSpec {
  name: string;
  action: 'add-participant' | 'merge-conversation';
  chatId: string;
  body: unknown;
  reply?: string; // key into spec.cannedOutfits
  throws?: boolean;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  currentReply = c.reply ? spec.cannedOutfits[c.reply] : null;
  providerThrows = !!c.throws;
  seenMessages = [];
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'lc-'));
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

  let tick = 0;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(spec.frozenNowMs + tick++);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return spec.frozenNowMs + tick++;
    }
  } as unknown as DateConstructor;

  try {
    const route = (await import('@/app/api/v1/chats/[id]/route')) as never as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >;
    const resp = (await route.POST(
      mockRequest(`http://localhost/api/v1/chats/${c.chatId}?action=${c.action}`, c.body),
      { params: Promise.resolve({ id: c.chatId }) },
    )) as { status: number; body?: unknown; json?: () => Promise<unknown> };
    const rawBody =
      resp.body !== undefined && typeof resp.body === 'object'
        ? (resp.body as Record<string, unknown>)
        : ((await (resp as { json: () => Promise<unknown> }).json()) as Record<string, unknown>);
    // Reduce the body to the stable claim (other bytes are pinned by the
    // chat-cast / chat-admin families).
    const reduced: Record<string, unknown> = {};
    if (c.action === 'add-participant') {
      reduced.participantCharacterId =
        (rawBody.participant as Record<string, unknown> | undefined)?.characterId ?? null;
      reduced.error = rawBody.error ?? null;
    } else {
      const merge = rawBody.merge as Record<string, unknown> | undefined;
      reduced.success = rawBody.success ?? null;
      reduced.mergedCharacterIds = merge?.mergedCharacterIds ?? null;
      reduced.error = rawBody.error ?? null;
    }
    return {
      name: c.name,
      status: resp.status,
      body: reduced,
      llmMessages: seenMessages,
      tables: await readTables(c.chatId),
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
    throw new Error(
      `chat-dialogs-llm-choose-tier3 oracle must run under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-dialogs-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CD_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-lc-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    // The pick succeeds: Pip joins The Rain Watch wearing the canned choice
    // (composite in top, boots, scarf). The scenario line rides the prompt.
    {
      name: 'add_llm_choose_pick',
      action: 'add-participant',
      chatId: MERGE_TARGET,
      body: {
        type: 'CHARACTER',
        characterId: PIP,
        outfitSelection: { characterId: PIP, mode: 'llm_choose' },
      },
      reply: 'pipPick',
    },
    // The reply carries a bogus id, a wrong-slot id, and a legacy scalar —
    // the validator drops/accepts exactly as v4.
    {
      name: 'add_llm_choose_invalid_ids',
      action: 'add-participant',
      chatId: MERGE_TARGET,
      body: {
        type: 'CHARACTER',
        characterId: PIP,
        outfitSelection: { characterId: PIP, mode: 'llm_choose' },
      },
      reply: 'pipInvalidIds',
    },
    // The provider fails → the default outfit (Pip's patched coat).
    {
      name: 'add_llm_choose_provider_fails',
      action: 'add-participant',
      chatId: MERGE_TARGET,
      body: {
        type: 'CHARACTER',
        characterId: PIP,
        outfitSelection: { characterId: PIP, mode: 'llm_choose' },
      },
      throws: true,
    },
    // The merge's explicit llm_choose: Pip merges in from The Lower Quay
    // wearing the canned choice.
    {
      name: 'merge_llm_choose_pick',
      action: 'merge-conversation',
      chatId: MERGE_TARGET,
      body: {
        sourceChatId: MERGE_SOURCE,
        outfitSelections: [{ characterId: PIP, mode: 'llm_choose' }],
      },
      reply: 'pipPick',
    },
    // The merge's llm_choose failure → the default outfit.
    {
      name: 'merge_llm_choose_provider_fails',
      action: 'merge-conversation',
      chatId: MERGE_TARGET,
      body: {
        sourceChatId: MERGE_SOURCE,
        outfitSelections: [{ characterId: PIP, mode: 'llm_choose' }],
      },
      throws: true,
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(
    `wrote ${lines.length} chat-dialogs-llm-choose-tier3 oracle rows to ${outPath}\n`,
  );
}

test('chat-dialogs-llm-choose-tier3 oracle', async () => {
  await main();
});
