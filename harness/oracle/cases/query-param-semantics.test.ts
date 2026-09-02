/**
 * @jest-environment node
 *
 * P4.67 ORACLE — **how v4 answers the `?action=` family of query shapes** at
 * every URL a v5 REST edge serves.
 *
 * The class this measures was found by the P4.D143 §3 review: v5 reads its
 * query through `Query<HashMap<String, String>>`, which yields `Some("")` for a
 * present-but-empty `?action=` and keeps the **last** value of a repeated key.
 * v4 reads `searchParams.get('action')` — the **first** value — and then gates
 * on JS truthiness, so `''` is *no action at all*. Both differences are
 * invisible to every existing family, because every existing family sends a
 * single well-formed action.
 *
 * ## What each row measures — and why two kinds
 *
 * Some shapes make v4 **refuse**. Those are compared across the trees
 * byte-for-byte: status + whole body, key order included.
 *
 * The rest make v4 run a **default handler** whose body is a payload over
 * database state (the chat list, the profile, the tool library). Comparing
 * those across trees would be comparing fixtures, not dispatch. So for them
 * this oracle records a **within-tree equality** instead — the only claim the
 * lane actually makes:
 *
 * - `fold`      — `?action=` answers exactly what a bare request answers.
 * - `firstWins` — `?action=<a>&action=<b>` answers exactly what `?action=<a>`
 *                 answers (v4's `searchParams.get` reads the FIRST).
 *
 * Both sides compute their own equality over their own state, so the comparand
 * is the BOOLEAN, and no fixture has to match. A `fold` that is `true` on v4
 * and `false` on v5 is precisely the bug.
 *
 * `body` is still recorded for the non-refusal rows, but only so a human can
 * see which leg ran; the Rust side does not compare it (see the family header).
 *
 * ## DB-free
 *
 * Every route here is driven through its REAL exported handler and its REAL
 * dispatch code; only the leaves below it are stubbed (`getRepositoriesSafe`,
 * the session, startup state, and the handful of service modules a default
 * handler reaches). Nothing opens a database.
 *
 * Run (Node 24, from a worktree PINNED at the oracle baseline — the checkout is
 * past it; cp to a /tmp mirror because jest ignores `.claude/`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   PIN=/tmp/qt-v4-pin-p4.67-6d2a50382
 *   TMPO=/tmp/qt-queryparam-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/query-param-semantics.test.ts" "$TMPO/cases/"
 *   cd "$PIN"
 *   QT_ORACLE_OUT=/tmp/oracle-query-param-semantics.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- query-param-semantics
 */

import * as fs from 'fs';

const USER = 'aa000000-0000-4000-8000-0000000000aa';
const CHAT = 'bb000000-0000-4000-8000-0000000000bb';
const ITEM = 'cc000000-0000-4000-8000-0000000000cc';

/** One URL+method a v5 REST edge serves. */
interface Endpoint {
  /** The row-name prefix, and the v5 family's key for this endpoint. */
  key: string;
  /** The v4 route module to import. */
  mod: string;
  method: 'GET' | 'POST' | 'PUT' | 'PATCH';
  /** The pathname (params are supplied separately). */
  path: string;
  /** Next.js dynamic params for the handler's second argument. */
  params?: Record<string, string>;
  /** An action v4 serves — used as the `<a>` of the first-wins probe. */
  known: string;
  /** A body for the methods that read one. */
  body?: unknown;
}

/**
 * Every endpoint whose v5 counterpart reads `?action=`.
 *
 * `known` is deliberately an action whose v4 handler REFUSES cheaply or whose
 * leg is distinguishable — the first-wins probe compares `?action=a&action=b`
 * against `?action=a`, so what matters is that the two agree, not what they say.
 */
