/**
 * @jest-environment node
 *
 * Tier-3 ORACLE (P4.D291) for recall's assemble and deliver stages: v4's REAL
 * `assembleRecall` + `deliverRecall` (`lib/memory/recall/{assemble,deliver}.ts`,
 * `584d28164`, pinned at `83334b9a5`) over a fresh copy of the committed
 * `recall-assemble-main.db` per case — the corpus of v4's own
 * `__tests__/unit/lib/memory/recall/assemble-deliver.test.ts`, re-planted so
 * the frozen archive, the index store, the repositories, the recap's banded
 * tier reads and the chat's ring write are all REAL (R-L6).
 *
 * Mocked, and nothing else:
 * - `generateEmbeddingForUser` — every text embeds as `[1, 0]` (`OPENAI`), as
 *   in v4's test; every call's text is recorded in order (the "one embedding
 *   per turn" comparand);
 * - `createLLMProvider` — one canned completion (the recap's narrative),
 *   RECORDING each `provider|model|temperature|messages` it answered so the
 *   Rust side replays the same keys; `getApiKeyForCheapLLMSelection` → a
 *   constant; `logLLMCall` → a no-op;
 * - `refreshMemoryTimeZone` → `UTC`.
 * `Date` is frozen at the spec's `$nowMs`. jest.setup's global vault-resolver
 * mock is UN-mocked (the fixture's characters have no rows, so no vault: the
 * conversation lists find nothing, on both sides).
 *
 * Mirrors the v4 test's fifteen: one embedding per ordinary turn; a row in at
 * most one pool, archive first (bug 224); the inter-character relevance half
 * from the turn's own pool; the recap's rows kept in the head; a
 * retrospective turn embedding only its query + probes; the mini-recap riding
 * the turn's vector when it names no one; the fallback search when the
 * proactive pass found nothing; relevance read past the head's cut; "writes
 * nothing until delivery"; delivery stamping only what reached the model (a
 * compression cut); the ring recording only the head; recap-only rows never
 * stamped; the trimmed mini-recap still counted delivered; nothing delivered →
 * nothing stamped; and the archive byte-stable on a retrospective turn.
 *
 * Each case is a list of STEPS; the record is `{ name, steps, embedTexts,
 * canned, logs, stamped, ring }` where a step is an assemble (the bundle in a
 * stable JSON shape) or a deliver (the delivery), `stamped` is the ids whose
 * `lastAccessedAt` became non-null, and `ring` the chat's
 * `commonplaceRecallHistory` afterwards. The `[Recall]` / `[Memory]` lines are
 * captured by splicing a transport into the logger singleton.
 *
 * Stage OUTSIDE any `.claude/` path; see `recall_assemble_tier3_equivalence.rs`
 * for the recipe.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  $nowMs: number;
  lyra: string;
  iris: string;
  chat: { id: string };
  memories: Array<{ id: string; summary: string; aboutCharacterId?: string }>;
}

type Step =
  | { op: 'assemble'; plan: Record<string, unknown>; pre?: PreSpec }
  | { op: 'deliver'; of: number; text: 'everything' | 'cutFirstHead' | 'nothing' }
  | { op: 'deliverRetro' };

interface PreSpec {
  text: string;
  signals?: Record<string, unknown>;
  /** Search the query (with a retrospective recall context when `retro`). */
  search?: { limit: number; minImportance?: number; retroContext?: boolean };
  /** Hand the search's head on, minus one id, cut to N (the past-cut case). */
  headWithout?: { id: string; take: number };
  /** Hand an empty candidate list on (the fallback case). */
  empty?: boolean;
  /** Hand the search's ranked rows on too. */
  ranked?: boolean;
}

interface CaseSpec {
  name: string;
  steps: Step[];
}

