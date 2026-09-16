/**
 * @jest-environment node
 *
 * P4.D184 collapse ORACLE — v4's REAL `collapse-duplicate-avatar-rolls-v1`
 * migration (`migrations/scripts/collapse-duplicate-avatar-rolls-v1.ts`, v4
 * `7fbf8a55b`) plus its REAL ledger write (`migrations/state.ts`
 * `recordCompletedMigration`), driven over the shared spec
 * `harness/oracle/fixtures/avatar-rolls-collapse-heal.json`.
 *
 * The scenarios are v4's OWN test cases from
 * `__tests__/unit/migrations/collapse-duplicate-avatar-rolls.test.ts`, rebuilt
 * as data so both implementations walk the same seeds — the original thirteen
 * plus the album/census cases `23abc1ba1` added for bug 145 (P4.D192). The migration reads TWO
 * databases: the MAIN partition through `migrations/lib/database-utils` (mocked
 * to a shared in-memory DB) and the MOUNT-INDEX partition, which it opens itself
 * — so this oracle borrows the P4.D152 realign oracle's two mechanics verbatim:
 * un-mock `better-sqlite3` by absolute path, and point
 * `getMountIndexDatabasePath()` at a real temp file with no pepper set, so both
 * sides read a plain file (the cipher is boot infrastructure, not this
 * comparand).
 *
 * Each row dumps:
 *   - `completedBefore` / `shouldRun` / the `MigrationResult` (id, success,
 *     itemsAffected, message — v4's summary sentence is a comparand);
 *   - the WHOLE post-pass state of every table the pass can touch: `files`
 *     (id, generationKey, storageKey, createdAt), `chats.characterAvatars`,
 *     `characters.avatarOverrides`, `chat_messages`
 *     (attachments, content, opaqueContent), and the five mount tables — so a
 *     survivor kept, a victim deleted, a reference repointed, a uuid swapped
 *     inline and a blob's chunks going with it are all comparands;
 *   - `infos` / `warns`: every `logger.info`/`logger.warn` the pass emitted, in
 *     order, with v4's own context fields — the protected-roll and
 *     blob-failure arms are log-only on the state;
 *   - `shouldRunAfter` and a SECOND `run()` (idempotence), plus the files table
 *     after it;
 *   - the `migrations_state` row v4's runner writes (its `completedAt` and
 *     `quilltapVersion` are nondeterministic and are NOT compared; the row's
 *     PRESENCE and its `itemsAffected`/`message` are).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths. The migration arrived at `7fbf8a55b` and was REWRITTEN at
 * `23abc1ba1` (bug 145), both past the `ffb6b3119` baseline, so pin the checkout
 * at or after `23abc1ba1` until the baseline moves):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=<this worktree root>
 *   TMPO=/tmp/qt-avatar-rolls-collapse-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/avatar-rolls-collapse-heal.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/avatar-rolls-collapse-heal.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-avatar-rolls-collapse.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "avatar-rolls-collapse-heal\.test\.ts$"
 */

import { describe, it, jest } from '@jest/globals';
import * as fs from 'fs';
import * as os from 'os';
import path from 'path';

function loadDriver() {
  try {
    return require(path.join(
      process.cwd(),
      'packages',
      'quilltap',
      'node_modules',
      'better-sqlite3-multiple-ciphers'
    ));
  } catch {
    return require(path.join(process.cwd(), 'node_modules', 'better-sqlite3'));
  }
}
const Database = loadDriver();
type DatabaseInstance = ReturnType<typeof Database>;

/** v4's `VAULT_MOUNT` — the character's own vault, where the album lives. */
const VAULT_MOUNT = 'vault-1';

let testDb: DatabaseInstance = null as unknown as DatabaseInstance;
let mountDbPath = '';

/** The migration constructs its own mount handle — give it the REAL driver. */
jest.mock('better-sqlite3', () => {
  const real = loadDriver();
  return { __esModule: true, default: real, Database: real };
});

