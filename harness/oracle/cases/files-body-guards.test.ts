/**
 * @jest-environment node
 *
 * P4.62 ORACLE — the files-family edges' BODY GUARDS: the caller-input sites the
 * wrong-type-collapse census counts in `crates/quilltap-web/src/files_routes.rs`,
 * measured against v4's REAL route handlers.
 *
 * Three routes, each driven by its real exported handler:
 *   - `mount-points/[id]/files/[...path]` PUT — the JSON leg's `writeBodySchema`
 *   - `files` POST `?action=upload` — the multipart `tags` part
 *   - `chats/[id]/files` POST `?action=link|attach-mount-file` — the two
 *     hand-rolled `!v || typeof v !== 'string'` guards, and the fact that v4
 *     resolves the CHAT before either of them
 *
 * Every arm refuses before any repository write, so the oracle is DB-free: the
 * stubs below answer `chats.findById` (present for `CHAT_OK`, absent otherwise)
 * and `files.findById` (always absent). Nothing here may reach `storeMountFile`
 * or `saveFileEntry`.
 *
 * The `upload_tags_*` arms exercise the ONE tags shape that refuses without a
 * database: `JSON.parse` succeeding on a non-array, whose `.map` then throws
 * into the route's outer catch. The wrong-typed-`tagId` shapes reach
 * `saveFileEntry` and are recorded, not driven (see the Rust test's header).
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-files-guards-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/files-body-guards.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-files-body-guards.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- files-body-guards
 */

import * as fs from 'fs';

const USER = 'aa000000-0000-4000-8000-0000000000aa';
/** The chat the stub resolves; anything else is a 404. */
const CHAT_OK = 'c1000000-0000-4000-8000-000000000001';
const CHAT_MISSING = 'c1000000-0000-4000-8000-0000000000ff';
const MOUNT = 'd1000000-0000-4000-8000-000000000001';

type Kind = 'mount_write' | 'upload' | 'chat_files';

interface CaseSpec {
  name: string;
  kind: Kind;
  /** mount_write: the JSON body. chat_files: the JSON body. */
  body?: unknown;
  /** The body's `json()` REJECTS — v4's `await req.json()` on malformed bytes. */
  badJson?: boolean;
  /** chat_files only. */
  chatId?: string;
  action?: 'link' | 'attach-mount-file';
  /** upload only: the raw `tags` form field. */
  tags?: string;
}

const CASES: CaseSpec[] = [
  // ── mount-file PUT, JSON leg: v4 `writeBodySchema.safeParse` →
  //    `Invalid body: <issue messages joined by ', '>` ──
  { name: 'write_content_absent', kind: 'mount_write', body: {} },
  { name: 'write_content_null', kind: 'mount_write', body: { content: null } },
  { name: 'write_content_number', kind: 'mount_write', body: { content: 7 } },
  { name: 'write_content_object', kind: 'mount_write', body: { content: {} } },
  {
    name: 'write_encoding_bad_enum',
    kind: 'mount_write',
    body: { content: 'hi', encoding: 'hex' },
  },
  {
    name: 'write_encoding_number',
    kind: 'mount_write',
    body: { content: 'hi', encoding: 7 },
  },
  {
    name: 'write_encoding_null',
    kind: 'mount_write',
    body: { content: 'hi', encoding: null },
  },
  {
    name: 'write_mtime_negative',
    kind: 'mount_write',
    body: { content: 'hi', expected_mtime: -5 },
  },
  {
    name: 'write_mtime_float',
    kind: 'mount_write',
    body: { content: 'hi', expected_mtime: 1.5 },
  },
  {
    name: 'write_mtime_string',
    kind: 'mount_write',
    body: { content: 'hi', expected_mtime: '5' },
  },
  { name: 'write_force_string', kind: 'mount_write', body: { content: 'hi', force: 'true' } },
  { name: 'write_force_number', kind: 'mount_write', body: { content: 'hi', force: 1 } },
  // Two failing fields at once — the join order is the SCHEMA's key order.
  {
    name: 'write_two_issues',
    kind: 'mount_write',
    body: { encoding: 'hex', force: 'true' },
  },
  { name: 'write_body_not_json', kind: 'mount_write', badJson: true },
  { name: 'write_body_null', kind: 'mount_write', body: null },
  { name: 'write_body_number', kind: 'mount_write', body: 42 },
  { name: 'write_body_array', kind: 'mount_write', body: [] },

  // ── files upload: the `tags` part ──
  { name: 'upload_tags_not_json', kind: 'upload', tags: '{oops' },
  // Parses, but is not an array — `.map` throws into the outer catch.
  { name: 'upload_tags_object', kind: 'upload', tags: '{"tagId":"x"}' },
  { name: 'upload_tags_number', kind: 'upload', tags: '7' },
  { name: 'upload_tags_string', kind: 'upload', tags: '"x"' },
  // NOT driven: `tags: 'null'` parses to `null`, which is FALSY, so v4 skips the
  // `.map` entirely and reaches `saveFileEntry` — a database call this oracle
  // deliberately does not provide. Its 500 would be the missing mock, not a
  // guard. The Rust side records the shape instead.

  // ── chats/[id]/files: the chat is resolved BEFORE the action's guards ──
  {
    name: 'link_missing_chat',
    kind: 'chat_files',
    action: 'link',
    chatId: CHAT_MISSING,
    body: {},
  },
  { name: 'link_absent', kind: 'chat_files', action: 'link', chatId: CHAT_OK, body: {} },
  {
    name: 'link_null',
    kind: 'chat_files',
    action: 'link',
    chatId: CHAT_OK,
    body: { fileId: null },
  },
  {
    name: 'link_empty',
    kind: 'chat_files',
    action: 'link',
    chatId: CHAT_OK,
    body: { fileId: '' },
  },
  {
    name: 'link_number',
    kind: 'chat_files',
    action: 'link',
    chatId: CHAT_OK,
    body: { fileId: 7 },
  },
  {
    name: 'link_object',
    kind: 'chat_files',
    action: 'link',
    chatId: CHAT_OK,
    body: { fileId: {} },
  },
  {
    name: 'link_unknown_string',
    kind: 'chat_files',
    action: 'link',
    chatId: CHAT_OK,
    body: { fileId: 'f0000000-0000-4000-8000-0000000000ff' },
  },
  {
    name: 'attach_missing_chat',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_MISSING,
    body: {},
  },
  {
    name: 'attach_mount_absent',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_OK,
    body: {},
  },
  {
    name: 'attach_mount_null',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_OK,
    body: { mountPointId: null, relativePath: 'a.md' },
  },
  {
    name: 'attach_mount_number',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_OK,
    body: { mountPointId: 7, relativePath: 'a.md' },
  },
  {
    name: 'attach_mount_empty',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_OK,
    body: { mountPointId: '', relativePath: 'a.md' },
  },
  {
    name: 'attach_path_absent',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_OK,
    body: { mountPointId: MOUNT },
  },
  {
    name: 'attach_path_number',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_OK,
    body: { mountPointId: MOUNT, relativePath: 7 },
  },
  {
    name: 'attach_unknown_file',
    kind: 'chat_files',
    action: 'attach-mount-file',
    chatId: CHAT_OK,
    body: { mountPointId: MOUNT, relativePath: 'nope.md' },
  },
];

