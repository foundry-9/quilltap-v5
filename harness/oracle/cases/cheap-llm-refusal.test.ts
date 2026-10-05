/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the cheap-LLM refusal record (P4.D225, v4 `49059fb14`,
 * `lib/memory/cheap-llm-tasks/core-execution.ts` `recordCheapRefusal`).
 *
 * Drives v4's REAL `executeCheapLLMTask` with only the model boundary canned
 * (`createLLMProvider`, a per-case script of `{content, finishReason}` or a
 * throw) and the API-key step stubbed (the fixture seeds no key; the Rust
 * boundary starts at the provider call). Everything past it is real: the
 * uncensored retry, `classifyRefusal`, `recordModerationRefusal` and the
 * auto-switch over the REAL chats + chat-settings repositories on the baked
 * fixture (`build-refusal-ledger-fixture.ts` with
 * `QT_REFUSAL_LEDGER_SPEC=cheap-llm-refusal.json`). Per case: the task result
 * and every `ConciergeRefusal` / `ConciergeRefusalLedger` line; then the
 * `chats` + `chat_messages` dumps.
 *
 * Regen recipe: the header of
 * `crates/quilltap-harness/tests/cheap_llm_fallback_equivalence.rs`
 * (`cheap_llm_refusal_matches_oracle`).
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  return v;
}

type Step =
  | { content: string; finishReason?: string }
  | { throws: string }
  | { throwsHttp: { status: number; body: string } };

/**
 * P4.118: what an openai-SDK provider plugin (OPENAI_COMPATIBLE here) throws on
 * a non-2xx — the REAL `APIError` class from the checkout's `openai` 7.23.0,
 * built exactly as the client's own non-2xx path builds it (`client.js`
 * `makeRequest`: `safeJSON(errText)`, `errJSON ? undefined : errText`, then
 * `makeStatusError`'s `{error: body}` wrap when `.error == null`, then
 * `APIError.generate`). The plugins rethrow it untouched (survey §A.3), so this
 * is the value `executeCheapLLMTask` catches. The wire family
 * (`text_http_errors_equivalence`) proves v5's production reconstruction of it
 * row by row; this arm proves the cheap chain acts on it.
 */
function openaiStatusError(status: number, text: string): Error {
  const { APIError } = require(require('node:path').join(process.cwd(), 'node_modules/openai'));
  let errJSON: unknown;
  try {
    errJSON = JSON.parse(text);
  } catch {
    errJSON = undefined;
  }
  const errMessage = errJSON ? undefined : text;
  const normalized =
    errJSON && typeof errJSON === 'object' && (errJSON as { error?: unknown }).error == null
      ? { error: errJSON }
      : errJSON;
  return APIError.generate(status, normalized, errMessage, new Headers());
}
interface Case {
  name: string;
  chatId: string | null;
  selection: Record<string, unknown>;
  uncensored: boolean;
  script: Step[];
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  /** v4 `3b463d6b1` (#76): the stored `conciergeSettings` the fallback's policy resolves from. */
  concierge: Record<string, unknown>;
  connectionProfiles: Array<{ id: string }>;
  ledgerPlants: Array<{ chatId: string; count: number; lastAt: string }>;
  cases: Case[];
}

const SERVICES = new Set(['ConciergeRefusal', 'ConciergeRefusalLedger']);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'cheap-llm-refusal.json'), 'utf8'),
  ) as Spec;
  const fixture = process.env.QT_FIXTURE_CHEAP_REFUSAL;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_CHEAP_REFUSAL must point at the seed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cheap-refusal-oracle-'));
  try {
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'cheap-refusal-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.QUILLTAP_JOB_CHILD;
  process.env.LOG_LEVEL = 'error';

  const logs: Array<{ service: string | null; level: string; message: string; bag: unknown }> = [];
  let script: Step[] = [];

  jest.resetModules();
  jest.doMock('@/lib/logger', () => {
    const recorder = (service: string | null) => {
      const record = (level: string) => (message: string, bag?: Record<string, unknown>) =>
        logs.push({ service, level, message, bag: JSON.parse(JSON.stringify(bag ?? {})) });
      const self: Record<string, unknown> = {
        debug: record('debug'),
        info: record('info'),
        warn: record('warn'),
        error: record('error'),
        trace: record('trace'),
      };
      self.child = (ctx: Record<string, unknown>) =>
        recorder(typeof ctx?.service === 'string' ? ctx.service : service);
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(null),
    };
  });
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/services/concierge-notifications/writer', () =>
    jest.requireActual('@/lib/services/concierge-notifications/writer'),
  );
  // The tier-3 model boundary: the case's script, one step per call.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async () => ({
        sendMessage: async () => {
          const step = script.shift();
          if (!step) throw new Error('the case script ran out');
          if ('throws' in step) throw new Error(step.throws);
          if ('throwsHttp' in step) throw openaiStatusError(step.throwsHttp.status, step.throwsHttp.body);
          return { content: step.content, finishReason: step.finishReason ?? null };
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

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { executeCheapLLMTask } = await import('@/lib/memory/cheap-llm-tasks/core-execution');
  const { ConciergeSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { resolveConciergeSettings } = await import('@/lib/services/dangerous-content/resolver.service');

  await initializeDatabase();
  const repos = getRepositories();
  for (const plant of spec.ledgerPlants) {
    await rawQuery(
      'UPDATE chats SET "moderationRefusalCount" = ?, "lastModerationRefusalAt" = ? WHERE id = ?',
      [plant.count, plant.lastAt, plant.chatId],
    );
  }
  const availableProfiles = [];
  for (const p of spec.connectionProfiles) {
    const row = await repos.connections.findById(p.id);
    if (!row) throw new Error(`connection profile ${p.id} not seeded`);
    availableProfiles.push(row);
  }
  const conciergePolicy = resolveConciergeSettings({
    conciergeSettings: ConciergeSettingsSchema.parse(spec.concierge),
  });

  const lines: string[] = [JSON.stringify({ kind: 'conciergePolicy', conciergePolicy })];
  for (const c of spec.cases) {
    script = c.script.slice();
    logs.length = 0;
    const result = await executeCheapLLMTask(
      { isLocal: false, ...c.selection } as never,
      [{ role: 'user', content: 'Summarize the scene.' }],
      spec.userId,
      (content: string) => content,
      'oracle-cheap-refusal',
      c.chatId ?? undefined,
      undefined,
      c.uncensored
        ? ({ conciergePolicy, availableProfiles, isDangerousChat: false } as never)
        : undefined,
    );
    if (script.length !== 0) throw new Error(`${c.name}: ${script.length} scripted step(s) unused`);
    lines.push(
      JSON.stringify({
        kind: 'case',
        name: c.name,
        result: { success: result.success, result: result.result ?? null, error: result.error ?? null },
        logs: logs
          .filter((l) => l.service !== null && SERVICES.has(l.service))
          .map(({ service, level, message, bag }) => ({ service, level, message, bag })),
      }),
    );
  }

  const dumpTable = async (table: string, orderBy: string) => {
    const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map(
      (c) => c.name,
    );
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
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
  };
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chats', 'id')) }));
  lines.push(JSON.stringify({ kind: 'table', ...(await dumpTable('chat_messages', 'chatId')) }));

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`cheap-llm-refusal oracle wrote ${outPath} (${spec.cases.length} cases)\n`);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('cheap-llm-refusal tier-3 oracle', async () => {
  await main();
});
