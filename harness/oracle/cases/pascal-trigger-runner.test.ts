/**
 * @jest-environment node
 *
 * P4.D305 ORACLE (v4 `362426c8f`): drives v4's REAL `runTriggeredCustomTools`
 * (lib/pascal/trigger-runner.ts) — with the REAL roster, run service, Pascal /
 * Prospero writers and repositories; ONLY the logger is spliced — over a FRESH
 * copy of the committed `pascal-run-custom-{main,mount}.db` fixture per case,
 * in its trigger room (`c1…07`, whose project carries the Trigger Shelf).
 *
 * Per case: the params, the frozen `nowMs` (v4 reads `Date.now()` ONCE per
 * message, `:92`; the Rust side passes the recorded value), the posted message
 * ids (as positions into the dumped rows), the room's `chat_messages` system
 * rows in INSERTION order (`rowid` — two posts may share a `createdAt`
 * millisecond), the stores after the run, and the pinned log lines. The runner
 * NEVER throws, so its failures are log-only — the lines are the evidence.
 *
 * Run (Node 24, from the v4 checkout — mirror the case file to /tmp; jest
 * ignores `.claude/`):
 *   QT_FIXTURE_PASCAL_MAIN=<copy>/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=<copy>/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-trigger-runner.ndjson \
 *     npx jest --silent --roots "$PWD" --roots "$M/cases" --runTestsByPath "$M/cases/pascal-trigger-runner.test.ts"
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

const CHAT = 'c1000000-0000-4000-8000-000000000007';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';
const CHAR_C = 'a1000000-0000-4000-8000-00000000000c';
const CHAR_D = 'a1000000-0000-4000-8000-00000000000d';
const CHAR_MISSING = 'a1000000-0000-4000-8000-0000000000ee';
const GROUP = 'a2000000-0000-4000-8000-0000000000aa';
const PROJECT = 'a3000000-0000-4000-8000-0000000000bb';
const P7_A = 'e7000000-0000-4000-8000-00000000000a';
const P7_B = 'e7000000-0000-4000-8000-00000000000b';

/** 2026-08-29T12:00:00Z, the handler family's anchor. */
const NOW_MS = 1_788_004_800_000;

const ON = { customTools: true, customToolTriggers: true };

interface CaseSpec {
  name: string;
  speaker: { participantId: string | null; characterId: string | null };
  content: string;
  chatSettings: { customTools?: boolean; customToolTriggers?: boolean } | null;
  /** SQL planted on the main copy before the open (the Rust side runs the same). */
  plant?: string[];
}

/** Every `chat_messages` insert aborts: the deal lands, the Pascal post (and
 *  Prospero's report of it) cannot. */
const PLANT_POST_FAILS = [
  'CREATE TRIGGER "p4d305_no_posts" BEFORE INSERT ON "chat_messages" BEGIN SELECT RAISE(ABORT, \'planted: no posts\'); END',
];

/** The lines this family pins: the runner's and the run service's. */
const PINNED_MESSAGES = new Set([
  'Trigger-phrase scan failed; no custom tools fired',
  'Trigger-phrase scan skipped — speaking character not found',
  'Trigger-phrase scan skipped — speaking character unreadable',
  'Trigger phrases over the per-message cap; later tools not fired',
  'Triggered custom tool refused',
  'Triggered custom tool ran but its announcement did not land',
  'Custom tool fired by trigger phrase',
  'Custom tool run: state cascade unavailable, defaulting to {}',
  'Custom tool run refused',
  'Custom tool dealt',
]);

const A = { participantId: P7_A, characterId: CHAR_A };
const B = { participantId: P7_B, characterId: CHAR_B };
const OPERATOR = { participantId: null, characterId: null };

