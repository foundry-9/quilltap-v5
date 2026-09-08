/**
 * @jest-environment node
 *
 * P4.82 TIER-3 (mocked-LLM) ORACLE for the `CHARACTER_HEADSHOULDERS_BACKFILL`
 * job handler: drives v4's REAL `handleCharacterHeadShouldersBackfill`
 * (`lib/background-jobs/handlers/character-headshoulders-backfill.ts`) over a
 * FRESH copy of the committed `headshoulders-{main,mount}.db` pair per case,
 * with the cheap-LLM provider CANNED, and dumps what the handler wrote so the
 * Rust port diffs it tier-2 style.
 *
 * The recorded `call` is half the point: the prompts ARE the port, so every
 * case that reaches the model dumps `provider|model|temperature|maxTokens|
 * profileParameters|messages` verbatim. `buildContextPrompt`'s seed-text
 * placement (the `imageDescription` slot) and the `HEAD_AND_SHOULDERS_
 * PHYSICAL_PROMPT` field prompt are therefore byte comparands, and so is the
 * ABSENT tenth `generateField` argument (v4 never passes `profileParameters`
 * on this path — it must arrive `undefined`/null on both sides).
 *
 * The write is dumped twice over: as the overlay READS `physicalDescription`
 * back, and as the vault store's rendered `physical-prompts.json` bytes, whose
 * key order is v4's `renderPhysicalPromptsJson`. The happy-path character
 * carries all five other prompt tiers, so dropping the `...pd` spread reddens
 * the case.
 *
 * Cases (17): the missing character; the vault-less character (the only shape
 * whose `physicalDescription` is absent — see the fixture builder's
 * measurement); a filled prompt; a WHITESPACE-only prompt (treated as absent,
 * so it writes); no seed at all; a whitespace-only seed (enqueued by the scan,
 * silently skipped here — the asymmetry); the `fullDescription` and
 * `shortPrompt` arms of the seed fall-through; no connection profile; no API
 * key; a LOCAL (Ollama) selection, whose key resolves to the empty string; the
 * provider throwing; an empty answer (`generateField`'s `No response from
 * model` throw — the job FAILS); an all-whitespace answer (the "returned empty
 * text" warn, no write); and two `substring(0, 500)` boundary cases, one
 * landing between astral characters and one SPLITTING a surrogate pair.
 *
 * Seams mocked, and why:
 *   - `createLLMProvider` — the tier-3 model boundary; records the request.
 *   - the background-jobs processor — off, so it cannot claim the fixture's
 *     seeded PENDING row and race the dump (the P4.6y lesson).
 *   - `logLLMCall` runs REAL, so the CHARACTER_WIZARD rows land and diff.
 *   - the character-vault bridge + mount-index modules are un-mocked so the
 *     REAL vault write lands byte-diffable rows.
 *   - `Date` frozen at `spec.frozenNowMs`, so the `physicalDescription
 *     .updatedAt` this handler writes is deterministic on both sides.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-hs-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/headshoulders-backfill-tier3.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/headshoulders.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/headshoulders-main.db \
 *   QT_FIXTURE_HS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/headshoulders-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-headshoulders-backfill.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- headshoulders-backfill-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ReplySpec {
  text?: string;
  repeat?: string;
  count?: number;
  suffix?: string;
  throws?: string;
}
interface CaseSpec {
  name: string;
  character: number | 'missing';
  user: number;
  reply?: ReplySpec;
}
interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
  missingCharacterId: string;
  users: Array<{ id: string }>;
  characters: Array<{ id: string }>;
  handlerCases: CaseSpec[];
}

/** The canned answer, built from the spec recipe — identical on both sides. */
function replyText(r: ReplySpec | undefined): string {
  if (!r) return '';
  if (r.text !== undefined) return r.text;
  return (r.repeat ?? '').repeat(r.count ?? 0) + (r.suffix ?? '');
}

