import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync, openSync } from 'node:fs';
import { resolve } from 'node:path';

import { makeDbKeyFile } from './support/dbkey';
import { seedCourierImagesFixture } from './support/seed-courier-fixture';
import { seedPascalToolsFixture } from './support/seed-pascal-tools-fixture';
import {
  ARTIFACTS_DIR,
  BASE_URL,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  FIXTURES_DIR,
  INSTANCE_DATA_DIR,
  INSTANCE_DIR,
  MOCK_LLM_PORT,
  PID_FILE,
  PORT,
  SERVER_LOG,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';

/**
 * Playwright global setup: build a passphrase-LOCKED copy of the committed
 * chat-send fixture and launch the REAL axum server against it + the built SPA.
 * The fixture is COPIED (never the committed original is mutated).
 *
 * Prerequisites (documented in apps/web/README.md): `cargo build -p quilltap-web
 * -p quilltap-cli` and `npm run build` (the SPA dist) — this setup fails loud
 * with guidance if either is missing.
 */
export default async function globalSetup(): Promise<void> {
  const web = webBinary();
  const cli = cliBinary();
  if (!existsSync(web) || !existsSync(cli)) {
    throw new Error(
      `Missing Rust binaries. Build them first:\n  cargo build -p quilltap-web -p quilltap-cli\n(looked for ${web} and ${cli})`,
    );
  }
  const dist = spaDir();
  if (!existsSync(resolve(dist, 'index.html'))) {
    throw new Error(`Missing SPA build at ${dist}. Build it first:\n  npm run build`);
  }

  // Fresh instance dir from the committed fixture (copy, never mutate original).
  rmSync(ARTIFACTS_DIR, { recursive: true, force: true });
  mkdirSync(INSTANCE_DATA_DIR, { recursive: true });
  // The terminal PTY spawns with its cwd at `<data>/files` and transcripts under
  // `<data>/logs/terminals`; the manager doesn't create the cwd (P4.6u), so make
  // them here (a real instance provisions them).
  mkdirSync(resolve(INSTANCE_DATA_DIR, 'files'), { recursive: true });
  mkdirSync(resolve(INSTANCE_DATA_DIR, 'logs'), { recursive: true });
  copyFileSync(resolve(FIXTURES_DIR, 'salon-main.db'), resolve(INSTANCE_DATA_DIR, 'quilltap.db'));
  copyFileSync(
    resolve(FIXTURES_DIR, 'salon-mount.db'),
    resolve(INSTANCE_DATA_DIR, 'quilltap-mount-index.db'),
  );
  // The llm-logs partition (P4.6ab/ac/ad unification): the engine only OPENS
  // `quilltap-llm-logs.db` when the file exists, and an autonomous turn's LLM
  // log write treats the missing partition as a TURN failure (the run
  // auto-pauses and the settings-autonomous walk races it). A real instance
  // always has the partition (fresh provisioning creates it — the salon
  // fixture family just predates it), so copy the committed empty one: an
  // instance provisioned by the real `setup` dispatch, its llm-logs db
  // PRAGMA-rekeyed to TEST_PEPPER. Instance materialization, NOT a fixture
  // regen (the terminal_sessions precedent).
  copyFileSync(
    resolve(FIXTURES_DIR, 'salon-llm-logs.db'),
    resolve(INSTANCE_DATA_DIR, 'quilltap-llm-logs.db'),
  );

  // Lock the instance: a user-passphrase .dbkey wrapping the test pepper (and NO
  // env pepper when we launch → the server boots `needs-passphrase`). Written
  // BEFORE the migrations so the CLI can unlock via the passphrase (this also
  // exercises the Node-generated .dbkey against the real reader).
  writeFileSync(
    resolve(INSTANCE_DATA_DIR, 'quilltap.dbkey'),
    makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
  );

  // Bring the fixture schema/data up to what the engine reads (mirrors the Rust
  // test harness `common::materialize_fixture_instance`): the `turnSkippingEnabled`
  // column (v4 add-turn-skipping-field-v1) and the user-id rewrite to the engine's
  // SINGLE_USER_ID (so `listChats` — filtered by that id — sees the chats). The
  // CLI unlocks the .dbkey via QUILLTAP_DB_PASSPHRASE.
  runCliWrite(cli, 'ALTER TABLE chats ADD COLUMN turnSkippingEnabled INTEGER;', {
    allowFail: true,
  });
  // The Salon fixture predates terminal support; the terminal routes (P4.6u) need
  // the `terminal_sessions` table (the P4.1c DDL, verbatim from the Rust web test
  // harness `common::materialize_fixture_instance`). IF NOT EXISTS keeps it
  // idempotent; this is fixture-schema materialization, NOT a fixture regen.
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS terminal_sessions (' +
      'id TEXT PRIMARY KEY, chatId TEXT, label TEXT, shell TEXT, cwd TEXT, ' +
      'startedAt TEXT, exitedAt TEXT, exitCode REAL, transcriptPath TEXT, ' +
      'createdAt TEXT, updatedAt TEXT);',
  );
  // The Salon fixture may predate Document Mode; the P4.6x document dispatch
  // (lane B, wired at unification) reads/writes `chat_documents`. The columns
  // match `quilltap-core::db::chat_documents` (the frozen v4 schema). IF NOT
  // EXISTS keeps it a no-op when the fixture already carries the table — this is
  // schema materialization, NOT a fixture regen (the terminal_sessions precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS chat_documents (' +
      'id TEXT PRIMARY KEY, chatId TEXT, filePath TEXT, scope TEXT, ' +
      'mountPoint TEXT, displayTitle TEXT, isActive INTEGER, ' +
      'createdAt TEXT, updatedAt TEXT);',
  );
  // The Salon fixture predates the groups schema; the P4.6bb Workbench library
  // + destinations verbs (`survey_attachments`) read `groups` (MAIN) and
  // `group_doc_mount_links` (mount-index) on every request, and the missing
  // tables surfaced as `no such table: groups` the moment the beats activated
  // at the unit-12 ∥ P4.6bb unification. Empty tables are the honest state (the
  // fixture has no groups); the DDL is fresh_schema.json's, verbatim. IF NOT
  // EXISTS keeps it a no-op when a future fixture regen carries them — schema
  // materialization, NOT a fixture regen (the terminal_sessions precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS "groups" (' +
      '"id" TEXT PRIMARY KEY NOT NULL, "name" TEXT NOT NULL, ' +
      '"officialMountPointId" TEXT, "createdAt" TEXT NOT NULL, ' +
      '"updatedAt" TEXT NOT NULL, "description" TEXT, "instructions" TEXT, ' +
      '"state" TEXT DEFAULT \'{}\', "color" TEXT, "icon" TEXT);',
  );
  runCliWrite(cli, 'CREATE INDEX IF NOT EXISTS "idx_groups_createdAt" ON "groups" ("createdAt" DESC);');
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS "group_doc_mount_links" (' +
      '"id" TEXT PRIMARY KEY NOT NULL, "groupId" TEXT NOT NULL, ' +
      '"mountPointId" TEXT NOT NULL, "createdAt" TEXT NOT NULL, ' +
      '"updatedAt" TEXT NOT NULL);',
    { mountPoints: true },
  );
  runCliWrite(
    cli,
    'CREATE INDEX IF NOT EXISTS "idx_group_doc_mount_links_createdAt" ON "group_doc_mount_links" ("createdAt" DESC);',
    { mountPoints: true },
  );
  // The Salon fixture predates embedding profiles; the P4.6z Scriptorium scan
  // enqueues mount-chunk embeddings, whose `default_profile_id` read touches
  // `embedding_profiles` (in the MAIN db). With the table absent the read errors
  // and the whole scan fails; an EMPTY table lets the enqueue skip gracefully
  // (no default profile → 0 jobs). IF NOT EXISTS keeps it a no-op when present —
  // schema materialization, NOT a fixture regen (the terminal_sessions precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS embedding_profiles (' +
      'id TEXT PRIMARY KEY NOT NULL, userId TEXT NOT NULL, name TEXT NOT NULL, ' +
      'provider TEXT NOT NULL, apiKeyId TEXT, baseUrl TEXT, modelName TEXT NOT NULL, ' +
      'dimensions REAL, truncateToDimensions REAL, normalizeL2 INTEGER DEFAULT 1, ' +
      "isDefault INTEGER DEFAULT 0, tags TEXT DEFAULT '[]', " +
      'createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);',
  );
  // The Salon fixture predates the general-files `folders` table (v4
  // FolderSchema: path/name/parentFolderId/projectId — all TEXT); the P4.6ae/ah
  // files surface reads it on every /files folder list and the P4.6af data beat
  // (self-activated at the P4.6ah unification) creates + browses a folder. A
  // real instance provisions it; the fixture just predates the files family.
  // IF NOT EXISTS keeps it a no-op when present — schema materialization, NOT a
  // fixture regen (the terminal_sessions precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS folders (' +
      'id TEXT PRIMARY KEY, userId TEXT, path TEXT, name TEXT, ' +
      'parentFolderId TEXT, projectId TEXT, createdAt TEXT, updatedAt TEXT);',
  );
  // The Salon fixture predates the `instance_settings` key/value store (a real
  // instance's version guard creates it at boot). The P4.d3 Data Retention card
  // WRITES `dataRetention` there; with the table absent the read tolerates it
  // (default 30) but the PUT's INSERT errors. An EMPTY table is enough (the GET
  // still falls back to the default). IF NOT EXISTS keeps it a no-op when present
  // — schema materialization, NOT a fixture regen (the terminal_sessions precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS instance_settings (' +
      '"key" TEXT PRIMARY KEY, "value" TEXT NOT NULL);',
  );
  // The P4.6ab/P4.6ac unification wire: copy the courier + image-attachment
  // chats from lane A's committed courier-images fixture into this instance so
  // the salon-courier-images beats find their content (they discover by
  // content and skip when absent). Runs BEFORE the userId rewrite below — the
  // courier fixture shares FIXTURE_USER, so the loop rewrites these rows too.
  seedCourierImagesFixture(cli);
  // The d68638b4-round §4 wire: a Tools/ roster in Aria's vault, which lights
  // the composer's Custom-tools button and self-activates the probe-guarded
  // salon-custom-tools-flow beat (mount-partition rows only — the userId
  // rewrite below does not touch them).
  seedPascalToolsFixture(cli);
  for (const table of [
    'chats',
    'connection_profiles',
    'api_keys',
    'chat_settings',
    'characters',
    'tags',
    'projects',
    'memories',
    'files',
    // P4.6ao/ap unification: image_profiles is user-scoped too — the
    // regenerate-background resolver checks `userId` ownership, and an
    // un-rewritten profile is invisible to it (the P4.6s lesson: the rewrite
    // must move EVERY user-scoped table).
    'image_profiles',
  ]) {
    runCliWrite(
      cli,
      `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      {
        allowFail: true,
      },
    );
  }
  // P4.9c: the `users` ROW ITSELF, whose identity is its PRIMARY KEY and so is
  // unreachable by the `userId` loop above. `userProfileGet` looks the acting
  // user up by id, so without this the Profile screen answers "User not found"
  // on a fixture that plainly has a user. (The same lesson as the P4.6s
  // "rewrite EVERY user-scoped table" note, one level down: the user table is
  // user-scoped by its own id.)
  runCliWrite(
    cli,
    `UPDATE users SET id = '${SINGLE_USER_ID}' WHERE id = '${FIXTURE_USER}';`,
    { allowFail: true },
  );
  // The Salon fixture predates the `text_replacement_rules` table (v4 created
  // it by MIGRATION, so fresh-generateDDL fixtures never have it — the
  // `folders`/`terminal_sessions` precedent). The P4.6ak surface reads/writes
  // it on the settings text-replacements routes and the composer beat creates
  // a rule live. Hand-DDL matching v4
  // `migrations/scripts/add-text-replacement-rules-table.ts:43-59`.
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS "text_replacement_rules" (' +
      '"id" TEXT PRIMARY KEY, "fromText" TEXT NOT NULL, "toText" TEXT NOT NULL, ' +
      '"caseSensitive" INTEGER NOT NULL DEFAULT 0, "enabled" INTEGER NOT NULL DEFAULT 1, ' +
      '"sortOrder" INTEGER NOT NULL DEFAULT 0, "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL);',
  );
  runCliWrite(
    cli,
    'CREATE INDEX IF NOT EXISTS "idx_text_replacement_rules_enabled" ON "text_replacement_rules" ("enabled");',
  );
  runCliWrite(
    cli,
    'CREATE INDEX IF NOT EXISTS "idx_text_replacement_rules_sortOrder" ON "text_replacement_rules" ("sortOrder");',
  );

  // The P4.6ak/P4.6am unification wire (dogfood #9): seed a story background on
  // "Solo Voyage" so `salon-background-flow.spec.ts` rides the LIVE
  // `chatGetBackground` dispatch + the live `/api/v1/files/{id}` byte route.
  // The bytes live where the host's LocalStorageBackend roots file storage
  // (`<instance>/files/<storageKey>` — spine.rs `base_dir.join("files")`,
  // where base_dir is the raw `--data-dir` arg, NOT its `data/` subdir); the row
  // shape mirrors the binary_routes test seed (`files.sha256` is NOT NULL in
  // this instance's schema, so the real hash is computed). v4 has no client
  // set-path for `storyBackgroundImageId` (only the unported generation
  // subsystem writes it), so SQL seeding here is the faithful stand-in.
  const bgPng = Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
    'base64',
  );
  mkdirSync(resolve(INSTANCE_DIR, 'files', 'e2e'), { recursive: true });
  writeFileSync(resolve(INSTANCE_DIR, 'files', 'e2e', 'story-bg.png'), bgPng);
  const bgSha = createHash('sha256').update(bgPng).digest('hex');
  runCliWrite(
    cli,
    `INSERT OR REPLACE INTO files (id, userId, sha256, originalFilename, mimeType, size, linkedTo, source, category, tags, storageKey, createdAt, updatedAt) ` +
      `VALUES ('bg-e2e-file', '${SINGLE_USER_ID}', '${bgSha}', 'story-bg.png', 'image/png', ${bgPng.length}, '[]', 'IMPORTED', 'IMAGE', '[]', 'e2e/story-bg.png', ` +
      `'2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z');`,
  );
  runCliWrite(
    cli,
    `UPDATE chats SET storyBackgroundImageId = 'bg-e2e-file' WHERE title = 'Solo Voyage';`,
  );

  // P4.6ap: seed the chat-row token/cost AGGREGATES on "Solo Voyage" so the
  // chat-totals header summary has something to show.
  //
  // These four columns are exactly what v4's non-detailed breakdown reads
  // (`cost-estimation.service.ts:139-190` — "Use stored aggregates if
  // available"); it does NOT sum the messages. The columns already exist in the
  // fixture (zero/null), so this is a value seed, not schema materialization.
  //
  // The PER-MESSAGE badge needs no seed at all: the fixture's Solo Voyage
  // already carries one assistant message with promptTokens=8/completionTokens=4
  // (`d1000000-…-0002`), which is what `salon-token-cost-flow` asserts live.
  //
  // The totals only reach the UI once lane A's `chatGetCost` verb exists — the
  // summary beat is ACTIVATE-AT-UNIFY and route-mocked in-lane — but the seed
  // lands here now so the beat goes live at unification with no fixture change.
  // 12000 + 3400 = 15400 → "15.4K tokens"; 0.0234 → "$0.023" (above the $0.01
  // band edge, so 3 digits).
  runCliWrite(
    cli,
    `UPDATE chats SET totalPromptTokens = 12000, totalCompletionTokens = 3400, ` +
      `estimatedCostUSD = 0.0234, priceSource = 'openrouter' WHERE title = 'Solo Voyage';`,
  );

  // P4.6ao/ap unification: make the fixture's "Mock Images" profile RESOLVABLE
  // so the live regenerate-background beat reaches the QUEUED arm. The resolver
  // (`image_profile_resolution.rs` arm 4) needs a user-owned profile with
  // `isDefault=1` AND a non-empty `apiKeyId`; the fixture ships it with neither,
  // so the un-refused edge answered the (correct, verbatim) "No image profile
  // available…" badRequest instead. The api-key id is the fixture's own row —
  // the JOB the enqueue spawns still fails later (no live image provider), which
  // is out of scope: the beat asserts the enqueue, not the image.
  runCliWrite(
    cli,
    `UPDATE image_profiles SET apiKeyId = 'a0000001-0000-4000-8000-000000000001', isDefault = 1 ` +
      `WHERE name = 'Mock Images';`,
  );

  // P4.6as: seed LLM logs for the Inspector walk.
  //
  // The committed `salon-llm-logs.db` carries the `llm_logs` TABLE (it is a real
  // provisioned instance's partition) but ZERO rows — peeked, not assumed. So no
  // hand-DDL is needed here, only content.
  //
  // These rows are USER-SCOPED, and the userId rewrite loop above cannot reach
  // them: it runs against the MAIN db, and llm_logs lives in the llm-logs
  // partition. They are therefore inserted with SINGLE_USER_ID directly — the
  // fourth recurrence of the P4.6s lesson, sidestepped rather than repeated.
  //
  // Three rows on Solo Voyage, chosen to exercise the panel's branches:
  //  - a CHAT_MESSAGE linked to the assistant message d1…0002 (badge "Chat",
  //    usage + duration, a cpu icon on that row),
  //  - a chat-level TITLE_GENERATION with NO messageId (badge "Title", no cpu
  //    icon — the messagesWithLogs truthiness filter),
  //  - a MEMORY_EXTRACTION linked to d1…0004 (badge "Memory", so the filter
  //    groups are distinguishable).
  // Timestamps ascend A→B→C; lane A's verb returns DESC and the panel reverses,
  // so the rendered order is A, B, C.
  const llmLogRows: Array<Record<string, string>> = [
    {
      id: 'e1000000-0000-4000-8000-000000000001',
      type: 'CHAT_MESSAGE',
      messageId: 'd1000000-0000-4000-8000-000000000002',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      request: JSON.stringify({
        messageCount: 1,
        messages: [
          {
            role: 'user',
            content: 'Hello there, captain.',
            contentLength: 21,
            hasAttachments: false,
          },
        ],
        temperature: 0.7,
        maxTokens: 2048,
        toolCount: 0,
      }),
      response: JSON.stringify({ content: 'Well met, traveller!', contentLength: 20 }),
      usage: JSON.stringify({ promptTokens: 8, completionTokens: 4, totalTokens: 12 }),
      durationMs: '1234',
      createdAt: '2026-02-01T00:00:10.000Z',
    },
    {
      id: 'e1000000-0000-4000-8000-000000000002',
      type: 'TITLE_GENERATION',
      messageId: '',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      request: JSON.stringify({ messageCount: 2, messages: [], toolCount: 0 }),
      response: JSON.stringify({ content: 'Solo Voyage', contentLength: 11 }),
      usage: '',
      durationMs: '400',
      createdAt: '2026-02-01T00:00:20.000Z',
    },
    {
      id: 'e1000000-0000-4000-8000-000000000003',
      type: 'MEMORY_EXTRACTION',
      messageId: 'd1000000-0000-4000-8000-000000000004',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      request: JSON.stringify({ messageCount: 3, messages: [], toolCount: 0 }),
      response: JSON.stringify({ content: 'No memories extracted.', contentLength: 22 }),
      usage: '',
      durationMs: '900',
      createdAt: '2026-02-01T00:00:30.000Z',
    },
  ];
  for (const row of llmLogRows) {
    runCliWrite(
      cli,
      `INSERT OR REPLACE INTO llm_logs (id, userId, type, messageId, chatId, characterId, ` +
        `autonomousRunId, provider, modelName, request, response, usage, cacheUsage, ` +
        `rawProviderUsage, requestHashes, durationMs, createdAt, updatedAt) VALUES (` +
        `'${row['id']}', '${SINGLE_USER_ID}', '${row['type']}', ` +
        `${row['messageId'] ? `'${row['messageId']}'` : 'NULL'}, ` +
        `'c1000000-0000-4000-8000-000000000001', NULL, NULL, ` +
        `'${row['provider']}', '${row['modelName']}', '${row['request']}', '${row['response']}', ` +
        `${row['usage'] ? `'${row['usage']}'` : 'NULL'}, NULL, NULL, NULL, ` +
        `${row['durationMs']}, '${row['createdAt']}', '${row['createdAt']}');`,
      { llmLogs: true },
    );
  }

  // Point the fixture's OPENAI_COMPATIBLE profile at the M4 mock LLM — this must
  // happen BEFORE the server launches (the CLI write-lock refuses a live holder),
  // so the mock listens on the fixed MOCK_LLM_PORT and the spec starts it there.
  runCliWrite(
    cli,
    `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
  );

  // Launch the real server (no env pepper → locked) serving the built SPA.
  const logFd = openSync(SERVER_LOG, 'w');
  const child = spawn(
    web,
    ['--host', '127.0.0.1', '--port', String(PORT), '--data-dir', INSTANCE_DIR, '--spa-dir', dist],
    { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
  );
  child.unref();
  writeFileSync(PID_FILE, String(child.pid));

  await waitForHealth();
}

function runCliWrite(
  cli: string,
  sql: string,
  opts: { allowFail?: boolean; llmLogs?: boolean; mountPoints?: boolean } = {},
): void {
  // The CLI `--data-dir` is the INSTANCE dir (it appends `/data` — resolve.rs);
  // it unlocks the .dbkey via QUILLTAP_DB_PASSPHRASE. `--llm-logs` targets the
  // llm-logs PARTITION (`quilltap-llm-logs.db`) and `--mount-points` the
  // mount-index partition (`quilltap-mount-index.db`) instead of the main db.
  const args = ['db', '--data-dir', INSTANCE_DIR, '--write', sql];
  if (opts.llmLogs) args.splice(1, 0, '--llm-logs');
  if (opts.mountPoints) args.splice(1, 0, '--mount-points');
  const res = spawnSync(cli, args, {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0 && !opts.allowFail) {
    throw new Error(`CLI migration failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
}

/** The server env WITHOUT any inherited pepper (so it boots locked). */
function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}

/** Poll `/health` until the server answers (423 locked is "ready" for the e2e). */
async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) {
        return;
      }
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await sleep(300);
  }
  throw new Error(`server did not become ready within 30s (${lastErr}); see ${SERVER_LOG}`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
