/**
 * @jest-environment node
 *
 * P4.D115 SCENARIO-CHANGE route-surface ORACLE: drives v4's REAL
 * `handleSetScenario` (`app/api/v1/chats/[id]/actions/scenario.ts`, NEW at v4
 * `44a8137e`) and v4's REAL `resolveScenarioSelection`
 * (`lib/chat/scenario-selection.ts`, also new there) over a FRESH copy of the
 * committed chat-scenario fixture per case, and emits each response body plus
 * post-mutation table dumps so the Rust port
 * (`services::chat_scenario` + `services::scenario_selection`) diffs
 * byte-for-byte.
 *
 * Two sections, both against the same fixture:
 *
 *   `verb_*`      — the route, `POST /api/v1/chats/[id]?action=scenario`,
 *                   loaded through v4's own route module so the Zod parse, the
 *                   404 gate and the response envelope are v4's. Each mutating
 *                   case dumps the chat row AND every chat event, which is where
 *                   the Host `scenario-change` announcement and the recompiled
 *                   `compiledIdentityStacks` land — the WRITES are the
 *                   discriminator for the no-op arm, not the body.
 *   `resolver_*`  — `resolveScenarioSelection` called directly, for the arms the
 *                   verb cannot express (an explicitly-passed character, a
 *                   `projectId` the chat does not carry) and for the precedence
 *                   chain in isolation. NOTHING is mocked: the tiers resolve out
 *                   of the fixture's real stores.
 *
 * Three `export_*` cases run `?action=export-markdown` AFTER a scenario change,
 * which is how the `scenario-change` kind's arrival in `HOST_LINK_KINDS`
 * (`lib/export/markdown-transcript.ts`) is proven end-to-end: the notice the
 * verb just wrote has to survive into the transcript.
 *
 * The clock is TICKING-frozen from NOW_MS (each argless `new Date()` /
 * `Date.now()` advances 1 ms) so messages written inside one case order by
 * insertion on both sides rather than by a tie-break artifact — the
 * `chain-depth-frozen-clock-artifact` lesson.
 *
 * **Must run under `TZ=UTC`** — the house convention for every clock-touching
 * oracle. The guard below fails loudly rather than emitting a locale-shifted
 * fixture.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cs-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-scenario-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-scenario-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-scenario-main.db \
 *   QT_FIXTURE_CS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-scenario-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-scenario.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-scenario-routes\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  frozenNowMs: number;
  generalBody: string;
  projectBody: string;
  groupBody: string;
  beaScenarioOneBody: string;
  beaScenarioTwoBody: string;
  drewScenarioBody: string;
  freeTextNotes: string;
}

// Pinned ids (shared verbatim with the builder + the Rust differential).
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BEA = 'a1000000-0000-4000-8000-000000000002';

/**
 * The character-scenario ids are MINTED by v4's vault projection at fixture
 * build time (fresh per build), so they arrive through the fixture's
 * `<main>.meta.json` sidecar keyed by title — the `chat-admin-web` precedent.
 */
interface FixtureMeta {
  beaScenarioIds: Record<string, string>;
  drewScenarioIds: Record<string, string>;
}
/** A well-formed uuid that is on nobody's `scenarios`. */
const SCEN_DANGLING = '51000000-0000-4000-8000-0000000000de';

const PROJ = '71000000-0000-4000-8000-000000000001';
const PROJ_BARE = '71000000-0000-4000-8000-000000000002';
const GRP = '72000000-0000-4000-8000-000000000001';
const GRP_DANGLING = '72000000-0000-4000-8000-0000000000de';

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const NULL_CHAT = 'c1000000-0000-4000-8000-000000000002';
const BARE_CHAT = 'c1000000-0000-4000-8000-000000000003';
const BROKEN_CHAT = 'c1000000-0000-4000-8000-000000000004';
const MISSING_ID = '99999999-9999-4999-8999-999999999999';

const GENERAL_PATH = 'Scenarios/welcome.md';
const GENERAL_BLANK_PATH = 'Scenarios/blank.md';
const PROJECT_PATH = 'Scenarios/tavern.md';
const GROUP_PATH = 'Scenarios/quay.md';
const GONE_PATH = 'Scenarios/gone.md';