/**
 * Rust's `serde_json` REFUSES an unpaired surrogate escape (`\ud83d`), which
 * `JSON.stringify` emits verbatim for the `astral_split_pair` case's
 * `substring(0, 500)` cut — one such row would make the whole NDJSON
 * unparseable on the Rust side. So the record is sanitized to U+FFFD before it
 * is written, `sanitizedLoneSurrogates` records that it happened, and the
 * LOSSLESS value travels separately as `headAndShouldersPromptUtf16` (the code
 * units of what v4 actually stored). The differential compares the UNIT ARRAY
 * for that case and asserts the flag is FALSE everywhere else, so the
 * sanitizer can never quietly launder a real divergence into a green.
 */
function loneSurrogates(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = i + 1 < s.length ? s.charCodeAt(i + 1) : 0;
      if (next >= 0xdc00 && next <= 0xdfff) {
        i++;
        continue;
      }
      n++;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      n++;
    }
  }
  return n;
}
function sanitizeString(s: string): string {
  let out = '';
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = i + 1 < s.length ? s.charCodeAt(i + 1) : 0;
      if (next >= 0xdc00 && next <= 0xdfff) {
        out += s[i] + s[i + 1];
        i++;
        continue;
      }
      out += '\ufffd';
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      out += '\ufffd';
    } else {
      out += s[i];
    }
  }
  return out;
}
let sanitizedAny = false;
function sanitizeDeep(v: unknown): unknown {
  if (typeof v === 'string') {
    if (loneSurrogates(v) > 0) {
      sanitizedAny = true;
      return sanitizeString(v);
    }
    return v;
  }
  if (Array.isArray(v)) return v.map(sanitizeDeep);
  if (v && typeof v === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) out[k] = sanitizeDeep(val);
    return out;
  }
  return v;
}
function utf16Units(s: string): number[] {
  const out: number[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i));
  return out;
}

interface LogLine {
  level: string;
  message: string;
  context: Record<string, unknown> | null;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'headshoulders.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_HS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_HS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_HS_MAIN and QT_FIXTURE_HS_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const lines: string[] = [];
  const RealDate = Date;

