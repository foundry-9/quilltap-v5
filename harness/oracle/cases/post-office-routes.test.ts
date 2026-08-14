/**
 * @jest-environment node
 *
 * P4.9E2A POST OFFICE / ANNOUNCER route-surface ORACLE: drives v4's REAL chat
 * action handlers (`actions/announcement.ts`, `actions/announcement-preview.ts`,
 * `actions/send-mail.ts`, `actions/mailbox.ts`) over a FRESH copy of the
 * committed post-office fixture per case, and emits each response body (+
 * post-mutation table dumps) so the Rust port (`api::chat_post_office::*`) diffs
 * byte-for-byte.
 *
 * The announcement-preview cases stop at v4's `notFound` arms on purpose — the
 * rewrite itself is model-dependent and is pinned by the separate tier-3
 * `announcer-tier3` oracle.
 *
 * The clock is frozen to NOW_MS for the mutating cases (the announcement's
 * `createdAt`, the delivered letter's `sentAt` → its filename's epoch prefix) so
 * the Rust side's injected constant lands on the same bytes.
 *
 * **Must run under `TZ=UTC`.** `buildReplyPreface` formats the quoted letter's
 * date with `toLocaleString` in the HOST zone, while v5's `format_date_time` is
 * UTC — the house convention for every clock-touching oracle (see
 * `chat-timestamp.ts`, `enclave-step-tier3.test.ts`). The guard below fails loudly
 * rather than emitting a locale-shifted fixture.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-po-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/post-office-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/post-office-web.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PO_MAIN=$V5W/crates/quilltap-web/tests/fixtures/post-office-main.db \
 *   QT_FIXTURE_PO_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/post-office-mount.db \
 *   QT_FIXTURE_PO_META=$V5W/crates/quilltap-web/tests/fixtures/post-office-main.db.meta.json \
 *   QT_ORACLE_OUT=/tmp/oracle-post-office.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- post-office-routes
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
  ariaVault: string;
  beaVault: string;
  clioVault: string;
  dorianVault: string;
  eliseVault: string;
  goneVault: string;
  seededLetterPath: string;
}

// Pinned entity ids (shared verbatim with the builder + the Rust differential).
const CONN = 'c0000001-0000-4000-8000-000000000001';
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BEA = 'a1000000-0000-4000-8000-000000000002';
const CLIO = 'a1000000-0000-4000-8000-000000000003';
const DORIAN = 'a1000000-0000-4000-8000-000000000004';
const ELISE = 'a1000000-0000-4000-8000-000000000005';
const GONE = 'a1000000-0000-4000-8000-000000000006';
const CHAT = 'c1000000-0000-4000-8000-000000000001';
// Also the GHOST participant's characterId: a played participant whose character
// row does not exist, so the arms BEHIND the auth gate are reachable.
const MISSING_ID = '99999999-9999-4999-8999-999999999999';
// P4.D37: CHAT PARTICIPANT ids (not character ids) — the whisper audience's
// currency. P_GONE carries `removedAt`; P_GHOST is live but points at a character
// row that does not exist.
const P_ARIA = 'e1000000-0000-4000-8000-000000000001';
const P_BEA = 'e1000000-0000-4000-8000-000000000002';
const P_GONE = 'e1000000-0000-4000-8000-000000000005';
const P_GHOST = 'e1000000-0000-4000-8000-000000000006';

// Shared frozen clock (matches NOW_MS in the Rust test).
const NOW_MS = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

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
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // jest.setup stubs the character-vault bridge to a single "mock-vault-mount";
  // un-mock it so each character resolves its own real vault (the mailbox read's
  // ensureCharacterVault + the delivery's vault ensure both depend on it).
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

/** Every chat message event (the announcement bubble's persisted row). */
async function readMessages(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  return (await getRepositories().chats.getMessages(chatId)) as unknown;
}

/**
 * Plant a tombstone on this case's FRESH COPY (P4.D65).
 *
 * Two things happen here, and both are deliberate.
 *
 * **The three archive columns are ADDED first.** This family's committed
 * fixture predates the D23 re-dump that introduced them, and it stays that way:
 * only three cases want an archived seat, while widening the committed bytes
 * would move every other case's dump AND the sibling `announcer-tier3` family
 * that shares the file. Adding them per case, on the throwaway copy, is the
 * same shape v5's boot ensure performs on a real instance carried forward from
 * 4.7 — and it leaves the fixture's vintage (and its read-tolerance coverage)
 * exactly as it was.
 *
 * **The flag goes on raw, not through the repository:**
 * `validateCharacterArchivePatch` sanctions exactly one patch on an archived
 * row, and would refuse the one that puts the row into that state to begin
 * with. The stamp is a fixed literal so nothing minted reaches a dump.
 */