const RealDate = Date;

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
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  // The chat route pulls the markdown renderer (unified/ESM) transitively — mock
  // it away; no case under test renders HTML.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  // Keep the background-jobs processor OFF so it can't claim a row and race the
  // dump (the P4.6y lesson).
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
    startProcessor: () => {},
    stopProcessor: () => {},
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
}

/** The hydrated chat row — `scenarioText` and `compiledIdentityStacks` live here. */
async function readChat(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  return ((await getRepositories().chats.findById(chatId)) ?? null) as unknown;
}

/** Every chat event, in order — where the Host `scenario-change` bubble lands. */
async function readMessages(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  return (await getRepositories().chats.getMessages(chatId)) as unknown;
}

/** [P4.D295] The scenario lines a case logs (L21–L33 + the location's). */
const LINE_PREFIXES = [
  '[Chats v1] ',
  '[ScenarioLocation] ',
  'Scenario file body is empty; skipping',
  'Scenarios: multiple isDefault: true entries',
];
let capturedLines: string[] = [];

function renderField(k: string, v: unknown): string {
  if (v === null || v === undefined) return ` ${k}=null`;
  if (Array.isArray(v) || typeof v === 'object') return ` ${k}Json=${JSON.stringify(v)}`;
  return ` ${k}=${String(v)}`;
}

async function installCapture(): Promise<void> {
  capturedLines = [];
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message?: string };
      };
      if (!LINE_PREFIXES.some((p) => data.message.startsWith(p))) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      let line = `${data.level.toUpperCase()} ${data.message}`;
      for (const [k, v] of Object.entries(fields)) line += renderField(k, v);
      if (data.error) line += ` error=${data.error.message}`;
      capturedLines.push(line);
    },
  });
}

/** [P4.D295] A raw main-DB statement on the per-case copy (a plant). */
async function mainExec(sql: string, ...args: unknown[]): Promise<void> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  };
  main.prepare(sql).run(...args);
}

