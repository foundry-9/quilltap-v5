/**
 * @jest-environment node
 *
 * P4.D183 TRANSCRIPT-ROUTE ORACLE: drives v4's REAL `GET` export from
 * `app/api/v1/messages/route.ts` (NEW at v4 `5029075bb`) over a FRESH copy of
 * the committed SALON fixture per case, and emits each response status + body
 * so the Rust port (`api::chat_transcript`) diffs byte-for-byte.
 *
 * The route is loaded whole — `createContextHandler(withCollectionActionDispatch(
 * {transcript: handleTranscript}, handleListMessages))` — so the dispatcher, the
 * `chatId` gate, the ownership 404, the `Number()` coercion and the response
 * envelopes are all v4's own, not this file's idea of them.
 *
 * **It reuses the salon fixture rather than minting a new committed pair.**
 * That is deliberate: the same rows feed `salon_reads`, so the transcript
 * verb's `messages`/`offSceneCharacters` being byte-identical to the chat GET's
 * — the whole reason v4 extracted `projectChatTranscript` — is a property this
 * corpus and that one prove TOGETHER, against one set of bytes.
 *
 * Two plants per case, applied to the fixture COPY on this side and mirrored on
 * the Rust side:
 *
 *   - the two `31436bae4` columns (`chats.transcriptVersion`,
 *     `files.generationKey`), because the committed pair predates both and this
 *     oracle must not depend on whether a jest boot happens to run v4's
 *     migration chain;
 *   - per-case counter and ownership values, which is how the version arms and
 *     the "owned by someone else" arms are posed at all. v5 has ONE user, so a
 *     foreign `userId` is planted rather than assumed — v4's
 *     `chat.userId !== user.id` is a real arm there, not dead code.
 *
 * The clock is TICKING-frozen from NOW_MS (each argless `new Date()` /
 * `Date.now()` advances 1 ms) — the `chain-depth-frozen-clock-artifact` lesson.
 * **Must run under `TZ=UTC`.**
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-tr-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/transcript-route.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/salon.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SALON_MAIN=$V5W/crates/quilltap-web/tests/fixtures/salon-main.db \
 *   QT_FIXTURE_SALON_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/salon-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-transcript-route.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "qt-tr-oracle.*cases/transcript-route\.test\.ts$"
 *
 * ⚠ The filter is anchored on the /tmp MIRROR PATH, not just the basename,
 * and it is deliberately loose between the stem and `cases/`, because
 * `recipe_sweep.py` appends the FAMILY NAME to `TMPO` when it runs a recipe
 * (`/tmp/qt-tr-oracle-transcript_route_equivalence`). A filter anchored on the
 * bare stem matches by hand and finds NOTHING under the driver, which then
 * reports `regen_failed` on a "No tests found" that looks nothing like its
 * cause. (And the looseness must not be spelled with a bracket-star: the
 * two characters that would end the class and the slash also END THIS BLOCK
 * COMMENT, and swc then reports a syntax error pointing at the line after.)
 *
 * v4 ships its own `__tests__/unit/app/api/v1/messages/transcript-route.test.ts`
 * (the 12 `it` titles this corpus mirrors), and a basename-anchored `--`
 * filter matches BOTH. Harmless today — v4's suite writes no `QT_ORACLE_OUT`,
 * and its passing is a free check that the pin is sound — but a slow or red v4
 * suite would fail this regen for reasons that have nothing to do with it.
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

const SOLO = 'c1000000-0000-4000-8000-000000000001';
const GROUP = 'c1000000-0000-4000-8000-000000000002';
const MISSING = '99999999-9999-4999-8999-999999999999';
const FOREIGN_USER = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const NOW_MS = 1_760_000_000_000;

const RealDate = Date;

/** v4's route handlers only ever read the URL — no body, no params. */
function mockRequest(url: string): unknown {
  return {
    method: 'GET',
    url,
    nextUrl: new URL(url),
    headers: new Headers(),
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
  // `renderedHtml` is the locked markdown-render divergence — the port omits
  // it, so the oracle must not emit it either. `canPreRenderMessage` false
  // makes v4 leave it null on every row, which is what the Rust side produces.
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

/** What a case may plant on its private fixture copy before the route runs. */
interface Plant {
  /** `chats.transcriptVersion` for a given chat id. */
  version?: { chatId: string; value: number | null };
  /** `chats.userId` for a given chat id — poses v4's ownership arm. */
  owner?: { chatId: string; userId: string };
}

interface CaseSpec {
  name: string;
  url: string;
  plant?: Plant;
}

/**
 * v4's 12 `it` titles, one case each. The names are the oracle's keys and the
 * Rust side asserts on them, so they are a contract.
 */
const CASES: CaseSpec[] = [
  // ── handleTranscript ──────────────────────────────────────────────────────
  {
    name: 'unchanged_without_projecting',
    url: `?chatId=${SOLO}&action=transcript&knownVersion=7`,
    plant: { version: { chatId: SOLO, value: 7 } },
  },
  {
    name: 'whole_transcript_when_the_version_moved',
    url: `?chatId=${SOLO}&action=transcript&knownVersion=6`,
    plant: { version: { chatId: SOLO, value: 7 } },
  },
  {
    name: 'whole_transcript_when_no_version_is_offered',
    url: `?chatId=${SOLO}&action=transcript`,
    plant: { version: { chatId: SOLO, value: 7 } },
  },
  {
    name: 'not_an_integer_is_not_trusted',
    url: `?chatId=${SOLO}&action=transcript&knownVersion=7.5`,
    plant: { version: { chatId: SOLO, value: 7 } },
  },
  {
    name: 'not_a_number_at_all_is_not_trusted',
    url: `?chatId=${SOLO}&action=transcript&knownVersion=abc`,
    plant: { version: { chatId: SOLO, value: 7 } },
  },
  {
    // `Number('')` is 0, not NaN — so a bare parameter really does match a
    // chat whose counter has never moved. v4's behaviour, pinned.
    name: 'an_empty_known_version_is_zero',
    url: `?chatId=${SOLO}&action=transcript&knownVersion=`,
    plant: { version: { chatId: SOLO, value: 0 } },
  },
  {
    name: 'a_row_with_no_counter_yet_is_version_zero',
    url: `?chatId=${SOLO}&action=transcript&knownVersion=0`,
    plant: { version: { chatId: SOLO, value: null } },
  },
  {
    name: 'off_scene_author_cards_travel_with_the_transcript',
    url: `?chatId=${GROUP}&action=transcript`,
  },
  {
    name: 'transcript_404s_a_chat_owned_by_someone_else',
    url: `?chatId=${SOLO}&action=transcript`,
    plant: { owner: { chatId: SOLO, userId: FOREIGN_USER } },
  },
  {
    name: 'transcript_404s_a_chat_this_user_cannot_see',
    url: `?chatId=${MISSING}&action=transcript`,
  },
  {
    name: 'transcript_requires_a_chat_id',
    url: `?action=transcript`,
  },
  // ── handleListMessages (the default leg) ──────────────────────────────────
  {
    name: 'listing_returns_stored_message_events',
    url: `?chatId=${SOLO}`,
  },
  {
    name: 'listing_404s_a_chat_owned_by_someone_else',
    url: `?chatId=${SOLO}`,
    plant: { owner: { chatId: SOLO, userId: FOREIGN_USER } },
  },
  {
    name: 'listing_requires_a_chat_id',
    url: ``,
  },
  // ── the dispatcher itself ─────────────────────────────────────────────────
  {
    // MEASURED, and not what the wiring suggests: `withCollectionActionDispatch`
    // has a DEFAULT handler here, yet an unrecognised action does NOT fall
    // through to it — `withActionDispatch` tests `if (action)` first.
    name: 'an_unknown_action_is_refused_not_listed',
    url: `?chatId=${SOLO}&action=no-such-action`,
  },
  {
    // …while a present-but-EMPTY action is JS-falsy and takes the default leg,
    // so this must equal `listing_returns_stored_message_events`.
    name: 'an_empty_action_lists_like_an_absent_one',
    url: `?chatId=${SOLO}&action=`,
  },
];

/** Plant the two `31436bae4` columns + this case's values on the copy. */
function plantOnCopy(mainPath: string, pepper: string, plant: Plant | undefined): void {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const Database = require(
    require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    ),
  );
  const db = new Database(mainPath);
  db.pragma(`key="x'${Buffer.from(pepper, 'base64').toString('hex')}'"`);
  const cols = (table: string): string[] =>
    (db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map((c) => c.name);
  if (!cols('chats').includes('transcriptVersion')) {
    db.exec('ALTER TABLE "chats" ADD COLUMN "transcriptVersion" INTEGER DEFAULT 0');
  }
  if (!cols('files').includes('generationKey')) {
    db.exec('ALTER TABLE "files" ADD COLUMN "generationKey" TEXT');
  }
  if (plant?.version) {
    db.prepare('UPDATE "chats" SET "transcriptVersion" = ? WHERE "id" = ?').run(
      plant.version.value,
      plant.version.chatId,
    );
  }
  if (plant?.owner) {
    db.prepare('UPDATE "chats" SET "userId" = ? WHERE "id" = ?').run(
      plant.owner.userId,
      plant.owner.chatId,
    );
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

  const work = mkdtempSync(join(scratch, 'tr-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  plantOnCopy(mainWork, spec.testPepperBase64, c.plant);
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
      if (a.length === 0) super(NOW_MS + tick++);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return NOW_MS + tick++;
    }
  } as unknown as DateConstructor;

  try {
    const route = (await import('@/app/api/v1/messages/route')) as {
      GET: (req: unknown) => Promise<{ status: number; json: () => Promise<unknown> }>;
    };
    const resp = await route.GET(mockRequest(`http://localhost/api/v1/messages${c.url}`));
    // `NextResponse.json(x).json()` hands back `x` BY REFERENCE, so a later
    // mutation of the handler's object would rewrite an already-"serialized"
    // body. Round-trip through JSON to take a real copy, and to pin the key
    // ORDER the wire would carry.
    const body = JSON.parse(JSON.stringify(await resp.json())) as unknown;
    return { name: c.name, status: resp.status, body };
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
    throw new Error(`transcript-route oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'salon.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SALON_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SALON_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-tr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const lines: string[] = [];
  for (const c of CASES) {
    lines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  rmSync(scratch, { recursive: true, force: true });
  // eslint-disable-next-line no-console
  console.log(`transcript-route oracle wrote ${outPath} (${lines.length} cases)`);
}

it('emits the transcript-route oracle', async () => {
  await main();
}, 600_000);