interface LogCall {
  message: string;
  context: Record<string, unknown>;
}
const infoCalls: LogCall[] = [];
const warnCalls: LogCall[] = [];
const errorCalls: LogCall[] = [];

jest.mock('@/migrations/lib/logger', () => ({
  logger: {
    info: (message: string, context: unknown) => {
      infoCalls.push({ message, context: (context ?? {}) as Record<string, unknown> });
    },
    debug: jest.fn(),
    warn: (message: string, context: unknown) => {
      warnCalls.push({ message, context: (context ?? {}) as Record<string, unknown> });
    },
    error: (message: string, context: unknown) => {
      errorCalls.push({ message, context: (context ?? {}) as Record<string, unknown> });
    },
    child: jest.fn().mockReturnValue({
      info: jest.fn(),
      debug: jest.fn(),
      warn: jest.fn(),
      error: jest.fn(),
    }),
  },
}));

jest.mock('@/migrations/lib/progress', () => ({ reportProgress: jest.fn() }));

jest.mock('@/migrations/lib/database-utils', () => ({
  isSQLiteBackend: () => true,
  sqliteTableExists: (name: string) =>
    (
      testDb
        .prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`)
        .get(name) as unknown
    ) !== undefined,
  sqliteColumnExists: (table: string, column: string) =>
    (testDb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).some(
      (c) => c.name === column
    ),
  getSQLiteDatabase: () => testDb,
  getSQLiteTableColumns: (name: string) =>
    testDb.prepare(`PRAGMA table_info(${name})`).all() as Array<{ name: string }>,
  executeSQLite: (sql: string) => {
    testDb.exec(sql);
  },
  querySQLite: (sql: string) => testDb.prepare(sql).all(),
  openMountIndexDbIfPresent: () => {
    if (!mountDbPath || !fs.existsSync(mountDbPath)) return null;
    const Driver = loadDriver();
    const db = new Driver(mountDbPath);
    db.pragma('busy_timeout = 5000');
    return db;
  },
}));

jest.mock('@/lib/paths', () => {
  const actual = jest.requireActual('@/lib/paths') as Record<string, unknown>;
  return { __esModule: true, ...actual, getMountIndexDatabasePath: () => mountDbPath };
});

interface RollSpec {
  id: string;
  prompt: string | null;
  createdAt: string;
  model?: string | null;
  /** `originalFilename` override — the "not an avatar" case uses it. */
  filename?: string;
  /** `category` override — the "not an image" case uses it. */
  category?: string;
  /** Skip the mount-side seeding entirely (a roll with no blob). */
  noBlob?: boolean;
  /**
   * The roll's own link path. v4's `seedRoll` writes
   * `character-avatars/avatar_Friday_<id>.webp` (`23abc1ba1`); an override is
   * how a scenario poses a roll living somewhere else.
   */
  relativePath?: string;
}

/**
 * A second link over a roll's content row — what "the operator kept this plate
 * in a character's album" looks like in the mount index (v4 `keepInAlbum`,
 * `23abc1ba1`). `mountPointId` defaults to the character's own vault, a
 * DIFFERENT mount from the roll's.
 */
interface AlbumLinkSpec {
  rollId: string;
  mountPointId?: string;
}

/** A plain `files` row outside the pass's avatar predicate (v4's `stowaway`). */
interface ExtraFileSpec {
  id: string;
  originalFilename: string;
  category?: string;
  prompt: string | null;
  model?: string | null;
  createdAt: string;
  storageKey?: string | null;
  /**
   * `survivor-of:<rollId>` derives the key from that roll's (model, prompt)
   * through v4's REAL `deriveLegacyAvatarCacheKey`, so neither side ever
   * carries a hand-copied hash. Any other string is written literally.
   */
  generationKey?: string | null;
}
interface Scenario {
  name: string;
  rolls: RollSpec[];
  chats?: Array<{ id: string; characterAvatars: unknown }>;
  characters?: Array<{ id: string; avatarOverrides: unknown; defaultImageId?: string | null }>;
  messages?: Array<{
    id: string;
    attachments: unknown;
    content?: string | null;
    opaqueContent?: string | null;
  }>;
  /** Album copies of a roll's bytes, seeded after the rolls unless… */
  albumLinks?: AlbumLinkSpec[];
  /**
   * …this is set, which seeds them BEFORE the rolls. rowid order is what
   * `Array.prototype.find` walks in `dropVictimRollLink`'s links SELECT (no
   * ORDER BY), so an album link seeded first is what makes the
   * `!isPhotosRelativePath` conjunct load-bearing rather than incidentally
   * correct.
   */
  albumLinksFirst?: boolean;
  /** Link ids deleted AFTER seeding and BEFORE the run (v4's case (c)). */
  deleteLinks?: string[];
  /** `files` rows outside the avatar predicate (v4's `stowaway`). */
  extraFiles?: ExtraFileSpec[];
  /** Plant the ledger row first — the cross-app "v4 already ran it" shape. */
  preCompleted?: boolean;
  /** Run the pass twice (idempotence). */
  runTwice?: boolean;
}
interface Spec {
  mountPointId: string;
  nowIso: string;
  scenarios: Scenario[];
}

function makeMainDb(): DatabaseInstance {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE "files" (
      "id" TEXT PRIMARY KEY,
      "originalFilename" TEXT NOT NULL,
      "category" TEXT NOT NULL,
      "generationPrompt" TEXT,
      "generationModel" TEXT,
      "generationKey" TEXT,
      "storageKey" TEXT,
      "createdAt" TEXT NOT NULL
    );
    CREATE TABLE "chats" ("id" TEXT PRIMARY KEY, "characterAvatars" TEXT);
    CREATE TABLE "characters" (
      "id" TEXT PRIMARY KEY,
      "avatarOverrides" TEXT,
      "defaultImageId" TEXT
    );
    CREATE TABLE "chat_messages" (
      "id" TEXT PRIMARY KEY,
      "attachments" TEXT,
      "content" TEXT,
      "opaqueContent" TEXT
    );
  `);
  return db;
}

function makeMountDb(file: string): DatabaseInstance {
  const db = new Database(file);
  db.exec(`
    CREATE TABLE "doc_mount_files" ("id" TEXT PRIMARY KEY);
    CREATE TABLE "doc_mount_blobs" ("id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL);
    CREATE TABLE "doc_mount_documents" ("id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL);
    CREATE TABLE "doc_mount_file_links" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "mountPointId" TEXT NOT NULL,
      "relativePath" TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE "doc_mount_chunks" ("id" TEXT PRIMARY KEY, "linkId" TEXT NOT NULL);
  `);
  return db;
}

/** v4's own `seedRoll`: a `files` row plus its vault file/blob/link/chunk. */
function seedRoll(
  main: DatabaseInstance,
  mount: DatabaseInstance,
  spec: Spec,
  roll: RollSpec
): void {
  const blobId = `blob-${roll.id}`;
  main
    .prepare(
      `INSERT INTO "files" (id, originalFilename, category, generationPrompt, generationModel, generationKey, storageKey, createdAt)
       VALUES (?, ?, ?, ?, ?, NULL, ?, ?)`
    )
    .run(
      roll.id,
      roll.filename ?? `avatar_Friday_${roll.id}.webp`,
      roll.category ?? 'IMAGE',
      roll.prompt,
      roll.model === undefined ? 'flux-dev' : roll.model,
      roll.noBlob ? null : `mount-blob:${spec.mountPointId}:${blobId}`,
      roll.createdAt
    );
  if (roll.noBlob) return;
  const contentId = `content-${roll.id}`;
  mount.prepare('INSERT INTO "doc_mount_files" (id) VALUES (?)').run(contentId);
  mount.prepare('INSERT INTO "doc_mount_blobs" (id, fileId) VALUES (?, ?)').run(blobId, contentId);
  mount
    .prepare('INSERT INTO "doc_mount_documents" (id, fileId) VALUES (?, ?)')
    .run(`doc-${roll.id}`, contentId);
  mount
    .prepare(
      'INSERT INTO "doc_mount_file_links" (id, fileId, mountPointId, relativePath) VALUES (?, ?, ?, ?)'
    )
    .run(
      `link-${roll.id}`,
      contentId,
      spec.mountPointId,
      roll.relativePath ?? `character-avatars/avatar_Friday_${roll.id}.webp`
    );
  mount
    .prepare('INSERT INTO "doc_mount_chunks" (id, linkId) VALUES (?, ?)')
    .run(`chunk-${roll.id}`, `link-${roll.id}`);
}

/**
 * v4's `keepInAlbum` (`23abc1ba1`): a SECOND link, over the same content row,
 * in a `photos/` folder. Two links over one set of bytes is what keeping a
 * plate looks like — and only the roll's own is the collapse's to take.
 */
function keepInAlbum(mount: DatabaseInstance, link: AlbumLinkSpec): void {
  mount
    .prepare(
      'INSERT INTO "doc_mount_file_links" (id, fileId, mountPointId, relativePath) VALUES (?, ?, ?, ?)'
    )
    .run(
      `album-${link.rollId}`,
      `content-${link.rollId}`,
      link.mountPointId ?? VAULT_MOUNT,
      `photos/kept-${link.rollId}.webp`
    );
}

/** A `files` row the pass never groups — seeded with a key it did not mint. */
function seedExtraFile(
  main: DatabaseInstance,
  scenario: Scenario,
  extra: ExtraFileSpec,
  deriveKey: (model: string | null, prompt: string) => string
): void {
  let key: string | null = extra.generationKey ?? null;
  if (key && key.startsWith('survivor-of:')) {
    const rollId = key.slice('survivor-of:'.length);
    const roll = scenario.rolls.find((r) => r.id === rollId);
    if (!roll) throw new Error(`survivor-of names no roll: ${rollId}`);
    key = deriveKey(roll.model === undefined ? 'flux-dev' : roll.model, roll.prompt ?? '');
  }
  main
    .prepare(
      `INSERT INTO "files" (id, originalFilename, category, generationPrompt, generationModel, generationKey, storageKey, createdAt)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`
    )
    .run(
      extra.id,
      extra.originalFilename,
      extra.category ?? 'IMAGE',
      extra.prompt,
      extra.model === undefined ? 'flux-dev' : extra.model,
      key,
      extra.storageKey ?? null,
      extra.createdAt
    );
}

function plantLedgerRow(db: DatabaseInstance, spec: Spec): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS "migrations_state" (
      "id" TEXT PRIMARY KEY,
      "completedAt" TEXT NOT NULL,
      "quilltapVersion" TEXT NOT NULL,
      "itemsAffected" INTEGER NOT NULL DEFAULT 0,
      "message" TEXT
    );
    CREATE TABLE IF NOT EXISTS "migrations_metadata" (
      "key" TEXT PRIMARY KEY,
      "value" TEXT NOT NULL
    );
  `);
  db.prepare(
    'INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?, ?, ?, ?, ?)'
  ).run('collapse-duplicate-avatar-rolls-v1', spec.nowIso, '4.10.0', 0, 'planted by the other app');
}

const q = (db: DatabaseInstance, sql: string) =>
  db.prepare(sql).all() as Array<Record<string, unknown>>;

function dumpAll(main: DatabaseInstance, mount: DatabaseInstance) {
  return {
    files: q(
      main,
      'SELECT id, generationKey, storageKey, createdAt FROM files ORDER BY id'
    ),
    chats: q(main, 'SELECT id, characterAvatars FROM chats ORDER BY id'),
    characters: q(
      main,
      'SELECT id, avatarOverrides, defaultImageId FROM characters ORDER BY id'
    ),
    chat_messages: q(
      main,
      'SELECT id, attachments, content, opaqueContent FROM chat_messages ORDER BY id'
    ),
    doc_mount_files: q(mount, 'SELECT id FROM doc_mount_files ORDER BY id'),
    doc_mount_blobs: q(mount, 'SELECT id, fileId FROM doc_mount_blobs ORDER BY id'),
    doc_mount_documents: q(mount, 'SELECT id, fileId FROM doc_mount_documents ORDER BY id'),
    doc_mount_file_links: q(
      mount,
      'SELECT id, fileId, mountPointId, relativePath FROM doc_mount_file_links ORDER BY id'
    ),
    doc_mount_chunks: q(mount, 'SELECT id, linkId FROM doc_mount_chunks ORDER BY id'),
  };
}

/** The log shape both sides compare: the sentence plus v4's own context keys. */
function shapeLogs(calls: LogCall[]) {
  return calls.map((c) => ({
    message: c.message,
    context: c.context.context ?? null,
    fileId: c.context.fileId ?? null,
    blobId: c.context.blobId ?? null,
    avatarRows: c.context.avatarRows ?? null,
    configurations: c.context.configurations ?? null,
    victims: c.context.victims ?? null,
    rowsKeyed: c.context.rowsKeyed ?? null,
    victimsDeleted: c.context.victimsDeleted ?? null,
    blobsDeleted: c.context.blobsDeleted ?? null,
    chatsChanged: c.context.chatsChanged ?? null,
    charactersChanged: c.context.charactersChanged ?? null,
    messagesChanged: c.context.messagesChanged ?? null,
    protectedKept: c.context.protectedKept ?? null,
    albumCopiesKept: c.context.albumCopiesKept ?? null,
    unexplainedCount: c.context.unexplainedCount ?? null,
    fileIds: c.context.fileIds ?? null,
  }));
}

describe('avatar-rolls-collapse-heal oracle', () => {
  it('runs the real migration + ledger over each scenario and dumps the result', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('set QT_ORACLE_OUT');

    const spec = JSON.parse(
      fs.readFileSync(
        path.join(__dirname, '..', 'fixtures', 'avatar-rolls-collapse-heal.json'),
        'utf8'
      )
    ) as Spec;

    // Both sides read a plain mount file (see the header).
    const savedPepper = process.env.ENCRYPTION_MASTER_PEPPER;
    delete process.env.ENCRYPTION_MASTER_PEPPER;

    const { collapseDuplicateAvatarRollsMigration } = await import(
      '@/migrations/scripts/collapse-duplicate-avatar-rolls-v1'
    );
    const { loadMigrationState, isMigrationCompleted, recordCompletedMigration } = await import(
      '@/migrations/state'
    );
    // The `extraFiles` keys are DERIVED through v4's own helper, never copied.
    const { deriveLegacyAvatarCacheKey } = await import('@/lib/wardrobe/avatar-cache');
    const deriveKey = (model: string | null, prompt: string) =>
      deriveLegacyAvatarCacheKey({ modelName: model, prompt });

    const lines: string[] = [];
    try {
      for (const scenario of spec.scenarios) {
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'qt-collapse-'));
        mountDbPath = path.join(dir, 'quilltap-mount-index.db');
        const mount = makeMountDb(mountDbPath);
        testDb = makeMainDb();
        // rowid order is a comparand: `dropVictimRollLink`'s links SELECT has no
        // ORDER BY, so whichever link was inserted first is the one
        // `Array.prototype.find` reaches first.
        if (scenario.albumLinksFirst) {
          for (const link of scenario.albumLinks ?? []) keepInAlbum(mount, link);
        }
        for (const roll of scenario.rolls) seedRoll(testDb, mount, spec, roll);
        if (!scenario.albumLinksFirst) {
          for (const link of scenario.albumLinks ?? []) keepInAlbum(mount, link);
        }
        for (const linkId of scenario.deleteLinks ?? []) {
          mount.prepare('DELETE FROM "doc_mount_file_links" WHERE id = ?').run(linkId);
        }
        for (const extra of scenario.extraFiles ?? []) {
          seedExtraFile(testDb, scenario, extra, deriveKey);
        }
        for (const c of scenario.chats ?? []) {
          testDb
            .prepare('INSERT INTO chats (id, characterAvatars) VALUES (?, ?)')
            .run(c.id, c.characterAvatars === null ? null : JSON.stringify(c.characterAvatars));
        }
        for (const c of scenario.characters ?? []) {
          testDb
            .prepare(
              'INSERT INTO characters (id, avatarOverrides, defaultImageId) VALUES (?, ?, ?)'
            )
            .run(
              c.id,
              c.avatarOverrides === null ? null : JSON.stringify(c.avatarOverrides),
              c.defaultImageId ?? null
            );
        }
        for (const m of scenario.messages ?? []) {
          testDb
            .prepare(
              'INSERT INTO chat_messages (id, attachments, content, opaqueContent) VALUES (?, ?, ?, ?)'
            )
            .run(
              m.id,
              m.attachments === null ? null : JSON.stringify(m.attachments),
              m.content ?? null,
              m.opaqueContent ?? null
            );
        }
        if (scenario.preCompleted) plantLedgerRow(testDb, spec);
        infoCalls.length = 0;
        warnCalls.length = 0;
        errorCalls.length = 0;

        const record: Record<string, unknown> = { name: scenario.name };

        let state = await loadMigrationState();
        record.completedBefore = isMigrationCompleted(
          state,
          'collapse-duplicate-avatar-rolls-v1'
        );
        record.shouldRun = await collapseDuplicateAvatarRollsMigration.shouldRun();

        // v4's runner: the completed check comes FIRST, then shouldRun.
        if (!record.completedBefore && record.shouldRun) {
          const result = await collapseDuplicateAvatarRollsMigration.run();
          record.result = {
            id: result.id,
            success: result.success,
            itemsAffected: result.itemsAffected,
            message: result.message,
            error: result.error ?? null,
          };
          if (result.success) state = await recordCompletedMigration(state, result);
        } else {
          record.result = null;
        }

        record.infos = shapeLogs(infoCalls);
        record.warns = shapeLogs(warnCalls);
        record.errors = shapeLogs(errorCalls);
        record.dumps = dumpAll(testDb, mount);

        record.shouldRunAfter = await collapseDuplicateAvatarRollsMigration.shouldRun();
        if (scenario.runTwice) {
          // Idempotence: v4's runner would SKIP on the ledger row, so the second
          // pass here is the "ledger lost, pass re-run" shape — it must find
          // nothing left to do.
          const second = await collapseDuplicateAvatarRollsMigration.run();
          record.secondResult = {
            id: second.id,
            success: second.success,
            itemsAffected: second.itemsAffected,
            message: second.message,
          };
          record.dumpsAfterSecond = dumpAll(testDb, mount);
        }

        const ledger = (
          testDb
            .prepare(
              `SELECT 1 FROM sqlite_master WHERE type='table' AND name='migrations_state'`
            )
            .get() as unknown
        )
          ? (testDb
              .prepare(
                'SELECT id, itemsAffected, message FROM migrations_state WHERE id = ?'
              )
              .get('collapse-duplicate-avatar-rolls-v1') as Record<string, unknown> | undefined)
          : undefined;
        // `completedAt`/`quilltapVersion` are nondeterministic and excluded; the
        // row's PRESENCE and what it claims are the comparands.
        record.ledgerRow = ledger ?? null;

        lines.push(JSON.stringify(record));
        testDb.close();
        mount.close();
        fs.rmSync(dir, { recursive: true, force: true });
      }
    } finally {
      if (savedPepper !== undefined) process.env.ENCRYPTION_MASTER_PEPPER = savedPepper;
    }

    fs.writeFileSync(out, lines.join('\n') + '\n');
    process.stderr.write(`avatar-rolls-collapse oracle wrote ${out} (${lines.length} lines)\n`);
  });
});
