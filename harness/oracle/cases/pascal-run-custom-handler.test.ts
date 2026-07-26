/**
 * @jest-environment node
 *
 * P4.6ay unit 4 (the handler half) ORACLE: drives v4's REAL
 * `executeRunCustomTool` (lib/tools/handlers/run-custom-handler.ts) over a FRESH
 * copy of the committed `pascal-run-custom-{main,mount}.db` fixture per case,
 * emitting the returned model output + the posted MAIN-db `chat_messages` rows
 * (the Pascal outcome / Prospero error). The Rust port
 * (`tools::run_custom::execute_run_custom_tool`) is diffed against it.
 *
 * Every roll is `min === max` (no draw), so the outcome is deterministic without
 * a shared byte source. Posted rows carry a minted id + createdAt — the Rust
 * diff normalizes both positionally.
 *
 * Only the DB stack is un-mocked past jest.setup (the real cipher binding); the
 * handler's seams (roster/writers/repos) are all REAL — the whole point is the
 * end-to-end DB behavior.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   QT_FIXTURE_PASCAL_MAIN=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-run-custom-handler.ndjson npx jest -- pascal-run-custom-handler
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

interface Meta {
  vaultA: string;
  vaultB: string;
  vaultC: string;
  vaultD: string;
}

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
const CHAR_C = 'a1000000-0000-4000-8000-00000000000c';
/** P4.d19: the character whose vault is broken (no `properties.json`). */
const CHAR_D = 'a1000000-0000-4000-8000-00000000000d';
const P_A = 'e1000000-0000-4000-8000-00000000000a';

interface CaseSpec {
  name: string;
  characterId: string | null;
  vaultKey: 'vaultA' | 'vaultB' | 'vaultC' | 'vaultD' | null;
  characterIds: string[];
  input: unknown;
  /**
   * P4.6bd: insert ONE cheap connection profile before the run, so the consult
   * RESOLVES through the (mocked, recording) provider instead of stopping at
   * `no connection profiles are configured`. The committed fixture itself stays
   * profile-free — the no-profiles cases depend on that.
   */
  profile?: boolean;
}

/**
 * The one profile a `profile: true` case inserts (both sides insert the SAME
 * row through their real repos — the Rust differential mirrors these fields).
 */
const CONSULT_PROFILE = {
  userId: '', // filled from the spec at insert time
  name: 'Consult Canned',
  provider: 'Anthropic',
  transport: 'api',
  courierDeltaMode: false,
  apiKeyId: null,
  baseUrl: null,
  modelName: 'consult-canned-model',
  parameters: {},
  isDefault: true,
  isCheap: true,
  allowWebSearch: false,
  useNativeWebSearch: false,
  allowToolUse: false,
  pseudoToolMode: 'auto',
  modelClass: null,
  maxContext: null,
  maxTokens: null,
  isDangerousCompatible: false,
  supportsImageUpload: false,
  tags: [],
  sortIndex: 0,
  totalTokens: 0,
  totalPromptTokens: 0,
  totalCompletionTokens: 0,
  messageCount: 0,
};
const CONSULT_PROFILE_ID = 'cc000000-0000-4000-8000-000000000001';
const CONSULT_PROFILE_TS = '2026-03-01T00:00:00.000Z';

/** The canned consult answer: `YES` hits the oracle tool's `eq: 'YES'` row. */
const CONSULT_ANSWER = 'YES';
const CONSULT_USAGE = { promptTokens: 42, completionTokens: 3, totalTokens: 45 };

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

