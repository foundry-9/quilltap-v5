/**
 * @jest-environment node
 *
 * P4.9K1 character RENAME / REFRESH-ARCHIVE ORACLE (tier 2): drives v4's REAL
 * `characters/[id]/handlers/post.ts` `rename` + `refresh-archive` actions —
 * `renameSchema.parse`, the neither-pair 400, `runCharacterRename` (the literal
 * regex matcher, the write routing through `repos.characters.update`, the
 * memory / chat / message updates, the render enqueue) and the refresh-archive
 * enqueue loop — over a FRESH copy of the committed characters fixture per
 * case, and emits the response (status + body) PLUS a post-state dump: the
 * overlaid character, every memory of the character, every chat, every chat
 * message and every background job. The Rust port (`api::generators_detail::
 * character_rename` / `character_refresh_archive`) is diffed against all of it
 * (identical baked ids → no remap; minted timestamps + job ids normalized on
 * the Rust side).
 *
 * Per-case `seeds` (the corpus JSON) are applied through v4's REAL repositories
 * — `repos.chats.addMessages` for a Staff message, `repos.memories.update` for
 * a memory body carrying a Canonicalize / GetSubstitution probe — and the Rust
 * side applies the same seeds through the ported twins, so both sides scan the
 * same rows.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-character-rename-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/character-rename.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/fixtures/character-rename-tier2.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-character-rename.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- character-rename
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

interface MessagesSeed {
  kind: 'messages';
  chatId: string;
  events: unknown[];
}
interface MemoryContentSeed {
  kind: 'memoryContent';
  memoryId: string;
  content: string;
  keywords?: string[];
}
type Seed = MessagesSeed | MemoryContentSeed;

interface CaseSpec {
  name: string;
  characterId: string;
  action: string;
  body: unknown;
  seeds?: Seed[];
}

interface Corpus {
  seedTimestamp: string;
  cases: CaseSpec[];
}

function mockRequest(url: string, body: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

/** The post-state census — the same SQL the Rust side runs. */
const MEMORIES_SQL =
  'SELECT id, characterId, aboutCharacterId, content, summary, keywords, updatedAt FROM memories WHERE characterId = ? ORDER BY id';
const CHATS_SQL = 'SELECT id, title, messageCount, lastMessageAt, updatedAt FROM chats ORDER BY id';
const MESSAGES_SQL =
  'SELECT id, chatId, type, role, content, systemSender, participantId, createdAt FROM chat_messages ORDER BY chatId, createdAt, id';
const JOBS_SQL =
  'SELECT id, userId, type, status, payload, priority, attempts, maxAttempts FROM background_jobs ORDER BY rowid';

async function runCase(
  spec: Spec,
  corpus: Corpus,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

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
  // `enqueueJob()` resolves the job-child entry relative to cwd — keep the
  // in-process runner out of the dumps (the help-sync-ensure recipe). The
  // enqueue itself stays REAL: the `background_jobs` rows are state under test.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });

  const work = mkdtempSync(join(scratch, 'rename-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  try {
    // Seeds, through the REAL repositories (the Rust side uses the twins).
    for (const seed of c.seeds ?? []) {
      if (seed.kind === 'messages') {
        await repos.chats.addMessages(seed.chatId, seed.events as never);
      } else if (seed.kind === 'memoryContent') {
        const patch: Record<string, unknown> = {
          content: seed.content,
          updatedAt: corpus.seedTimestamp,
        };
        if (seed.keywords) patch.keywords = seed.keywords;
        await repos.memories.update(seed.memoryId, patch as never);
      }
    }

    const url = `http://localhost/api/v1/characters/${c.characterId}?action=${c.action}`;
    const { POST } = (await import('@/app/api/v1/characters/[id]/route')) as {
      POST: (...a: unknown[]) => Promise<unknown>;
    };
    const response = (await POST(mockRequest(url, c.body), {
      params: Promise.resolve({ id: c.characterId }),
    })) as { status: number; json: () => Promise<unknown> };
    const status = response.status;
    const body = await response.json();

    // The overlaid character (v4 `findById` — THROWS on a broken vault; that
    // arm is recorded as `{ok:false, error}` rather than lost).
    let character: unknown;
    try {
      character = { ok: true, value: await repos.characters.findById(c.characterId) };
    } catch (err) {
      character = { ok: false, error: err instanceof Error ? err.message : String(err) };
    }
    const memories = (await rawQuery<unknown[]>(MEMORIES_SQL, [c.characterId])) ?? [];
    const chats = (await rawQuery<unknown[]>(CHATS_SQL)) ?? [];
    const messages = (await rawQuery<unknown[]>(MESSAGES_SQL)) ?? [];
    const jobs = (await rawQuery<unknown[]>(JOBS_SQL)) ?? [];
    return { name: c.name, status, body, character, memories, chats, messages, jobs };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'characters.json'), 'utf8'),
  ) as Spec;
  const corpus = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'character-rename-tier2.json'), 'utf8'),
  ) as Corpus;

  const fixtures = {
    main: process.env.QT_FIXTURE_CHARACTERS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CHARACTERS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-character-rename-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of corpus.cases) {
    const payload = await runCase(spec, corpus, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`character-rename oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('character-rename oracle', async () => {
  await main();
});