const RECAP_TEXT = 'Lyra remembers a great deal.';
const SELECTION = {
  provider: 'OPENAI',
  modelName: 'gpt-canned',
  connectionProfileId: 'p1',
  isLocal: false,
};

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'recall-assemble.json'), 'utf8'),
  ) as Spec;
  const mainFixture = process.env.QT_FIXTURE_RA_MAIN;
  if (!mainFixture || !existsSync(mainFixture)) {
    throw new Error('QT_FIXTURE_RA_MAIN must point at the committed fixture');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const NOW = spec.$nowMs;
  const LYRA = spec.lyra;
  const IRIS = spec.iris;
  const idOf = (summary: string) => spec.memories.find(m => m.summary === summary)!.id;
  const ABOUT_IRIS = idOf('iris secret');
  const day = 86_400_000;

  const basePlan = {
    userId: spec.userId,
    provider: 'OPENAI',
    character: { id: LYRA, name: 'Lyra' },
    chat: { id: spec.chat.id, projectId: null, compactionGeneration: 0, commonplaceRecallHistory: null },
    memoryBudget: 2000,
    minImportance: 0.3,
    skipMemories: false,
    present: [{ id: LYRA, name: 'Lyra' }, { id: IRIS, name: 'Iris' }],
    interCharacterSubjects: [{ id: IRIS, name: 'Iris' }],
    currentStateTokens: 0,
    recallSettings: { scopePolicy: 'down-weight', expandRelated: false, perTurnConversationSummaries: true },
    recentWindow: 'What did Iris tell you at the lighthouse?',
    conversations: { maxContext: 32000 },
  };
  const noPerTurn = { recallSettings: { scopePolicy: 'down-weight', expandRelated: false, perTurnConversationSummaries: false } };
  const withRecap = { recap: { fallbackQuery: 'the scenario' } };
  const retroSignals = {
    keywords: ['lighthouse'],
    paraphrase: 'They remember the lighthouse.',
    retrospective: true,
    entities: ['Lighthouse Point'],
    timeRange: { from: new Date(NOW - 9 * day).toISOString(), to: new Date(NOW - day).toISOString() },
  };

  const CASES: CaseSpec[] = [
    { name: 'ordinary_turn', steps: [{ op: 'assemble', plan: {} }] },
    { name: 'recap_rows_kept_in_head', steps: [{ op: 'assemble', plan: withRecap }] },
    {
      name: 'retrospective_pre_searched',
      steps: [{
        op: 'assemble',
        plan: noPerTurn,
        pre: { text: 'w', signals: retroSignals, search: { limit: 30, minImportance: 0.3, retroContext: true } },
      }],
    },
    {
      name: 'mini_recap_names_no_one',
      steps: [{
        op: 'assemble',
        plan: noPerTurn,
        pre: { text: 'w', signals: { keywords: ['bridge'], paraphrase: 'They remember the bridge.', retrospective: true, entities: [] }, search: { limit: 30 } },
      }],
    },
    { name: 'fallback_when_pre_found_nothing', steps: [{ op: 'assemble', plan: {}, pre: { text: 'nothing matched this', empty: true } }] },
    {
      name: 'relevance_past_the_cut',
      steps: [{
        op: 'assemble',
        plan: {},
        pre: { text: 'w', search: { limit: 30, minImportance: 0.3 }, headWithout: { id: ABOUT_IRIS, take: 3 }, ranked: true },
      }],
    },
    { name: 'deliver_compression_cut', steps: [{ op: 'assemble', plan: {} }, { op: 'deliver', of: 0, text: 'cutFirstHead' }] },
    { name: 'deliver_ring_head_only', steps: [{ op: 'assemble', plan: {} }, { op: 'deliver', of: 0, text: 'everything' }] },
    { name: 'deliver_recap_turn', steps: [{ op: 'assemble', plan: { ...withRecap, ...noPerTurn } }, { op: 'deliver', of: 0, text: 'everything' }] },
    { name: 'deliver_nothing', steps: [{ op: 'assemble', plan: {} }, { op: 'deliver', of: 0, text: 'nothing' }] },
    { name: 'deliver_trimmed_mini_recap', steps: [{ op: 'deliverRetro' }] },
    {
      name: 'archive_still_on_retro_turn',
      steps: [
        { op: 'assemble', plan: {} },
        { op: 'assemble', plan: {}, pre: { text: 'w', signals: { keywords: ['x'], paraphrase: 'Remember the bridge?', retrospective: true }, empty: true } },
      ],
    },
  ];

  const RealDate = Date;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(NOW);
      // @ts-expect-error variadic forwarding
      else super(...args);
    }
    static now(): number {
      return NOW;
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null, spawning: false, shuttingDown: true, childCrashed: false,
    restartTimestamps: [], signalHandlersInstalled: false,
  };

  const lines: string[] = [];
  for (const c of CASES) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-ra-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    copyFileSync(mainFixture, mainWork);
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount-work.db');
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'debug';

    const embedTexts: string[] = [];
    const canned = new Map<string, Record<string, unknown>>();
    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
    // jest.setup mocks the vault resolver globally; the real one answers "no
    // vault" for a character with no row, as the Rust side's does.
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: jest.fn(async (text: string) => {
          embedTexts.push(text);
          return { embedding: new Float32Array([1, 0]), model: 'canned', dimensions: 2, provider: 'OPENAI' };
        }),
      };
    });
    jest.doMock('@/lib/llm', () => {
      const actual = jest.requireActual('@/lib/llm');
      return {
        __esModule: true,
        ...actual,
        createLLMProvider: async (provider: string) => ({
          sendMessage: async (
            params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number },
          ) => {
            const messages = params.messages.map(m => ({ role: m.role, content: m.content }));
            const usage = { promptTokens: 10, completionTokens: 5, totalTokens: 15 };
            const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
            if (!canned.has(key)) {
              canned.set(key, { provider, model: params.model, temperature: params.temperature ?? null, messages, response: RECAP_TEXT, usage });
            }
            return { content: RECAP_TEXT, finishReason: 'stop', usage };
          },
        }),
      };
    });
    jest.doMock('@/lib/services/api-key.service', () => {
      const actual = jest.requireActual('@/lib/services/api-key.service');
      return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
    });
    jest.doMock('@/lib/services/llm-logging.service', () => {
      const actual = jest.requireActual('@/lib/services/llm-logging.service');
      return { __esModule: true, ...actual, logLLMCall: async () => undefined };
    });
    jest.doMock('@/lib/memory/time/calendar-refresh', () => ({
      __esModule: true,
      refreshMemoryTimeZone: async () => 'UTC',
    }));

    const { logger } = await import('@/lib/logger');
    const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
        if (!/^\[(Recall|Memory)\]/.test(data.message)) return;
        const { service: _s, environment: _e, module: _m, ...fields } = data.context ?? {};
        if (data.error) fields.error = data.error.message;
        captured.push({ level: data.level, message: data.message, fields });
      },
    });

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    await initializeDatabase();
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { assembleRecall, emptyRecallBundle } = await import('@/lib/memory/recall/assemble');
    const { deliverRecall } = await import('@/lib/memory/recall/deliver');
    const { buildRecallQuery } = await import('@/lib/memory/recall/query');
    const { searchRecall } = await import('@/lib/memory/recall/search');
    const { buildTurnRecallContext } = await import('@/lib/memory/recall/tags');
    const db = getRawDatabase()!;

    const everything = (b: any) =>
      [b.sections.recap, b.sections.relevant, b.sections.interCharacter].filter(Boolean).join('\n\n');
    const dumpBundle = (b: any) => ({
      path: b.path,
      query: b.query
        ? {
            text: b.query.text,
            retrospective: b.query.retrospective,
            anchors: b.query.anchors,
            window: b.query.window,
            probes: (b.query.probes ?? []).map((p: any) => ({ text: p.text, embedded: !!p.embedding })),
            embedded: !!b.query.embedding,
          }
        : null,
      pool: b.pool.map((c: any) => c.memory.id),
      relevancePool: (b.relevancePool ?? []).map((c: any) => c.memory.id),
      sections: b.sections,
      tokens: b.tokens,
      lines: b.lines,
      pools: b.pools,
      retroSignature: b.retroSignature ?? null,
      debugMemories: b.debugMemories,
      debugInterCharacterMemories: b.debugInterCharacterMemories,
      memoriesIncluded: b.memoriesIncluded,
      interCharacterIncluded: b.interCharacterIncluded,
      trace: b.trace,
      warnings: b.warnings,
    });

    const bundles: any[] = [];
    const steps: unknown[] = [];
    for (const step of c.steps) {
      if (step.op === 'assemble') {
        let preRecall: Record<string, unknown> | undefined;
        if (step.pre) {
          const query = await buildRecallQuery({ userId: spec.userId, text: step.pre.text, signals: step.pre.signals as never });
          let candidates: unknown[] = [];
          let ranked: unknown[] | undefined;
          let trace: unknown[] | undefined;
          if (step.pre.search) {
            const recallContext = step.pre.search.retroContext
              ? buildTurnRecallContext({
                  chat: { id: spec.chat.id } as never,
                  recallSettings: { scopePolicy: 'down-weight', expandRelated: false },
                  turnContext: null,
                  turnTemporal: null,
                  turnRetrospective: true,
                  nowMs: NOW,
                } as never)
              : undefined;
            const out = await searchRecall(LYRA, query, {
              limit: step.pre.search.limit,
              minImportance: step.pre.search.minImportance,
              recallContext,
            } as never);
            candidates = out.candidates;
            ranked = out.ranked;
            trace = out.trace;
            if (step.pre.headWithout) {
              const w = step.pre.headWithout;
              candidates = (candidates as any[]).filter(x => x.memory.id !== w.id).slice(0, w.take);
            }
          }
          if (step.pre.empty) candidates = [];
          preRecall = { query, candidates };
          if (trace && step.pre.search?.retroContext) preRecall.trace = trace;
          if (step.pre.ranked) preRecall.ranked = ranked;
        }
        const over = step.plan as Record<string, any>;
        const plan: Record<string, any> = { ...basePlan, ...over };
        plan.conversations = { maxContext: 32000, standingIds: async () => new Set() };
        if (over.recap) plan.recap = { selection: SELECTION, fallbackQuery: over.recap.fallbackQuery };
        if (preRecall) plan.preRecall = preRecall;
        const bundle = await assembleRecall(plan as never);
        bundles.push(bundle);
        steps.push({ op: 'assemble', bundle: dumpBundle(bundle) });
      } else if (step.op === 'deliver') {
        const bundle = bundles[step.of];
        let text = everything(bundle);
        if (step.text === 'cutFirstHead') text = text.replace(bundle.lines.head[0].line, '');
        if (step.text === 'nothing') text = '';
        const delivery = await deliverRecall(bundle, {
          characterId: LYRA,
          chat: { id: spec.chat.id, commonplaceRecallHistory: null } as never,
          deliveredText: text,
        });
        steps.push({ op: 'deliver', deliveredText: text, delivery });
      } else {
        const bundle = emptyRecallBundle();
        bundle.sections.retrospective = '### Relevant Past Conversations\n\n#### The Lighthouse (`conv-1`)\n';
        bundle.retroSignature = 'sig';
        const text = `before\n\n${bundle.sections.retrospective.trim()}\n\nafter`;
        const delivery = await deliverRecall(bundle, {
          characterId: LYRA,
          chat: { id: spec.chat.id, commonplaceRecallHistory: null } as never,
          deliveredText: text,
        });
        steps.push({ op: 'deliver', deliveredText: text, delivery });
      }
    }
    await new Promise(resolve => setTimeout(resolve, 50));
    const stamped = (db.prepare('SELECT "id" FROM "memories" WHERE "lastAccessedAt" IS NOT NULL ORDER BY "id"').all() as Array<{ id: string }>).map(r => r.id);
    const ringRow = db.prepare('SELECT "commonplaceRecallHistory" AS h FROM "chats" WHERE "id" = ?').get(spec.chat.id) as { h: string | null };
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });
    lines.push(JSON.stringify({
      name: c.name,
      steps,
      embedTexts,
      canned: [...canned.values()],
      logs: captured,
      stamped,
      ring: ringRow?.h ? JSON.parse(ringRow.h) : null,
    }));
  }

  (global as { Date: DateConstructor }).Date = RealDate;
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`recall-assemble oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('recall-assemble oracle', async () => {
  await main();
}, 240000);