async function runCase(
  spec: Spec,
  meta: Meta,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

  // P4.6bd: recorded canned-completion entries, deduped by the exact call key.
  // The key format MUST match `quilltap_core::model::completion::canned_completion_key`.
  const cannedRecorded = new Map<
    string,
    {
      provider: string;
      model: string;
      temperature: number | null;
      messages: Array<{ role: string; content: string }>;
      response: string;
      usage: { promptTokens: number; completionTokens: number; totalTokens: number };
    }
  >();

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
  // P4.6bd: the consult's provider seam — a recording canned provider (the
  // memory-processor recipe). Only a resolving consult ever reaches it.
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
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!cannedRecorded.has(key)) {
            cannedRecorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response: CONSULT_ANSWER,
              usage: CONSULT_USAGE,
            });
          }
          return { content: CONSULT_ANSWER, finishReason: 'stop', usage: CONSULT_USAGE };
        },
      }),
    };
  });
  // The v5 side has no API-key step in the cheap path — pin v4's to a constant.
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return {
      __esModule: true,
      ...actual,
      getApiKeyForCheapLLMSelection: async () => 'test-key',
    };
  });
  // Run the REAL logLLMCall so the CUSTOM_TOOL_CONSULT row lands and is diffed.
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
  );

  const work = mkdtempSync(join(scratch, 'pascal-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  // A fresh llm-logs DB per case for the un-mocked logLLMCall (P4.6bd).
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient, getRawLLMLogsDatabase } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { executeRunCustomTool } = await import('@/lib/tools/handlers/run-custom-handler');

  await initializeDatabase();

  if (c.profile) {
    const { ConnectionProfilesRepository } = await import(
      '@/lib/database/repositories/connection-profiles.repository'
    );
    const repo = new ConnectionProfilesRepository();
    await repo.create({ ...CONSULT_PROFILE, userId: spec.userId } as never, {
      id: CONSULT_PROFILE_ID,
      createdAt: CONSULT_PROFILE_TS,
      updatedAt: CONSULT_PROFILE_TS,
    });
  }

  try {
    const context = {
      userId: spec.userId,
      chatId: CHAT,
      characterId: c.characterId,
      characterMountPointId: c.vaultKey ? meta[c.vaultKey] : null,
      characterIds: c.characterIds,
      projectId: null,
      callerParticipantId: P_A,
    };
    const output = await executeRunCustomTool(c.input, context as never);

    // Let the fire-and-forget logLLMCall settle before dumping (the
    // oracle-fire-and-forget-tail rule: stray promises poison the NEXT case).
    await new Promise((resolve) => setTimeout(resolve, 200));

    const mdb = getRawDatabase();
    if (!mdb) throw new Error('main DB handle unavailable');
    const columns = (
      mdb.prepare(`PRAGMA table_info(chat_messages)`).all() as Array<{ name: string }>
    ).map((col) => col.name);
    const rawRows = mdb.prepare(`SELECT * FROM chat_messages ORDER BY createdAt`).all() as Array<
      Record<string, unknown>
    >;
    const rows = rawRows.map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    });

    // P4.6bd: the llm_logs rows this case wrote (id/timestamps placeholdered,
    // sorted by canonical JSON — the memory-processor dump shape).
    let llmLogs: { columns: string[]; rows: Array<Record<string, unknown>> } = {
      columns: [],
      rows: [],
    };
    const lldb = getRawLLMLogsDatabase();
    if (lldb) {
      try {
        const llColumns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map(
          (col) => col.name,
        );
        const llRaw = lldb.prepare('SELECT * FROM llm_logs').all() as Array<
          Record<string, unknown>
        >;
        const llRows = llRaw
          .map((r) => {
            const out: Record<string, unknown> = {};
            for (const col of llColumns) out[col] = canonValue(r[col]);
            out.id = '<id>';
            out.createdAt = '<ts>';
            out.updatedAt = '<ts>';
            return out;
          })
          .sort((a, b) => {
            const sa = JSON.stringify(a);
            const sb = JSON.stringify(b);
            return sa < sb ? -1 : sa > sb ? 1 : 0;
          });
        llmLogs = { columns: llColumns, rows: llRows };
      } catch {
        // No llm_logs table (the case never logged) — keep the empty dump.
      }
    }

    return {
      name: c.name,
      output,
      columns,
      rows,
      canned: Array.from(cannedRecorded.values()),
      llmLogs,
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'pascal-run-custom.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PASCAL_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PASCAL_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const meta = JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as Meta;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-handler-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const ALL = [CHAR_A, CHAR_B, CHAR_C];
  const cases: CaseSpec[] = [
    { name: 'gated-hit', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'ansible' } },
    { name: 'gated-miss', characterId: CHAR_B, vaultKey: 'vaultB', characterIds: ALL, input: { tool: 'ansible' } },
    { name: 'gated-empty', characterId: CHAR_C, vaultKey: 'vaultC', characterIds: ALL, input: { tool: 'ansible' } },
    { name: 'public-coin', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'coin' } },
    { name: 'whispered-default', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'whispered' } },
    { name: 'private-override', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'coin', private: true } },
    { name: 'public-override', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'whispered', private: false } },
    { name: 'unknown-tool', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'nonexistent' } },
    { name: 'validation-fail', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { notTool: 1 } },
    { name: 'run-error', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'coin', parameters: { bad: 1 } } },
    // The 616930db consult, end to end. The fixture carries no connection
    // profiles, so BOTH sides take the invoker's `no connection profiles are
    // configured` arm: the run does NOT fail, `pascalMeta.llm` records the
    // rendered prompt + the technical reason + the author's errorMessage as the
    // output, and the table branches to the `ok: false` row. Deterministic
    // without mocking a provider.
    { name: 'llm-consult-no-profiles', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'oracle' } },
    { name: 'llm-consult-private', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'oracle', private: true } },
    // P4.6bd: the consult RESOLVES — a profile is inserted, the (recording)
    // provider answers 'YES', the `eq: 'YES'` outcome row fires, and the
    // CUSTOM_TOOL_CONSULT llm-log row lands. The tier-1 proof of the wire.
    { name: 'llm-consult-resolved', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'oracle' }, profile: true },
    // P4.d10 `$state`: the entrance resolves the merged cascade scoped to the
    // ROLLING character's own groups. CHAR_A (in Table Stakes): roll = chat
    // difficulty 6 (over general's 9), 6 >= gscore 4 → success, with
    // {{state.*}} rendered from all three tiers. CHAR_B (no group): gscore
    // falls back to 99 → failure — the scoping made visible in the outcome.
    { name: 'state-cascade-hit', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'stateful' } },
    { name: 'state-no-group-falls-back', characterId: CHAR_B, vaultKey: 'vaultB', characterIds: ALL, input: { tool: 'stateful' } },

    // ---- P4.d19: the availability gate, end-to-end through the handler ----
    // The two gated definitions live in the GENERAL store, so the only thing
    // that differs between these rows is the invoker's own fact sheet. A
    // withheld tool is not merely un-run: it is ABSENT from the roster, so the
    // handler answers with the same "No custom tool named …" it gives an
    // invented name — which is exactly what the gate is for.
    { name: 'gate-available-runs', characterId: CHAR_A, vaultKey: 'vaultA', characterIds: ALL, input: { tool: 'secure_line' } },
    { name: 'gate-available-missing-key-unrunnable', characterId: CHAR_B, vaultKey: 'vaultB', characterIds: ALL, input: { tool: 'secure_line' } },
    { name: 'gate-available-empty-sheet-unrunnable', characterId: CHAR_C, vaultKey: 'vaultC', characterIds: ALL, input: { tool: 'secure_line' } },
    { name: 'gate-withheld-when-holds-unrunnable', characterId: CHAR_B, vaultKey: 'vaultB', characterIds: ALL, input: { tool: 'novice_aid' } },
    // The same empty sheet that fails `availableWhen` above satisfies no
    // `withheldWhen`, so CHAR_C IS dealt this one. The asymmetry, at the door.
    { name: 'gate-withheld-when-empty-sheet-runs', characterId: CHAR_C, vaultKey: 'vaultC', characterIds: ALL, input: { tool: 'novice_aid' } },

    // ---- P4.d19: the fact-sheet read MOVED ahead of the roster -----------
    // A broken vault now fails before a definition is in hand, so the whisper
    // decision can no longer consult `defaultVisibility`. `private: true` is the
    // only thing that whispers one of these.
    { name: 'vault-broken-private-true', characterId: CHAR_D, vaultKey: 'vaultD', characterIds: ALL, input: { tool: 'coin', private: true } },
    { name: 'vault-broken-private-false', characterId: CHAR_D, vaultKey: 'vaultD', characterIds: ALL, input: { tool: 'whispered', private: false } },
    // THE arm whose behavior CHANGED: `whispered` declares
    // defaultVisibility:'whisper', and before 6864bf0e that made this failure a
    // whisper. It is public now.
    { name: 'vault-broken-private-absent-whisper-default', characterId: CHAR_D, vaultKey: 'vaultD', characterIds: ALL, input: { tool: 'whispered' } },
    // A name no roster carries, through a broken vault: the vault failure wins,
    // because it is answered first.
    { name: 'vault-broken-unknown-tool', characterId: CHAR_D, vaultKey: 'vaultD', characterIds: ALL, input: { tool: 'nonexistent' } },
  ];

  const out = fs.createWriteStream(outPath);
  for (const c of cases) {
    const row = await runCase(spec, meta, c, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal run_custom handler oracle', async () => {
  await main();
}, 120000);