const CASES: CaseSpec[] = [
  // ---- the gate: silent, and nothing is read --------------------------------
  { name: 'gate-off', speaker: A, content: 'abracadabra', chatSettings: { customTools: true, customToolTriggers: false } },
  { name: 'gate-master-off', speaker: A, content: 'abracadabra', chatSettings: { customTools: false, customToolTriggers: true } },
  { name: 'gate-null-settings', speaker: A, content: 'abracadabra', chatSettings: null },
  { name: 'gate-custom-tools-absent', speaker: A, content: 'abracadabra', chatSettings: { customToolTriggers: true } },
  { name: 'blank-content', speaker: A, content: '   \n  ', chatSettings: ON },
  // ---- perspective --------------------------------------------------------
  // CHAR_A's sheet carries `blessed: true`; CHAR_B's does not — the roster's
  // gate decides who can set `seer` off.
  { name: 'gated-tool-blessed-speaker', speaker: A, content: 'Let me consult the oracle.', chatSettings: ON },
  { name: 'gated-tool-unblessed-speaker', speaker: B, content: 'Let me consult the oracle.', chatSettings: ON },
  // The bare operator rolls as nobody: a default-whisper tool goes to [userId].
  { name: 'operator-default-whisper', speaker: OPERATOR, content: 'Keep it quiet, please.', chatSettings: ON },
  // A user-driven CHARACTER seat speaks AS its character (its vault, its sheet).
  { name: 'user-driven-seat', speaker: B, content: 'Open Sesame!', chatSettings: ON },
  { name: 'missing-character', speaker: { participantId: P7_A, characterId: CHAR_MISSING }, content: 'abracadabra', chatSettings: ON },
  { name: 'broken-vault', speaker: { participantId: P7_A, characterId: CHAR_D }, content: 'abracadabra', chatSettings: ON },
  // ---- the deal -----------------------------------------------------------
  // Two hits in APPEARANCE order (stoke before wind, though the roster lists
  // wind first); both write a progression, so their `updatedAt` stamps are the
  // ONE `nowMs` the runner read for the message.
  { name: 'two-hits-one-clock', speaker: A, content: 'Stoke the fire, then wind the clock.', chatSettings: ON },
  { name: 'cap-four-and-dropped', speaker: A, content: 'xyzzy1 xyzzy2 xyzzy3 xyzzy4 xyzzy0', chatSettings: ON },
  // A required parameter with no default is REFUSED (Prospero), and the next
  // hit still runs.
  { name: 'refused-then-good', speaker: A, content: 'I draw my sword — abracadabra!', chatSettings: ON },
  { name: 'carina-line-only', speaker: A, content: '@Alice: abracadabra', chatSettings: ON },
  { name: 'carina-line-then-spoken', speaker: A, content: '@Alice: what is it?\nAbracadabra!', chatSettings: ON },
  { name: 'disabled-tool', speaker: A, content: 'sleeping words', chatSettings: ON },
  // `private: undefined` on a default-whisper tool → targeted to the speaker.
  { name: 'character-default-whisper', speaker: A, content: 'keep it quiet', chatSettings: ON },
  // A consult-bearing tool: the fixture holds no connection profiles, so the
  // consult fails soft on v4's `no connection profiles are configured` arm and
  // the table branches on it.
  { name: 'consult-bearing', speaker: A, content: 'read the omens', chatSettings: ON },
  { name: 'no-match', speaker: A, content: 'nothing to see here', chatSettings: ON },
  // The unposted arm: the roll happened (its effects stand), the announcement
  // did not land — the DEBUG, then Prospero (whose own post fails too).
  { name: 'trigger-unposted', speaker: A, content: 'abracadabra', chatSettings: ON, plant: PLANT_POST_FAILS },

  // ---- P4.D319 (v4 `a05c6f755`): `{{character}}` + `announceAsCharacter` ----
  // The runner supplies the OVERLAID row's name (no service read): Bertie, the
  // row announced as CHAR_A. A padded RAW name (a per-case plant — `name` is not
  // vault-managed, so the overlay carries it through) is trimmed. CHAR_D's
  // broken vault still WARN-skips the scan before any name is read.
  { name: 'announced-dresser-overlaid-name', speaker: A, content: 'Time to dress the part.', chatSettings: ON },
  { name: 'announced-dresser-padded-name', speaker: A, content: 'Dress the part!', chatSettings: ON, plant: [`UPDATE "characters" SET "name" = '  Bertie  ' WHERE "id" = '${CHAR_A}'`] },
  { name: 'announced-dresser-broken-vault', speaker: { participantId: P7_A, characterId: CHAR_D }, content: 'dress the part', chatSettings: ON },
];