/** [P4.D295] Plant a chat's stored recipe (the three columns, raw). */
function plantRecipe(
  chatId: string,
  text: string | null,
  source: { mountPointId: string; path: string; bodySha256: string } | null,
  notes: string | null,
) {
  return () =>
    mainExec(
      'UPDATE "chats" SET "scenarioText" = ?, "scenarioSource" = ?, "scenarioNotes" = ? WHERE "id" = ?',
      text,
      source === null ? null : JSON.stringify(source),
      notes,
      chatId,
    );
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
  /** [P4.D295] A plant run after the DB opens, before the capture starts. */
  prep?: () => Promise<void>;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';

/**
 * P4.D183: bring the committed fixture COPY up to the schema v4's code at the
 * pin expects, the way v4's migration chain would on a real instance.
 *
 * **Why this had to exist.** The v5 side of this family heals the same vintage
 * through `test_support::ensure_p4d171_columns`; the ORACLE side never gained
 * the matching heal, so every case that WRITES — v4's scenario change posts a
 * Host announcement through `addMessage`, into a `chat_messages` table with no
 * `routeTrail` column — answered a 500 and the family recorded
 * `{"error":"Internal server error"}` as though that were v4's contract.
 * 18 of its 50 cases, all of them the ones that change anything.
 *
 * It stayed hidden because the family SKIPs (and so passes) when
 * `QT_ORACLE_CHAT_SCENARIO` is unset, and nothing had regenerated it since
 * P4.D171 moved the schema. MEASURED identical at both the `f4ad2c8d1`
 * baseline and the `31436bae4` target — 18 either way — so it is fixture rot,
 * not this round's drift.
 *
 * `transcriptVersion` is planted alongside them because `5029075bb` makes
 * every one of those writes announce; v4 swallows a failed bump, so it is not
 * what was breaking this, but a fixture that lies about the schema is what
 * produced the bug above.
 */
function healVintageColumns(mainPath: string, pepperB64: string): void {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const Database = require(
    require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    ),
  );
  const db = new Database(mainPath);
  db.pragma(`key="x'${Buffer.from(pepperB64, 'base64').toString('hex')}'"`);
  const has = (table: string, column: string): boolean =>
    (db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).some(
      (r) => r.name === column,
    );
  if (!has('chat_messages', 'routeTrail')) {
    db.exec('ALTER TABLE "chat_messages" ADD COLUMN "routeTrail" TEXT');
  }
  if (!has('chats', 'cycleOrderParticipantIds')) {
    db.exec('ALTER TABLE "chats" ADD COLUMN "cycleOrderParticipantIds" TEXT');
  }
  if (!has('chats', 'transcriptVersion')) {
    db.exec('ALTER TABLE "chats" ADD COLUMN "transcriptVersion" INTEGER DEFAULT 0');
  }
  db.close();
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'cs-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  healVintageColumns(mainWork, spec.testPepperBase64);
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
    if (c.prep) await c.prep();
    await installCapture();
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
      lines: capturedLines,
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
      `chat-scenario-routes oracle must run under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-scenario-web.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const meta = JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as FixtureMeta;
  const SCEN_BEA_1 = meta.beaScenarioIds['The parlour'];
  const SCEN_BEA_2 = meta.beaScenarioIds['Watch-change'];
  const SCEN_DREW = meta.drewScenarioIds['The vacated room'];
  for (const [k, v] of Object.entries({ SCEN_BEA_1, SCEN_BEA_2, SCEN_DREW })) {
    if (!v) throw new Error(`fixture sidecar is missing ${k}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cs-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // [P4.D295] DEBUG: L27 / L29 / L30 / L33 and the location's L1 are DEBUG.
  process.env.LOG_LEVEL = 'debug';

  const B = 'http://localhost/api/v1';
  const loadRoute = async (): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> =>
    (await import(CHAT_ROUTE)) as never;
  const post = async (chatId: string, body: unknown) =>
    (await loadRoute()).POST(mockRequest(`${B}/chats/${chatId}?action=scenario`, body), {
      params: Promise.resolve({ id: chatId }),
    });
  /**
   * `GET ?action=export-markdown`, reduced to `{status, bodyText}`. The jest
   * `next/server` shim exposes the raw body string on `.body` (no `.text()`);
   * a real NextResponse has `.text()`. Both are handled, as the sibling
   * chat-dialogs-export oracle does.
   */
  const exportMarkdown = async (chatId: string): Promise<{ status: number; body: unknown }> => {
    const resp = (await (await loadRoute()).GET(
      mockRequest(`${B}/chats/${chatId}?action=export-markdown`),
      { params: Promise.resolve({ id: chatId }) },
    )) as { status: number; body?: unknown; text?: () => Promise<string> };
    const bodyText =
      typeof resp.text === 'function' ? await resp.text() : String(resp.body ?? '');
    return { status: resp.status, body: { bodyText } };
  };

  /** The chat row + every event — the writes the no-op arm must NOT make. */
  const dump = async (chatId = CHAT) => ({
    chat: await readChat(chatId),
    messages: await readMessages(chatId),
  });

  /** A `?action=scenario` case that dumps its writes. */
  const verb = (name: string, chatId: string, body: unknown): CaseSpec => ({
    name,
    run: async () => {
      const { status, body: out } = await respond(await post(chatId, body));
      return { status, body: out, tables: await dump(chatId) };
    },
  });
  /** A refusal case: no writes are expected, so nothing is dumped. */
  const refusal = (name: string, chatId: string, body: unknown): CaseSpec => ({
    name,
    run: async () => respond(await post(chatId, body)),
  });

  /**
   * `resolveScenarioSelection` called directly — v4's REAL module, nothing
   * mocked. Since `545e299bb` it takes `{ userId, characterId }` and answers
   * the RECIPE (`{ scenarioText, scenarioSource, scenarioNotes }`); a refused
   * `scenarioRef` THROWS `ScenarioRefRefusedError` (`Invalid scenarioRef: …`).
   */
  const resolver = (
    name: string,
    fields: Record<string, unknown>,
    opts: { projectId?: string | null; characterId?: string | null } = {},
    prep?: () => Promise<void>,
  ): CaseSpec => ({
    name,
    prep,
    run: async () => {
      const { resolveScenarioSelection } = await import('@/lib/chat/scenario-selection');
      const { getRepositories } = await import('@/lib/repositories/factory');
      try {
        const resolved = await resolveScenarioSelection(fields as never, {
          repos: getRepositories(),
          userId: spec.userId,
          projectId: opts.projectId ?? null,
          characterId: opts.characterId ?? null,
          logTag: '[Chats v1]',
        });
        return { status: 200, body: { resolved } };
      } catch (error) {
        return {
          status: 200,
          body: { threw: error instanceof Error ? `${error.name}: ${error.message}` : String(error) },
        };
      }
    },
  });

  /** [P4.D295] `GET ?action=scenario-source`. */
  const getSource = async (chatId: string) =>
    respond(
      await (await loadRoute()).GET(mockRequest(`${B}/chats/${chatId}?action=scenario-source`), {
        params: Promise.resolve({ id: chatId }),
      }),
    );
  const sourceCase = (name: string, chatId: string, prep?: () => Promise<void>): CaseSpec => ({
    name,
    prep,
    run: () => getSource(chatId),
  });
  const verbWithPrep = (name: string, chatId: string, body: unknown, prep: () => Promise<void>): CaseSpec => ({
    ...verb(name, chatId, body),
    prep,
  });

  // [P4.D295] The pinned stores + bodies the recipe rows plant against.
  const GENERAL_MP = '83000000-0000-4000-8000-000000000003';
  const NOT_AN_OWNER = '83000000-0000-4000-8000-0000000000ff';
  const { createHash } = await import('node:crypto');
  const sha = (b: string) => createHash('sha256').update(b, 'utf8').digest('hex');
  const welcome = { mountPointId: GENERAL_MP, path: GENERAL_PATH, bodySha256: sha(spec.generalBody) };
  const OLD_WELCOME = 'The concourse as it once stood, before the revision.';
  const welcomeOld = { mountPointId: GENERAL_MP, path: GENERAL_PATH, bodySha256: sha(OLD_WELCOME) };

  const LONG_PATH = 'S'.repeat(501);

  const cases: CaseSpec[] = [
    // ── the resolver, in isolation (v4's own unit-test list, un-mocked) ──────
    resolver('resolver_nothing_chosen', {}),
    resolver('resolver_character_scenario', { scenarioId: SCEN_BEA_2 }, { characterId: BEA }),
    resolver(
      'resolver_character_beats_every_file_tier',
      {
        scenarioId: SCEN_BEA_1,
        projectScenarioPath: PROJECT_PATH,
        groupScenarioPath: GROUP_PATH,
        groupScenarioGroupId: GRP,
        generalScenarioPath: GENERAL_PATH,
      },
      { characterId: BEA, projectId: PROJ },
    ),
    resolver(
      'resolver_project_beats_group_and_general',
      {
        projectScenarioPath: PROJECT_PATH,
        groupScenarioPath: GROUP_PATH,
        groupScenarioGroupId: GRP,
        generalScenarioPath: GENERAL_PATH,
      },
      { projectId: PROJ },
    ),
    resolver('resolver_group_beats_general', {
      groupScenarioPath: GROUP_PATH,
      groupScenarioGroupId: GRP,
      generalScenarioPath: GENERAL_PATH,
    }),
    resolver('resolver_general_alone', { generalScenarioPath: GENERAL_PATH }),
    resolver('resolver_free_text_alone', { scenario: 'Just the notes.' }),
    resolver('resolver_free_text_beneath_preset', {
      generalScenarioPath: GENERAL_PATH,
      scenario: 'It is raining.',
    }),
    // Fail-soft: every tier falls through rather than refusing.
    resolver(
      'resolver_dangling_scenario_id_falls_through',
      { scenarioId: SCEN_DANGLING, generalScenarioPath: GENERAL_PATH },
      { characterId: BEA },
    ),
    resolver('resolver_project_path_without_project_id', {
      projectScenarioPath: PROJECT_PATH,
      generalScenarioPath: GENERAL_PATH,
    }),
    resolver(
      'resolver_project_without_official_store',
      { projectScenarioPath: PROJECT_PATH, generalScenarioPath: GENERAL_PATH },
      { projectId: PROJ_BARE },
    ),
    resolver('resolver_group_path_without_group_id', {
      groupScenarioPath: GROUP_PATH,
      generalScenarioPath: GENERAL_PATH,
    }),
    resolver('resolver_group_id_unknown', {
      groupScenarioPath: GROUP_PATH,
      groupScenarioGroupId: GRP_DANGLING,
      generalScenarioPath: GENERAL_PATH,
    }),
    resolver(
      'resolver_project_path_unresolvable',
      { projectScenarioPath: GONE_PATH, generalScenarioPath: GENERAL_PATH },
      { projectId: PROJ },
    ),
    resolver('resolver_general_path_body_blank', {
      generalScenarioPath: GENERAL_BLANK_PATH,
      scenario: 'Notes survive.',
    }),
    resolver('resolver_every_tier_fails_free_text_survives', {
      generalScenarioPath: GONE_PATH,
      scenario: 'Notes survive.',
    }),
    // JS truthiness: an EMPTY-STRING pointer is falsy and falls through.
    resolver(
      'resolver_empty_string_scenario_id',
      { scenarioId: '', generalScenarioPath: GENERAL_PATH },
      { characterId: BEA },
    ),
    resolver(
      'resolver_empty_string_project_id',
      { projectScenarioPath: PROJECT_PATH, generalScenarioPath: GENERAL_PATH },
      { projectId: '' },
    ),
    resolver('resolver_empty_string_paths', {
      projectScenarioPath: '',
      groupScenarioPath: '',
      generalScenarioPath: GENERAL_PATH,
    }),

    // ── POST ?action=scenario ───────────────────────────────────────────────
    // NULL_CHAT starts with no scene, so a general pick is a real change here.
    // (On CHAT the general preset IS the seeded scene — that is the no-op arm.)
    verb('verb_general_preset', NULL_CHAT, { generalScenarioPath: GENERAL_PATH }),
    // A path whose file resolves to no body: the tier warns, falls through, and
    // with nothing beneath it the scene is CLEARED.
    verb('verb_general_path_body_blank_clears', CHAT, {
      generalScenarioPath: GENERAL_BLANK_PATH,
    }),
    verb('verb_project_preset', CHAT, { projectScenarioPath: PROJECT_PATH }),
    verb('verb_group_preset', CHAT, {
      groupScenarioPath: GROUP_PATH,
      groupScenarioGroupId: GRP,
    }),
    // BEA is the SECOND character in the cast: v4 searches the whole active
    // cast, so ARIA (no scenarios) must not stop the walk.
    verb('verb_character_preset', CHAT, { scenarioId: SCEN_BEA_1 }),
    // DREW owns it, but DREW's participant is `removed` → nobody owns it → the
    // chain falls through to nothing, which CLEARS the scene.
    verb('verb_character_preset_on_removed_participant', CHAT, { scenarioId: SCEN_DREW }),
    verb('verb_free_text_only', CHAT, { scenario: 'A rooftop in the rain.' }),
    verb('verb_free_text_beneath_preset', CHAT, {
      projectScenarioPath: PROJECT_PATH,
      scenario: 'It is raining.',
    }),
    verb('verb_precedence_character_over_all', CHAT, {
      scenarioId: SCEN_BEA_2,
      projectScenarioPath: PROJECT_PATH,
      groupScenarioPath: GROUP_PATH,
      groupScenarioGroupId: GRP,
      generalScenarioPath: GENERAL_PATH,
    }),
    verb('verb_empty_payload_clears', CHAT, {}),
    verb('verb_blank_free_text_clears', CHAT, { scenario: '   ' }),
    verb('verb_all_fields_null_clears', CHAT, {
      scenario: null,
      scenarioId: null,
      projectScenarioPath: null,
      groupScenarioPath: null,
      groupScenarioGroupId: null,
      generalScenarioPath: null,
    }),
    // The no-op arms: the response says `changed:false` and the TABLES say
    // nothing was written — no announcement, no recompile, no `updatedAt` move.
    verb('verb_noop_same_general_preset', CHAT, { generalScenarioPath: GENERAL_PATH }),
    verb('verb_noop_same_custom_text', CHAT, { scenario: spec.generalBody }),
    verb('verb_noop_already_cleared', NULL_CHAT, {}),
    // Fall-through arms, through the route.
    verb('verb_project_path_without_project', NULL_CHAT, {
      projectScenarioPath: PROJECT_PATH,
      generalScenarioPath: GENERAL_PATH,
    }),
    verb('verb_project_without_official_store', BARE_CHAT, {
      projectScenarioPath: PROJECT_PATH,
      generalScenarioPath: GENERAL_PATH,
    }),
    verb('verb_project_path_unresolvable', CHAT, {
      projectScenarioPath: GONE_PATH,
      generalScenarioPath: GENERAL_BLANK_PATH,
      scenario: spec.freeTextNotes,
    }),
    verb('verb_group_path_without_group_id', CHAT, {
      groupScenarioPath: GROUP_PATH,
      generalScenarioPath: GENERAL_BLANK_PATH,
      scenario: spec.freeTextNotes,
    }),
    verb('verb_dangling_scenario_id', CHAT, {
      scenarioId: SCEN_DANGLING,
      projectScenarioPath: PROJECT_PATH,
    }),
    // The cast walk hits a character whose vault will not open: v4 catches,
    // `logger.debug`s and keeps going, so the change still succeeds.
    verb('verb_unreadable_character_skipped', BROKEN_CHAT, {
      scenarioId: SCEN_DANGLING,
      generalScenarioPath: GENERAL_PATH,
    }),
    // Refusals — v4's flat `Validation error` envelope (an uncaught `.parse`).
    refusal('verb_bad_scenario_id', CHAT, { scenarioId: 'not-a-uuid' }),
    refusal('verb_bad_group_id', CHAT, {
      groupScenarioPath: GROUP_PATH,
      groupScenarioGroupId: 'nope',
    }),
    // P4.162: shape-valid, RFC-INVALID ids (version nibble `0`; variant `c`) —
    // Zod 4's `z.uuid()` is the RFC regex, so both refuse at the parse.
    refusal('verb_scenario_id_uuid_v0', CHAT, {
      scenarioId: '12345678-1234-0234-8234-123456789abc',
    }),
    refusal('verb_scenario_id_uuid_variant_c', CHAT, {
      scenarioId: '12345678-1234-4234-c234-123456789abc',
    }),
    refusal('verb_group_id_uuid_v0', CHAT, {
      groupScenarioPath: GROUP_PATH,
      groupScenarioGroupId: '12345678-1234-0234-8234-123456789abc',
    }),
    refusal('verb_group_id_uuid_variant_c', CHAT, {
      groupScenarioPath: GROUP_PATH,
      groupScenarioGroupId: '12345678-1234-4234-c234-123456789abc',
    }),
    refusal('verb_path_too_long', CHAT, { generalScenarioPath: LONG_PATH }),
    refusal('verb_wrong_type_scenario', CHAT, { scenario: 123 }),
    refusal('verb_wrong_type_path', CHAT, { generalScenarioPath: ['a'] }),
    refusal('verb_chat_missing', MISSING_ID, { generalScenarioPath: GENERAL_PATH }),
    // The COMPOSITE guard order: `handlePost` 404s on a missing chat BEFORE it
    // dispatches, so an invalid body against a missing chat is a 404, not a
    // 400 — even though `handleSetScenario` itself parses first. v5 has no
    // separate route layer, so its verb must reproduce the composite.
    refusal('verb_bad_body_chat_missing', MISSING_ID, { scenarioId: 'not-a-uuid' }),

    // ── P4.D295 (v4 `545e299bb`): the recipe ───────────────────────────────
    // The resolver: refs, the archived character, the new arms.
    resolver('resolver_ref_ok', { scenarioRef: { mountPointId: GENERAL_MP, path: GENERAL_PATH }, scenario: 'Beneath.' }),
    resolver('resolver_ref_missing_falls_through', {
      scenarioRef: { mountPointId: GENERAL_MP, path: GONE_PATH },
      projectScenarioPath: PROJECT_PATH,
    }, { projectId: PROJ }),
    resolver('resolver_ref_refused_bad_path', { scenarioRef: { mountPointId: GENERAL_MP, path: '../x.md' } }),
    resolver('resolver_ref_refused_not_an_owner', { scenarioRef: { mountPointId: NOT_AN_OWNER, path: GENERAL_PATH } }),
    resolver('resolver_ref_malformed_percent_throws', { scenarioRef: { mountPointId: GENERAL_MP, path: '%E0' } }),
    resolver('resolver_ref_percent_decoded', { scenarioRef: { mountPointId: GENERAL_MP, path: 'Scenarios%2Fwelcome.md' } }),
    resolver(
      'resolver_archived_character_scenario_id',
      { scenarioId: SCEN_BEA_2, generalScenarioPath: GENERAL_PATH },
      { characterId: BEA },
      () => mainExec('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?', '2026-05-02T00:00:00.000Z', BEA),
    ),
    resolver(
      'resolver_general_unprovisioned',
      { generalScenarioPath: GENERAL_PATH, scenario: 'Notes alone.' },
      {},
      () => mainExec('DELETE FROM "instance_settings" WHERE "key" = ?', 'generalMountPointId'),
    ),
    resolver('resolver_bad_project_path', { projectScenarioPath: '../x.md', generalScenarioPath: GENERAL_PATH }, { projectId: PROJ }),
    resolver('resolver_nested_group_path', { groupScenarioPath: 'Scenarios/a/b.md', groupScenarioGroupId: GRP }),
    resolver('resolver_general_path_percent_decoded', { generalScenarioPath: 'welc%6Fme' }),
    resolver('resolver_general_path_malformed_percent_throws', { generalScenarioPath: 'welc%E0me' }),
    resolver('resolver_notes_under_a_custom_scene_are_not_notes', { scenario: '  Only notes.  ' }),
    // `?action=scenario` — the two new body fields' Zod floors.
    verb('verb_reload_true_no_source', NULL_CHAT, { generalScenarioPath: GENERAL_PATH, reload: true }),
    refusal('verb_reload_null_refuses', CHAT, { generalScenarioPath: GENERAL_PATH, reload: null }),
    refusal('verb_reload_string_refuses', CHAT, { generalScenarioPath: GENERAL_PATH, reload: 'yes' }),
    verb('verb_scenario_ref_null_is_absent', NULL_CHAT, { scenarioRef: null, generalScenarioPath: GENERAL_PATH }),
    refusal('verb_scenario_ref_malformed_refuses', CHAT, { scenarioRef: { mountPointId: 'x', path: GENERAL_PATH } }),
    refusal('verb_scenario_ref_empty_path_refuses', CHAT, { scenarioRef: { mountPointId: GENERAL_MP, path: '' } }),
    // A ref: ok; refused (400 `Invalid scenarioRef: …`); a malformed `%`.
    verb('verb_ref_ok', NULL_CHAT, { scenarioRef: { mountPointId: GENERAL_MP, path: GENERAL_PATH } }),
    refusal('verb_ref_refused_not_an_owner', CHAT, { scenarioRef: { mountPointId: NOT_AN_OWNER, path: GENERAL_PATH } }),
    refusal('verb_ref_refused_bad_path', CHAT, { scenarioRef: { mountPointId: GENERAL_MP, path: 'Scenarios/a/b.md' } }),
    refusal('verb_ref_malformed_percent', CHAT, { scenarioRef: { mountPointId: GENERAL_MP, path: '%E0' } }),
    // The kept scene: the chat's source file was revised since it was picked
    // (its stored hash is the OLD body's, its text the OLD body + notes).
    verbWithPrep('verb_kept_scene_no_reload', CHAT, { generalScenarioPath: GENERAL_PATH, scenario: 'New notes.' },
      plantRecipe(CHAT, `${OLD_WELCOME}\n\nOld notes.`, welcomeOld, 'Old notes.')),
    verbWithPrep('verb_kept_scene_same_notes_is_a_noop', CHAT, { generalScenarioPath: GENERAL_PATH, scenario: 'Old notes.' },
      plantRecipe(CHAT, `${OLD_WELCOME}\n\nOld notes.`, welcomeOld, 'Old notes.')),
    verbWithPrep('verb_reload_pulls_the_revision', CHAT, { generalScenarioPath: GENERAL_PATH, scenario: 'Old notes.', reload: true },
      plantRecipe(CHAT, `${OLD_WELCOME}\n\nOld notes.`, welcomeOld, 'Old notes.')),
    verbWithPrep('verb_kept_scene_falls_through_when_text_was_edited', CHAT, { generalScenarioPath: GENERAL_PATH },
      plantRecipe(CHAT, 'A hand-edited scene that no longer ends in its notes.', welcomeOld, 'Old notes.')),
    // A provenance-only change: the same words, now traced to their file —
    // written, `changed: true`, no recompile, no Host bubble (textChanged false).
    verbWithPrep('verb_provenance_only_change', CHAT, { generalScenarioPath: GENERAL_PATH },
      plantRecipe(CHAT, spec.generalBody, null, null)),
    // A notes-only change (planted stale notes beside an unchanged text).
    verbWithPrep('verb_notes_only_change', CHAT, { generalScenarioPath: GENERAL_PATH },
      plantRecipe(CHAT, spec.generalBody, welcome, 'Stale notes.')),
    // The whole-recipe no-op.
    verbWithPrep('verb_whole_recipe_noop', CHAT, { generalScenarioPath: GENERAL_PATH },
      plantRecipe(CHAT, spec.generalBody, welcome, null)),
    // ── `GET ?action=scenario-source` ─────────────────────────────────────────
    sourceCase('source_none', NULL_CHAT),
    sourceCase('source_none_keeps_notes', NULL_CHAT, plantRecipe(NULL_CHAT, 'Custom.', null, 'Leftover notes.')),
    sourceCase('source_ok_unchanged', CHAT, plantRecipe(CHAT, spec.generalBody, welcome, null)),
    sourceCase('source_ok_changed', CHAT, plantRecipe(CHAT, `${OLD_WELCOME}\n\nOld notes.`, welcomeOld, 'Old notes.')),
    sourceCase('source_missing_file', CHAT, plantRecipe(CHAT, 'Gone.', { ...welcome, path: GONE_PATH }, null)),
    sourceCase('source_refused_not_an_owner', CHAT, plantRecipe(CHAT, 'Elsewhere.', { ...welcome, mountPointId: NOT_AN_OWNER }, null)),
    sourceCase('source_refused_bad_path', CHAT, plantRecipe(CHAT, 'Nested.', { ...welcome, path: 'Scenarios/a/b.md' }, null)),
    {
      name: 'source_chat_missing',
      run: () => getSource(MISSING_ID),
    },

    // ── the transcript carry (`scenario-change` ∈ HOST_LINK_KINDS) ──────────
    {
      // Change the scene, then export: the revision notice has to survive into
      // the Markdown transcript. RED before `scenario-change` joins v5's set.
      name: 'export_after_scenario_change',
      run: async () => {
        await post(CHAT, { projectScenarioPath: PROJECT_PATH });
        return exportMarkdown(CHAT);
      },
    },
    {
      name: 'export_after_scenario_cleared',
      run: async () => {
        await post(CHAT, {});
        return exportMarkdown(CHAT);
      },
    },
    {
      // The no-op posts nothing, so the transcript is the untouched baseline —
      // the counterexample that keeps the two cases above meaningful.
      name: 'export_after_noop',
      run: async () => {
        await post(CHAT, { generalScenarioPath: GENERAL_PATH });
        return exportMarkdown(CHAT);
      },
    },
  ];

  const lines: string[] = [];
  for (const c of cases) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  process.stderr.write(`wrote ${lines.length} chat-scenario oracle rows to ${outPath}\n`);
}

test('chat-scenario-routes oracle', async () => {
  await main();
});