async function archiveCharacter(characterId: string): Promise<void> {
  const { rawQuery } = await import('@/lib/database/manager');
  const existing = new Set(
    ((await rawQuery('PRAGMA table_info(characters)')) as Array<{ name: string }>).map(
      (c) => c.name,
    ),
  );
  for (const column of ['archivedAt', 'archiveFileId', 'archivedAvatarFileId']) {
    if (!existing.has(column)) {
      await rawQuery(`ALTER TABLE "characters" ADD COLUMN "${column}" TEXT`);
    }
  }
  await rawQuery('UPDATE characters SET archivedAt = ? WHERE id = ?', [
    '2026-05-02T00:00:00.000Z',
    characterId,
  ]);
}

/** The character row's vault FK (the mailbox GET's adopt-write evidence). */
async function readCharacterVaultFk(characterId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const raw = await getRepositories().characters.findByIdRaw(characterId);
  return raw ? { id: raw.id, characterDocumentMountPointId: raw.characterDocumentMountPointId ?? null } : null;
}

/**
 * A vault's mailbox as stored: every `Mail/…` link with its document content.
 * The letters' bytes ARE the diff (frontmatter + body + reply preface).
 */
async function readVaultMail(vaultId: string): Promise<unknown> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const db = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: (...a: unknown[]) => Array<Record<string, unknown>> };
  } | null;
  if (!db) return [];
  return db
    .prepare(
      'SELECT l.relativePath AS path, d.content AS content ' +
        'FROM doc_mount_file_links l ' +
        'JOIN doc_mount_files f ON f.id = l.fileId ' +
        'JOIN doc_mount_documents d ON d.fileId = f.id ' +
        "WHERE l.mountPointId = ? AND l.relativePath LIKE 'Mail/%' " +
        'ORDER BY l.relativePath',
    )
    .all(vaultId);
}

/**
 * Structural counts for a vault's store, for the mailbox adopt case: the ADOPT
 * branch must add NO documents (it re-links an existing populated vault).
 * `chunks` is dumped, never normalized away — see the §2 tripwire note in the
 * Rust differential.
 */
async function readVaultShape(vaultId: string): Promise<unknown> {
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const db = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => {
      get: (...a: unknown[]) => Record<string, unknown>;
      all: (...a: unknown[]) => Array<Record<string, unknown>>;
    };
  } | null;
  if (!db) return null;
  const links = db
    .prepare('SELECT COUNT(*) AS n FROM doc_mount_file_links WHERE mountPointId = ?')
    .get(vaultId) as { n: number };
  const folders = db
    .prepare('SELECT COUNT(*) AS n FROM doc_mount_folders WHERE mountPointId = ?')
    .get(vaultId) as { n: number };
  const chunks = db
    .prepare('SELECT COUNT(*) AS n FROM doc_mount_chunks WHERE mountPointId = ?')
    .get(vaultId) as { n: number };
  const mp = db
    .prepare('SELECT id, name, storeType, mountType FROM doc_mount_points WHERE id = ?')
    .get(vaultId) as Record<string, unknown> | undefined;
  const mountPointCount = db
    .prepare("SELECT COUNT(*) AS n FROM doc_mount_points WHERE storeType = 'character'")
    .get() as { n: number };
  return {
    mountPoint: mp ?? null,
    linkCount: links.n,
    folderCount: folders.n,
    chunkCount: chunks.n,
    characterMountPointCount: mountPointCount.n,
  };
}

interface CaseSpec {
  name: string;
  freezeClock?: boolean;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'po-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  const RealDate = Date;
  if (c.freezeClock) {
    const iso = new RealDate(NOW_MS).toISOString();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    global.Date = class extends RealDate {
      constructor(...a: unknown[]) {
        if (a.length === 0) super(iso);
        // @ts-expect-error forward variadic args
        else super(...a);
      }
      static now(): number {
        return NOW_MS;
      }
    } as unknown as DateConstructor;
  }

