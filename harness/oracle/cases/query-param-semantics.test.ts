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
 * Run (Node 24; cp to a /tmp mirror because jest ignores `.claude/`). While v4
 * HEAD is past the oracle baseline the `cd` below must reach a PINNED worktree —
 * that rewrite is the sweep driver's job (`recipe_sweep.py --run <family> --v4
 * "$PIN"`); a committed recipe never names a `/tmp` pin:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-queryparam-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/query-param-semantics.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
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

  // ===================================================================
  // P4.72 — the other seventeen `?action=`-reading edges (P4.67's Tier 1
  // item 3 remainder). Same six shapes; the `known` action is picked so the
  // first-wins probe has something to compare, never for what it says.
  // ===================================================================
  // ===================================================================
  // P4.73's endpoint, added at the follow-ups-round-2 unification (the lane
  // recorded it as coordination-only). v4 `app/api/v1/images/route.ts:161-171`
  // is the FIRST dispatch shape: `getActionParam` compared to the one literal
  // `'generate'`, and EVERY other value — unknown, `?action=` (empty), none —
  // falls through to the upload/import leg. There is no envelope on this
  // route, so every row here is a handler leg and the comparands are the
  // within-tree equalities. `{}` as JSON reaches `importFromUrlSchema.parse`
  // (upload leg) or `generateImageSchema.parse` (generate leg) — both refuse
  // before any repository call, so the row is DB-free on both sides.
  // ===================================================================
  {
    key: 'images_collection_post',
    mod: '@/app/api/v1/images/route',
    method: 'POST',
    path: '/api/v1/images',
    known: 'generate',
    body: {},
  },
  {
    key: 'system_restore_post',
    mod: '@/app/api/v1/system/restore/route',
    method: 'POST',
    path: '/api/v1/system/restore',
    known: 'preview',
    // `uploadId` is supplied so the preview leg and the default restore leg
    // answer DIFFERENT sentences (`Upload not found or expired` vs `mode must
    // be …`). With `{}` both said `uploadId is required` and the first-wins
    // probe compared a constant with itself.
    body: { uploadId: 'oracle-upload' },
  },
  {
    key: 'characters_wardrobe_get',
    mod: '@/app/api/v1/characters/[id]/wardrobe/route',
    method: 'GET',
    path: `/api/v1/characters/${ITEM}/wardrobe`,
    params: { id: ITEM },
    known: 'instructions',
  },
  {
    key: 'character_item_get',
    mod: '@/app/api/v1/characters/[id]/route',
    method: 'GET',
    path: `/api/v1/characters/${ITEM}`,
    params: { id: ITEM },
    known: 'stats',
  },
  {
    key: 'characters_collection_post',
    mod: '@/app/api/v1/characters/route',
    method: 'POST',
    path: '/api/v1/characters',
    known: 'import',
    body: {},
  },
  {
    key: 'embedding_profiles_collection_get',
    mod: '@/app/api/v1/embedding-profiles/route',
    method: 'GET',
    path: '/api/v1/embedding-profiles',
    known: 'list-providers',
  },
  {
    key: 'files_item_get',
    mod: '@/app/api/v1/files/[id]/route',
    method: 'GET',
    path: `/api/v1/files/${ITEM}`,
    params: { id: ITEM },
    known: 'thumbnail',
  },
  // The chat-files POST twice — once per hand-rolled arm
  // (`chats/[id]/files/route.ts:43-52`), because the first-wins probe is
  // per-arm and the two arms read the same parameter through the same
  // hand-rolled truthiness.
  {
    key: 'chat_files_post_link',
    mod: '@/app/api/v1/chats/[id]/files/route',
    method: 'POST',
    path: `/api/v1/chats/${CHAT}/files`,
    params: { id: CHAT },
    known: 'link',
    body: {},
  },
  {
    key: 'chat_files_post_attach',
    mod: '@/app/api/v1/chats/[id]/files/route',
    method: 'POST',
    path: `/api/v1/chats/${CHAT}/files`,
    params: { id: CHAT },
    known: 'attach-mount-file',
    body: {},
  },
  {
    key: 'text_replacements_post',
    mod: '@/app/api/v1/settings/text-replacements/route',
    method: 'POST',
    path: '/api/v1/settings/text-replacements',
    known: 'bulk-replace',
    body: {},
  },
  {
    key: 'conversation_summaries_get',
    mod: '@/app/api/v1/system/conversation-summaries/route',
    method: 'GET',
    path: '/api/v1/system/conversation-summaries',
    known: 'regenerate',
  },
  {
    key: 'conversation_summaries_post',
    mod: '@/app/api/v1/system/conversation-summaries/route',
    method: 'POST',
    path: '/api/v1/system/conversation-summaries',
    known: 'regenerate',
    body: {},
  },
  {
    key: 'system_job_post',
    mod: '@/app/api/v1/system/jobs/[id]/route',
    method: 'POST',
    path: `/api/v1/system/jobs/${ITEM}`,
    params: { id: ITEM },
    known: 'pause',
    body: {},
  },
  {
    key: 'system_unlock_post',
    mod: '@/app/api/v1/system/unlock/route',
    method: 'POST',
    path: '/api/v1/system/unlock',
    known: 'lock',
    body: {},
  },
  {
    key: 'wardrobe_collection_get',
    mod: '@/app/api/v1/wardrobe/route',
    method: 'GET',
    path: '/api/v1/wardrobe',
    known: 'instructions',
  },
  {
    key: 'wardrobe_collection_post',
    mod: '@/app/api/v1/wardrobe/route',
    method: 'POST',
    path: '/api/v1/wardrobe',
    known: 'instructions',
    body: {},
  },
  {
    key: 'chat_item_get',
    mod: '@/app/api/v1/chats/[id]/route',
    method: 'GET',
    path: `/api/v1/chats/${CHAT}`,
    params: { id: CHAT },
    known: 'get-background',
  },
  {
    key: 'chat_item_post',
    mod: '@/app/api/v1/chats/[id]/route',
    method: 'POST',
    path: `/api/v1/chats/${CHAT}`,
    params: { id: CHAT },
    known: 'equip',
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
 * Whether a `REGENERATE_CONVERSATION_SUMMARIES` job is already "in flight" for
 * this oracle run. Lives OUTSIDE `applyMocks` because `jest.resetModules()`
 * runs between shapes and would reset anything held inside the factory — and
 * the whole point is that the second enqueue of a run sees the first.
 */
let summaryRegenerationEnqueued = false;

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
      findById: async (id: string) => ({
        id,
        userId: USER,
        chatType: 'brahma',
        title: 'Oracle',
        participants: [],
        projectId: null,
        createdAt: new Date(0).toISOString(),
        updatedAt: new Date(0).toISOString(),
      }),
      list: async () => [],
      findAll: async () => [],
      countAll: async () => 0,
      hasDangerous: async () => false,
      // `characters/[id]` GET's DEFAULT leg counts the character's chats.
      findByCharacterId: async () => [],
      getMessages: async () => [],
    },
    // Same trap: `characters/[id]` POST runs `findById` → `notFound` BEFORE the
    // action gate, so a null here makes all six shapes a 404.
    characters: {
      findById: async (id: string) => ({ id, userId: USER, name: 'Oracle', isFavorite: false }),
      setFavorite: async (id: string) => ({ id, isFavorite: true }),
    },
    // A NON-resizable file, so `files/[id]?action=thumbnail` refuses
    // (`File is not a resizable image`) while the default leg downloads — with
    // `null` every shape was the same 404 and the first-wins probe measured
    // nothing, and with an image both legs answered the same bytes.
    files: {
      findById: async (id: string) => ({
        id,
        userId: USER,
        mimeType: 'text/plain',
        originalFilename: 'oracle.txt',
        storageKey: `files/${id}.txt`,
        size: 3,
        category: 'general',
      }),
      findByCategory: async () => [],
    },
    mountPoints: { findById: async () => null },
    embeddingProfiles: { findById: async () => null, findByUserId: async () => [] },
    tags: { findAll: async () => [] },
    // ---- P4.72 leaves ----
    // The wardrobe reads the `?action=instructions` routes' DEFAULT legs run.
    wardrobe: {
      findArchetypes: async () => [],
      findByCharacterId: async () => [],
      findArchetypesInMounts: async () => [],
      create: async () => null,
    },
    // `system/jobs/[id]` POST looks the job up only AFTER its action gate, so a
    // null here is the deterministic answer for every SERVED action too.
    backgroundJobs: { findById: async () => null, pause: async () => null, resume: async () => null },
    textReplacementRules: { findAll: async () => [], create: async () => null },
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

  // ---- P4.72 leaves ----
  // `settings/text-replacements` reaches its repositories through
  // `getRepositories` from the OTHER factory module; the rest of that module is
  // kept real so its sibling exports (the conflict error class the route
  // imports) still resolve.
  jest.doMock('@/lib/database/repositories', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/database/repositories'),
    getRepositories: () => repos,
    getRepositoriesSafe: async () => repos,
  }));

  // `system/restore` — the real restore service would touch the filesystem and
  // the databases; dispatch is all this oracle measures.
  jest.doMock('@/lib/backup/restore-service', () => ({
    __esModule: true,
    restore: async () => ({ warnings: [], counts: {} }),
    previewRestore: async () => ({ manifest: {} }),
  }));

  // `system/conversation-summaries` — the enqueue is the SERVED leg's whole
  // body; the refusal legs never reach it.
  //
  // The stub is STATEFUL on purpose. v4's real
  // `enqueueRegenerateConversationSummaries` looks for a PENDING/PROCESSING job
  // of the same type first and answers `isNew: false` with the existing id
  // (`queue-service.ts:745-773`) — so the second `?action=regenerate` POST of a
  // run answers a different sentence than the first, on BOTH trees. A stateless
  // stub said `isNew: true` forever and made v4's `firstWins` a claim about the
  // stub. The first-wins reading for this endpoint is carried instead by the
  // byte-compared `empty_then_known` row (`?action=&action=regenerate` answers
  // the refusal — i.e. the FIRST value won).
  jest.doMock('@/lib/background-jobs/queue-service', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/queue-service'),
    enqueueRegenerateConversationSummaries: async () => {
      const isNew = !summaryRegenerationEnqueued;
      summaryRegenerationEnqueued = true;
      return { jobId: 'oracle-job', isNew };
    },
  }));

  // `system/jobs/[id]` POST's resume leg nudges the processor.
  jest.doMock('@/lib/background-jobs', () => ({
    __esModule: true,
    ensureProcessorRunning: () => {},
  }));

  // `chats/[id]/files` default leg — the upload pipeline.
  jest.doMock('@/lib/chat-files-v2', () => ({
    __esModule: true,
    uploadChatFile: async () => ({ file: { id: 'oracle-file' } }),
  }));

  // `wardrobe` POST's default (create) leg resolves the General mount.
  jest.doMock('@/lib/instance-settings', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/instance-settings'),
    getGeneralMountPointId: async () => null,
  }));
  jest.doMock('@/lib/mount-index/general-wardrobe', () => ({
    __esModule: true,
    ensureGeneralWardrobeFolder: async () => null,
  }));

  // `chats/[id]/handlers/get.ts` imports the markdown renderer, which imports
  // `unified` — ESM, which jest's CJS runtime cannot compile. Pre-rendering
  // plays no part in action dispatch.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async (md: string) => md,
    canPreRenderMessage: () => false,
    escapeMarkdownInBrackets: (s: string) => s,
  }));

  // `characters/[id]/wardrobe` GET's `?scope=group` leg walks the tiered pool.
  jest.doMock('@/lib/mount-index/tiered-mount-pool', () => ({
    __esModule: true,
    resolveGroupMountPointIdsForCharacter: async () => [],
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