async function dumpStores(chatId: string): Promise<Record<string, unknown>> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { readGeneralState } = await import('@/lib/mount-index/general-state');
  const repos = getRepositories();
  const chat = (await repos.chats.findById(chatId)) as { state?: unknown; projectId?: string } | null;
  const project = chat?.projectId
    ? ((await repos.projects.findById(chat.projectId)) as { state?: unknown } | null)
    : null;
  const group = (await repos.groups.findById(GROUP)) as { state?: unknown } | null;
  const metadata: Record<string, unknown> = {};
  for (const [label, id] of [['A', CHAR_A], ['B', CHAR_B], ['C', CHAR_C]] as const) {
    try {
      const character = (await repos.characters.findById(id)) as { metadata?: unknown } | null;
      metadata[label] = character?.metadata ?? null;
    } catch {
      metadata[label] = null;
    }
  }
  return {
    chat: chat?.state ?? null,
    project: project?.state ?? null,
    group: group?.state ?? null,
    general: await readGeneralState(),
    metadata,
  };
}

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  // Hold v4's job host off, so nothing claims a job mid-case (memory note
  // `v4-oracle-job-host-and-log-capture` §1).
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };

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
  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });

  const work = mkdtempSync(join(scratch, 'trigger-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  if (c.plant) {
    const Cipher = jest.requireActual(cipherDriverPath);
    const conn = new Cipher(mainWork);
    conn.pragma(`key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`);
    for (const sql of c.plant) conn.exec(sql);
    conn.close();
  }
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  process.env.LOG_LEVEL = 'debug';
  const { logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      if (!PINNED_MESSAGES.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
      if (data.error) fields.error = data.error.message;
      logs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { runTriggeredCustomTools } = await import('@/lib/pascal/trigger-runner');

  await initializeDatabase();

  try {
    const params = {
      userId: spec.userId,
      chatId: CHAT,
      content: c.content,
      speaker: c.speaker,
      projectId: PROJECT,
      characterIds: [CHAR_A, CHAR_B],
      chatSettings: c.chatSettings,
    };
    const plant = c.plant ?? null;
    const onPosted: string[] = [];
    const realNow = Date.now;
    Date.now = () => NOW_MS;
    let posted: Array<{ id: string }>;
    try {
      posted = (await runTriggeredCustomTools({
        ...params,
        onPosted: (m: { id: string }) => onPosted.push(m.id),
      } as never)) as never;
    } finally {
      Date.now = realNow;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));

    const mdb = getRawDatabase();
    if (!mdb) throw new Error('main DB handle unavailable');
    const columns = (
      mdb.prepare(`PRAGMA table_info(chat_messages)`).all() as Array<{ name: string }>
    ).map((col) => col.name);
    const rawRows = mdb
      .prepare(`SELECT * FROM chat_messages WHERE chatId = ? ORDER BY rowid`)
      .all(CHAT) as Array<Record<string, unknown>>;
    const rows = rawRows.map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    });
    const position = (id: string) => rows.findIndex((r) => r.id === id);

    return {
      name: c.name,
      params,
      plant,
      nowMs: NOW_MS,
      posted: posted.map((m) => position(m.id)),
      onPosted: onPosted.map(position),
      rows,
      stores: await dumpStores(CHAT),
      logs,
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
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-trigger-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;

  const out = fs.createWriteStream(outPath);
  for (const c of CASES) {
    const row = await runCase(spec, c, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal trigger-runner oracle', async () => {
  await main();
}, 300000);