  try {
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  // The reply preface's date is host-zone-formatted in v4 and UTC in v5 — the
  // oracle must be generated under TZ=UTC or the two can never agree.
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`post-office-routes oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'post-office-web.json'), 'utf8'),
  ) as Spec;
  const metaPath = process.env.QT_FIXTURE_PO_META ?? '';
  const meta = JSON.parse(fs.readFileSync(metaPath, 'utf8')) as Meta;

  const fixtures = {
    main: process.env.QT_FIXTURE_PO_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PO_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-po-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1';
  const post = async (chatId: string, action: string, body: unknown) =>
    (await loadRoute(CHAT_ROUTE)).POST(
      mockRequest(`${B}/chats/${chatId}?action=${action}`, body),
      { params: Promise.resolve({ id: chatId }) },
    );
  const get = async (chatId: string, query: string) =>
    (await loadRoute(CHAT_ROUTE)).GET(mockRequest(`${B}/chats/${chatId}?${query}`), {
      params: Promise.resolve({ id: chatId }),
    });

  const cases: CaseSpec[] = [
    // ── ?action=announcement ────────────────────────────────────────────────
    {
      name: 'announcement_staff',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: '  The Lantern is lit; the evening post departs at eight.  ',
            sender: { kind: 'staff', staffId: 'lantern' },
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      name: 'announcement_staff_pascal',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'The table is open.',
            sender: { kind: 'staff', staffId: 'pascal' },
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      name: 'announcement_character',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Dorian sends word from the far bank.',
            sender: { kind: 'character', characterId: DORIAN },
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      name: 'announcement_custom',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'A stranger leaves a card at the door.',
            sender: { kind: 'custom', displayName: 'The Night Porter' },
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      name: 'announcement_character_missing',
      run: async () =>
        respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Nobody sends this.',
            sender: { kind: 'character', characterId: MISSING_ID },
          }),
        ),
    },
    {
      name: 'announcement_chat_missing',
      run: async () =>
        respond(
          await post(MISSING_ID, 'announcement', {
            contentMarkdown: 'Into the void.',
            sender: { kind: 'staff', staffId: 'host' },
          }),
        ),
    },
    {
      name: 'announcement_empty_content',
      run: async () =>
        respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: '',
            sender: { kind: 'staff', staffId: 'host' },
          }),
        ),
    },
    {
      // Whitespace-only passes Zod's min(1) but trims to empty inside the
      // writer → `null` → v4's badRequest, NOT the validation arm.
      name: 'announcement_whitespace_content',
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: '   \n\t  ',
            sender: { kind: 'staff', staffId: 'host' },
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      name: 'announcement_display_name_too_long',
      run: async () =>
        respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Too long a name.',
            sender: { kind: 'custom', displayName: 'x'.repeat(121) },
          }),
        ),
    },
    {
      name: 'announcement_unknown_staff',
      run: async () =>
        respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Who?',
            sender: { kind: 'staff', staffId: 'quartermaster' },
          }),
        ),
    },

    // ── P4.D37 (a163862c): the whisper audience ─────────────────────────────
    // The persisted row IS the evidence — `targetParticipantIds` on the message
    // dump — plus the two 400 arms the resolver exists to produce.
    {
      name: 'announcement_whisper_named',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Only the two of you need hear this.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: [P_ARIA, P_BEA],
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      // An explicit `null` is the same as omitting it: public.
      name: 'announcement_whisper_null_is_public',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Everyone hears this one.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: null,
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      // An empty array normalizes to NULL on the row — "public" gets exactly one
      // representation, rather than a stored `[]` meaning "whispered to nobody".
      name: 'announcement_whisper_empty_array_is_null',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Nobody named.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: [],
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      // Duplicates collapse; first appearance wins the order.
      name: 'announcement_whisper_duplicate_ids',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Said once.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: [P_BEA, P_ARIA, P_BEA],
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      // A participant who has left the scene is not a valid target — whispering
      // to them is the dangling case the resolver guards against.
      name: 'announcement_whisper_removed_participant',
      run: async () =>
        respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'For someone who left.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: [P_GONE],
          }),
        ),
    },
    {
      // A live target plus an id belonging to no participant of this chat: the
      // whole post is refused, and only the unknown id is named.
      name: 'announcement_whisper_foreign_id',
      run: async () =>
        respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'For a stranger.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: [P_ARIA, MISSING_ID],
          }),
        ),
    },
    {
      // A participant whose CHARACTER ROW is missing is still a live participant
      // — the target resolves (it is only the NAME that falls back to 'Someone').
      name: 'announcement_whisper_ghost_participant',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'For the unnameable.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: [P_GHOST],
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      // A custom display name whispers just as happily — the sender union and the
      // audience are independent.
      name: 'announcement_whisper_custom_sender',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Psst.',
            sender: { kind: 'custom', displayName: 'A Distant Bell' },
            targetParticipantIds: [P_ARIA],
          }),
        );
        return { status, body, tables: { messages: await readMessages(CHAT) } };
      },
    },
    {
      // `z.array(z.uuid())` — a non-uuid never reaches the resolver.
      name: 'announcement_whisper_invalid_uuid',
      run: async () =>
        respond(
          await post(CHAT, 'announcement', {
            contentMarkdown: 'Malformed audience.',
            sender: { kind: 'staff', staffId: 'host' },
            targetParticipantIds: ['not-a-uuid'],
          }),
        ),
    },

    // ── ?action=announcement-preview (the pre-LLM arms) ─────────────────────
    {
      name: 'preview_character_missing',
      run: async () =>
        respond(
          await post(CHAT, 'announcement-preview', {
            seedMarkdown: 'Tell them the lamps are out.',
            characterId: MISSING_ID,
            connectionProfileId: CONN,
          }),
        ),
    },
    {
      name: 'preview_profile_missing',
      run: async () =>
        respond(
          await post(CHAT, 'announcement-preview', {
            seedMarkdown: 'Tell them the lamps are out.',
            characterId: DORIAN,
            connectionProfileId: MISSING_ID,
          }),
        ),
    },
    {
      name: 'preview_empty_seed',
      run: async () =>
        respond(
          await post(CHAT, 'announcement-preview', {
            seedMarkdown: '',
            characterId: DORIAN,
            connectionProfileId: CONN,
          }),
        ),
    },
    {
      name: 'preview_chat_missing',
      run: async () =>
        respond(
          await post(MISSING_ID, 'announcement-preview', {
            seedMarkdown: 'Tell them.',
            characterId: DORIAN,
            connectionProfileId: CONN,
          }),
        ),
    },

    // ── ?action=send-mail ──────────────────────────────────────────────────
    {
      name: 'send_mail_ok',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: BEA,
            bodyMarkdown: 'Bea — I asked Dorian. He says the oil was short.',
          }),
        );
        return {
          status,
          body,
          tables: {
            recipientMail: await readVaultMail(meta.beaVault),
            senderMail: await readVaultMail(meta.ariaVault),
            recipientShape: await readVaultShape(meta.beaVault),
          },
        };
      },
    },
    {
      name: 'send_mail_reply',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: BEA,
            bodyMarkdown: 'Bea — answering yours below.',
            inReplyToPath: meta.seededLetterPath,
          }),
        );
        return {
          status,
          body,
          tables: {
            recipientMail: await readVaultMail(meta.beaVault),
            recipientShape: await readVaultShape(meta.beaVault),
          },
        };
      },
    },
    {
      name: 'send_mail_reply_not_found',
      freezeClock: true,
      run: async () => {
        const { status, body } = await respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: BEA,
            bodyMarkdown: 'Answering a letter I never had.',
            inReplyToPath: 'Mail/1700000000000-from-nobody.md',
          }),
        );
        return { status, body, tables: { recipientMail: await readVaultMail(meta.beaVault) } };
      },
    },
    {
      // Not a player-character in this chat (LLM-controlled) → 400, not 403.
      name: 'send_mail_from_llm_character',
      run: async () =>
        respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: CLIO,
            toCharacterId: BEA,
            bodyMarkdown: 'Signed by an LLM character.',
          }),
        ),
    },
    {
      // A removed participant fails the `!p.removedAt` predicate.
      name: 'send_mail_from_removed_participant',
      run: async () =>
        respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: GONE,
            toCharacterId: BEA,
            bodyMarkdown: 'Signed by someone who left.',
          }),
        ),
    },
    {
      name: 'send_mail_from_stranger',
      run: async () =>
        respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: DORIAN,
            toCharacterId: BEA,
            bodyMarkdown: 'Signed by a stranger to this scene.',
          }),
        ),
    },
    {
      name: 'send_mail_recipient_missing',
      run: async () =>
        respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: MISSING_ID,
            bodyMarkdown: 'Into the void.',
          }),
        ),
    },
    {
      // The GHOST participant: passes the played-participant gate, then the raw
      // character read misses → notFound('Sender character').
      name: 'send_mail_sender_row_missing',
      run: async () =>
        respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: MISSING_ID,
            toCharacterId: BEA,
            bodyMarkdown: 'From a participant with no character row.',
          }),
        ),
    },
    {
      name: 'send_mail_empty_body',
      run: async () =>
        respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: BEA,
            bodyMarkdown: '',
          }),
        ),
    },
    {
      name: 'send_mail_chat_missing',
      run: async () =>
        respond(
          await post(MISSING_ID, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: BEA,
            bodyMarkdown: 'Into the void.',
          }),
        ),
    },
    // ── P4.D65: the archived-character 400s (the banked P4.D63 unit-4 arms) ──
    // The tombstone is planted per case on the FRESH COPY rather than baked
    // into the committed fixture: nothing else in this family wants an archived
    // seat, and a baked one would drag every other case's dumps with it. Both
    // send-mail arms read the sender and the recipient by RAW ID, so no name
    // resolution stands between the flag and the refusal.
    {
      name: 'send_mail_from_archived_sender',
      run: async () => {
        await archiveCharacter(ARIA);
        return respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: BEA,
            bodyMarkdown: 'One last letter.',
          }),
        );
      },
    },
    {
      name: 'send_mail_to_archived_recipient',
      run: async () => {
        await archiveCharacter(BEA);
        return respond(
          await post(CHAT, 'send-mail', {
            fromCharacterId: ARIA,
            toCharacterId: BEA,
            bodyMarkdown: 'Are you still there?',
          }),
        );
      },
    },

    // ── GET ?action=mailbox ────────────────────────────────────────────────
    {
      name: 'mailbox_aria',
      run: async () => {
        const { status, body } = await respond(await get(CHAT, `action=mailbox&characterId=${ARIA}`));
        return {
          status,
          body,
          tables: {
            vault: await readCharacterVaultFk(ARIA),
            shape: await readVaultShape(meta.ariaVault),
          },
        };
      },
    },
    {
      // Bea's mailbox is empty (she SENT the seeded letter; no "Sent" copy is
      // kept) and her vault has no Mail/ folder at all — v4's listMailbox
      // tolerates the absent folder and returns [].
      name: 'mailbox_bea_empty',
      run: async () => respond(await get(CHAT, `action=mailbox&characterId=${BEA}`)),
    },
    {
      // The ADOPT branch: Elise's vault FK is null but her populated same-name
      // vault still exists → ensureCharacterVault re-links it. A WRITE on a GET.
      name: 'mailbox_elise_adopts_vault',
      run: async () => {
        const { status, body } = await respond(
          await get(CHAT, `action=mailbox&characterId=${ELISE}`),
        );
        return {
          status,
          body,
          tables: {
            vault: await readCharacterVaultFk(ELISE),
            shape: await readVaultShape(meta.eliseVault),
          },
        };
      },
    },
    {
      // The GHOST participant: authorized, then the raw character read misses.
      name: 'mailbox_character_row_missing',
      run: async () => respond(await get(CHAT, `action=mailbox&characterId=${MISSING_ID}`)),
    },
    {
      name: 'mailbox_forbidden_llm_character',
      run: async () => respond(await get(CHAT, `action=mailbox&characterId=${CLIO}`)),
    },
    {
      name: 'mailbox_forbidden_stranger',
      run: async () => respond(await get(CHAT, `action=mailbox&characterId=${DORIAN}`)),
    },
    {
      name: 'mailbox_missing_character_id',
      run: async () => respond(await get(CHAT, 'action=mailbox')),
    },
    {
      name: 'mailbox_chat_missing',
      run: async () => respond(await get(MISSING_ID, `action=mailbox&characterId=${ARIA}`)),
    },
    {
      // Authorized (Aria is a seat the operator plays), then refused on the
      // FLAG — before `ensureCharacterVault`, so an archived character's
      // mailbox read never touches her vault.
      name: 'mailbox_archived_character',
      run: async () => {
        await archiveCharacter(ARIA);
        return respond(await get(CHAT, `action=mailbox&characterId=${ARIA}`));
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`post-office-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('post-office-routes oracle', async () => {
  await main();
});