  for (const c of spec.handlerCases) {
    const characterId =
      c.character === 'missing' ? spec.missingCharacterId : spec.characters[c.character - 1].id;
    const userId = spec.users[c.user - 1].id;

    const scratch = mkdtempSync(join(tmpdir(), 'qt-hs-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'hs-main.db');
    const mountWork = join(scratch, 'hs-mount.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'hs-llm-logs.db');
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const logLines: LogLine[] = [];
    const calls: Array<Record<string, unknown>> = [];

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );
    jest.doMock('@/lib/services/llm-logging.service', () =>
      jest.requireActual('@/lib/services/llm-logging.service'),
    );

    // The handler's own six lines come through the bare `logger`.
    jest.doMock('@/lib/logger', () => {
      const record =
        (level: LogLine['level']) =>
        (message: string, context?: Record<string, unknown>) => {
          if (typeof message === 'string' && message.startsWith('[HeadShouldersBackfill]')) {
            logLines.push({ level, message, context: context ?? null });
          }
        };
      const mk = (): Record<string, unknown> => {
        const l = {
          info: record('info'),
          warn: record('warn'),
          debug: record('debug'),
          error: record('error'),
          child: () => mk(),
        };
        return l;
      };
      return { __esModule: true, logger: mk() };
    });

    // The tier-3 model boundary: record the whole request, answer the case's
    // canned reply (or throw).
    jest.doMock('@/lib/llm', () => {
      const actual = jest.requireActual('@/lib/llm');
      return {
        __esModule: true,
        ...actual,
        createLLMProvider: async (provider: string, baseUrl?: string) => ({
          sendMessage: async (params: Record<string, unknown>, apiKey: string) => {
            calls.push({
              provider,
              baseUrl: baseUrl ?? null,
              apiKey,
              model: params.model ?? null,
              temperature: params.temperature ?? null,
              maxTokens: params.maxTokens ?? null,
              profileParameters:
                params.profileParameters === undefined ? null : params.profileParameters,
              messages: params.messages ?? null,
            });
            if (c.reply?.throws) throw new Error(c.reply.throws);
            return { content: replyText(c.reply), usage: undefined };
          },
        }),
      };
    });
    jest.doMock('@/lib/llm/plugin-factory', () => {
      const actual = jest.requireActual('@/lib/llm/plugin-factory');
      return {
        __esModule: true,
        ...actual,
        createLLMProvider: async (provider: string, baseUrl?: string) => ({
          sendMessage: async (params: Record<string, unknown>, apiKey: string) => {
            calls.push({
              provider,
              baseUrl: baseUrl ?? null,
              apiKey,
              model: params.model ?? null,
              temperature: params.temperature ?? null,
              maxTokens: params.maxTokens ?? null,
              profileParameters:
                params.profileParameters === undefined ? null : params.profileParameters,
              messages: params.messages ?? null,
            });
            if (c.reply?.throws) throw new Error(c.reply.throws);
            return { content: replyText(c.reply), usage: undefined };
          },
        }),
      };
    });

    jest.doMock('@/lib/background-jobs/processor', () => {
      const actual = jest.requireActual('@/lib/background-jobs/processor');
      return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
    });

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRawLLMLogsDatabase } = await import(
      '@/lib/database/backends/sqlite/llm-logs-client'
    );
    const { getRepositories } = await import('@/lib/repositories/factory');

    await initializeDatabase();

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
      const record: Record<string, unknown> = { name: c.name };
      const { handleCharacterHeadShouldersBackfill } = await import(
        '@/lib/background-jobs/handlers/character-headshoulders-backfill'
      );
      const job = {
        id: `oracle-hs-${c.name}`,
        userId,
        type: 'CHARACTER_HEADSHOULDERS_BACKFILL',
        status: 'PROCESSING',
        payload: { characterId },
      };
      try {
        await handleCharacterHeadShouldersBackfill(job as never);
        record.threw = null;
      } catch (e) {
        record.threw = e instanceof Error ? e.message : String(e);
      }

      // What the overlay reads back.
      const repos = getRepositories();
      const after = c.character === 'missing' ? null : await repos.characters.findById(characterId);
      record.physicalDescription = after?.physicalDescription ?? null;

      // The rendered vault file bytes — key order is the comparand.
      const midb = getRawMountIndexDatabase();
      if (!midb) throw new Error('mount-index DB handle unavailable for dump');
      const mountPointId = after?.characterDocumentMountPointId ?? null;
      record.physicalPromptsFile = mountPointId
        ? ((
            midb
              .prepare(
                `SELECT d."content" AS content FROM "doc_mount_documents" d
                   JOIN "doc_mount_files" f ON f."id" = d."fileId"
                   JOIN "doc_mount_file_links" l ON l."fileId" = f."id"
                  WHERE l."mountPointId" = ? AND l."relativePath" = ?`,
              )
              .get(mountPointId, 'physical-prompts.json') as { content: string } | undefined
          )?.content ?? null)
        : null;

      // The CHARACTER_WIZARD rows (fire-and-forget → settle first).
      await new Promise((resolve) => setTimeout(resolve, 150));
      record.llmLogs = ((): Array<Record<string, unknown>> => {
        try {
          const lldb = getRawLLMLogsDatabase();
          if (!lldb) return [];
          const exists = lldb
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='llm_logs'")
            .get();
          if (!exists) return [];
          const rows = lldb
            .prepare('SELECT "type", "characterId", "provider", "modelName" FROM "llm_logs"')
            .all() as Array<Record<string, unknown>>;
          return rows.sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
        } catch {
          return [];
        }
      })();

      record.calls = calls;
      record.log = logLines;
      // The LOSSLESS carrier for the split-surrogate case (see the sanitizer's
      // note above) — the code units of what v4 actually stored.
      const writtenPrompt = (after?.physicalDescription as { headAndShouldersPrompt?: string | null } | null)
        ?.headAndShouldersPrompt;
      record.headAndShouldersPromptUtf16 =
        typeof writtenPrompt === 'string' ? utf16Units(writtenPrompt) : null;
      sanitizedAny = false;
      const safe = sanitizeDeep(record) as Record<string, unknown>;
      safe.sanitizedLoneSurrogates = sanitizedAny;
      lines.push(JSON.stringify(safe));
    } finally {
      global.Date = RealDate;
      await new Promise((resolve) => setTimeout(resolve, 50));
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(
    `headshoulders-backfill oracle wrote ${outPath} (${lines.length} lines)\n`,
  );
}

test('headshoulders-backfill tier-3 oracle', async () => {
  await main();
});