const ENDPOINTS: Endpoint[] = [
  {
    key: 'system_tools_get',
    mod: '@/app/api/v1/system/tools/route',
    method: 'GET',
    path: '/api/v1/system/tools',
    known: 'tasks-queue',
  },
  {
    key: 'system_tools_post',
    mod: '@/app/api/v1/system/tools/route',
    method: 'POST',
    path: '/api/v1/system/tools',
    known: 'tasks-queue',
    body: {},
  },
  {
    key: 'system_data_dir_post',
    mod: '@/app/api/v1/system/data-dir/route',
    method: 'POST',
    path: '/api/v1/system/data-dir',
    known: 'open',
    body: {},
  },
  {
    key: 'user_profile_get',
    mod: '@/app/api/v1/user/profile/route',
    method: 'GET',
    path: '/api/v1/user/profile',
    known: 'theme-preference',
  },
  {
    key: 'user_profile_put',
    mod: '@/app/api/v1/user/profile/route',
    method: 'PUT',
    path: '/api/v1/user/profile',
    known: 'theme-preference',
    body: { themePreference: 'system' },
  },
  {
    key: 'user_profile_patch',
    mod: '@/app/api/v1/user/profile/route',
    method: 'PATCH',
    path: '/api/v1/user/profile',
    known: 'set-avatar',
    body: {},
  },
  {
    key: 'brahma_item_patch',
    mod: '@/app/api/v1/brahma-console/[id]/route',
    method: 'PATCH',
    path: `/api/v1/brahma-console/${CHAT}`,
    params: { id: CHAT },
    known: 'set-model',
    body: {},
  },
  {
    key: 'chats_collection_get',
    mod: '@/app/api/v1/chats/route',
    method: 'GET',
    path: '/api/v1/chats',
    known: 'has-dangerous',
  },
  {
    key: 'embedding_profile_item_post',
    mod: '@/app/api/v1/embedding-profiles/[id]/route',
    method: 'POST',
    path: `/api/v1/embedding-profiles/${ITEM}`,
    params: { id: ITEM },
    known: 'refit',
    body: {},
  },
  {
    key: 'mount_point_action_post',
    mod: '@/app/api/v1/mount-points/[id]/route',
    method: 'POST',
    path: `/api/v1/mount-points/${ITEM}`,
    params: { id: ITEM },
    known: 'scan',
    body: {},
  },
  {
    key: 'custom_tools_collection_get',
    mod: '@/app/api/v1/custom-tools/route',
    method: 'GET',
    path: '/api/v1/custom-tools',
    known: 'destinations',
  },
  {
    key: 'custom_tools_collection_post',
    mod: '@/app/api/v1/custom-tools/route',
    method: 'POST',
    path: '/api/v1/custom-tools',
    known: 'preview',
    body: {},
  },
  {
    key: 'chat_custom_tools_post',
    mod: '@/app/api/v1/chats/[id]/custom-tools/route',
    method: 'POST',
    path: `/api/v1/chats/${CHAT}/custom-tools`,
    params: { id: CHAT },
    known: 'run',
    body: {},
  },
  {
    key: 'character_item_post',
    mod: '@/app/api/v1/characters/[id]/route',
    method: 'POST',
    path: `/api/v1/characters/${ITEM}`,
    params: { id: ITEM },
    known: 'favorite',
    body: {},
  },
];

/** The query string each probe appends. */
type Shape = 'bare' | 'empty' | 'unknown' | 'known' | 'known_then_unknown' | 'empty_then_known';

const SHAPES: Shape[] = [
  'bare',
  'empty',
  'unknown',
  'known',
  'known_then_unknown',
  'empty_then_known',
];

const UNKNOWN_ACTION = 'zzz-not-an-action';

function queryFor(shape: Shape, known: string): string {
  switch (shape) {
    case 'bare':
      return '';
    case 'empty':
      return '?action=';
    case 'unknown':
      return `?action=${UNKNOWN_ACTION}`;
    case 'known':
      return `?action=${known}`;
    case 'known_then_unknown':
      return `?action=${known}&action=${UNKNOWN_ACTION}`;
    case 'empty_then_known':
      return `?action=&action=${known}`;
  }
}

function mockRequest(url: string, ep: Endpoint): unknown {
  return {
    method: ep.method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: async () => ep.body ?? {},
    formData: async () => new FormData(),
    text: async () => JSON.stringify(ep.body ?? {}),
  };
}

/**
 * The leaves. Everything a DEFAULT handler reaches is stubbed to something
 * deterministic — the point is which leg ran, never what it found.
 */