function jsonRequest(url: string, c: CaseSpec): unknown {
  return {
    method: c.kind === 'mount_write' ? 'PUT' : 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: c.badJson
      ? async () => {
          throw new SyntaxError('Unexpected end of JSON input');
        }
      : async () => c.body,
  };
}

/** The upload leg reads `formData()`, so the request carries one instead. */
function uploadRequest(url: string, c: CaseSpec): unknown {
  const form = new Map<string, unknown>();
  form.set('file', new File([new Uint8Array([1, 2, 3])], 'note.txt', { type: 'text/plain' }));
  if (c.tags !== undefined) form.set('tags', c.tags);
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'multipart/form-data; boundary=x' }),
    formData: async () => ({ get: (k: string) => form.get(k) ?? null }),
  };
}

function applyMocks(): void {
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: USER } }),
  }));
  const repos = {
    users: { findById: async () => ({ id: USER }) },
    chats: { findById: async (id: string) => (id === CHAT_OK ? { id, userId: USER } : null) },
    files: { findById: async () => null, addLink: async () => null },
    docMountFiles: { findByMountPointAndPath: async () => null },
    docMountBlobs: { findByMountPointAndPath: async () => null },
    docMountPoints: { findById: async () => null },
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
}

async function runCase(c: CaseSpec): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks();
  let response: { status: number; json: () => Promise<unknown> };
  if (c.kind === 'mount_write') {
    const url = `http://localhost/api/v1/mount-points/${MOUNT}/files/notes/a.md`;
    const { PUT } = (await import('@/app/api/v1/mount-points/[id]/files/[...path]/route')) as {
      PUT: (...a: unknown[]) => Promise<unknown>;
    };
    response = (await PUT(jsonRequest(url, c), {
      params: Promise.resolve({ id: MOUNT, path: ['notes', 'a.md'] }),
    })) as typeof response;
  } else if (c.kind === 'upload') {
    const url = 'http://localhost/api/v1/files?action=upload';
    const { POST } = (await import('@/app/api/v1/files/route')) as {
      POST: (...a: unknown[]) => Promise<unknown>;
    };
    response = (await POST(uploadRequest(url, c))) as typeof response;
  } else {
    const url = `http://localhost/api/v1/chats/${c.chatId}/files?action=${c.action}`;
    const { POST } = (await import('@/app/api/v1/chats/[id]/files/route')) as {
      POST: (...a: unknown[]) => Promise<unknown>;
    };
    response = (await POST(jsonRequest(url, c), {
      params: Promise.resolve({ id: c.chatId! }),
    })) as typeof response;
  }
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = null;
  }
  return { name: c.name, status: response.status, body };
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';
  const out = fs.createWriteStream(outPath);
  for (const c of CASES) {
    out.write(JSON.stringify(await runCase(c)) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
}

describe('P4.62 files body guards oracle', () => {
  it('emits the guard arms', async () => {
    await main();
  });
});