function applyMocks(): void {
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: USER } }),
  }));

  const repos = {
    users: {
      findById: async () => ({ id: USER, name: 'Oracle', chatSettings: {} }),
      update: async () => ({ id: USER, name: 'Oracle', chatSettings: {} }),
    },
    chats: {
      // `verifyBrahmaChat` demands ownership AND `chatType === 'brahma'`; a
      // 404 here would answer every shape identically and measure nothing.
      findById: async (id: string) => ({ id, userId: USER, chatType: 'brahma', title: 'Oracle' }),
      list: async () => [],
      findAll: async () => [],
      countAll: async () => 0,
      hasDangerous: async () => false,
    },
    // Same trap: `characters/[id]` POST runs `findById` → `notFound` BEFORE the
    // action gate, so a null here makes all six shapes a 404.
    characters: {
      findById: async (id: string) => ({ id, userId: USER, name: 'Oracle', isFavorite: false }),
      setFavorite: async (id: string) => ({ id, isFavorite: true }),
    },
    files: { findById: async () => null, findByCategory: async () => [] },
    mountPoints: { findById: async () => null },
    embeddingProfiles: { findById: async () => null },
    tags: { findAll: async () => [] },
  };
  jest.doMock('@/lib/repositories/factory', () => ({
    __esModule: true,
    getRepositoriesSafe: async () => repos,
    getRepositories: () => repos,
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

  // `mount-points/[id]/route.ts` imports the mount watcher, which imports
  // chokidar — ESM, which jest's CJS runtime cannot compile. The watcher plays
  // no part in action dispatch.
  jest.doMock('@/lib/mount-index/watcher', () => ({
    __esModule: true,
    startMountWatcher: () => {},
    stopMountWatcher: () => {},
    restartMountWatcher: () => {},
    getMountWatcherStatus: () => ({ watching: false }),
  }));

  // `system/data-dir?action=open` shells out (`execFile('open', …)`). The
  // oracle measures dispatch, and must never launch a file browser.
  jest.doMock('child_process', () => ({
    __esModule: true,
    ...jest.requireActual('child_process'),
    execFile: (_cmd: string, _args: string[], cb?: (e: unknown) => void) => {
      if (typeof cb === 'function') cb(null);
    },
  }));

  // Default-leg leaves.
  jest.doMock('@/lib/pascal/workbench', () => ({
    __esModule: true,
    buildCustomToolLibrary: async () => ({ tools: [], stores: [] }),
    listCustomToolDestinations: async () => ({ destinations: [] }),
  }));
}

interface Row {
  name: string;
  status: number;
  body: unknown;
}

async function runShape(ep: Endpoint, shape: Shape): Promise<Row> {
  jest.resetModules();
  applyMocks();
  const url = `http://localhost${ep.path}${queryFor(shape, ep.known)}`;
  let status = -1;
  let body: unknown = null;
  try {
    const mod = (await import(ep.mod)) as Record<string, (...a: unknown[]) => Promise<unknown>>;
    const handler = mod[ep.method];
    if (typeof handler !== 'function') {
      return { name: `${ep.key}__${shape}`, status: -1, body: `NO ${ep.method} EXPORT` };
    }
    const response = (await handler(mockRequest(url, ep), {
      params: Promise.resolve(ep.params ?? {}),
    })) as { status: number; json: () => Promise<unknown> };
    status = response.status;
    try {
      body = await response.json();
    } catch {
      body = null;
    }
  } catch (error) {
    // A default handler that dies in a stubbed leaf is still a recorded
    // answer — the row's value is the LEG, and a throw is a distinguishable
    // leg. It must be deterministic, which is why the leaves are stubs.
    status = -2;
    body = { threw: error instanceof Error ? error.name : String(error) };
  }
  return { name: `${ep.key}__${shape}`, status, body };
}

function sameAnswer(a: Row, b: Row): boolean {
  return a.status === b.status && JSON.stringify(a.body) === JSON.stringify(b.body);
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';
  const out = fs.createWriteStream(outPath);

  for (const ep of ENDPOINTS) {
    const rows: Partial<Record<Shape, Row>> = {};
    for (const shape of SHAPES) {
      const row = await runShape(ep, shape);
      rows[shape] = row;
      out.write(JSON.stringify(row) + '\n');
    }
    // The two within-tree equalities the Rust side actually compares.
    out.write(
      JSON.stringify({
        name: `${ep.key}__equalities`,
        fold: sameAnswer(rows.empty!, rows.bare!),
        firstWins: sameAnswer(rows.known_then_unknown!, rows.known!),
        // `?action=&action=<known>` — `searchParams.get` reads the FIRST, so
        // this is the EMPTY answer, which is not always the bare one (v4's
        // hand-rolled routes render `null` for absent and `` for empty).
        emptyFirstWins: sameAnswer(rows.empty_then_known!, rows.empty!),
      }) + '\n',
    );
  }

  out.end();
  await new Promise((r) => out.on('finish', r));
}

describe('P4.67 query-parameter semantics oracle', () => {
  it('emits the action-shape rows', async () => {
    await main();
  });
});
