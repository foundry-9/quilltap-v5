import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync, openSync } from 'node:fs';
import { resolve } from 'node:path';

import { seedArchivedCharacter } from './support/seed-archived-character';
import { makeDbKeyFile } from './support/dbkey';
import { seedCourierImagesFixture } from './support/seed-courier-fixture';
import { seedPascalToolsFixture } from './support/seed-pascal-tools-fixture';
import { seedPhotosFixture } from './support/seed-photos-fixture';
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
  MOCK_SERPER_PORT,
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
  // The P4.9H2A list enrichment reads TWO more embedding tables the vintage
  // fixture lacks: `tfidf_vocabularies` (the BUILTIN `vocabularyStats` probe —
  // with it absent the FIRST BUILTIN profile 500s every list, which is exactly
  // how the P4.9H2B CRUD beat found this) and `embedding_status` (the
  // `embeddingStats` probe swallows its error, but the empty table is the honest
  // state, and the boot conversation-render reconcile reads it too). DDL from
  // fresh_schema.json, verbatim; IF NOT EXISTS keeps both no-ops when a future
  // fixture regen carries them — schema materialization, NOT a fixture regen.
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS "tfidf_vocabularies" (' +
      '"id" TEXT PRIMARY KEY NOT NULL, "profileId" TEXT NOT NULL, ' +
      '"userId" TEXT NOT NULL, "vocabulary" TEXT NOT NULL, "idf" TEXT NOT NULL, ' +
      '"avgDocLength" REAL NOT NULL, "vocabularySize" REAL NOT NULL, ' +
      '"includeBigrams" INTEGER DEFAULT 1, "fittedAt" TEXT NOT NULL, ' +
      '"createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL);',
  );
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS "embedding_status" (' +
      '"id" TEXT PRIMARY KEY NOT NULL, "userId" TEXT NOT NULL, ' +
      '"entityType" TEXT NOT NULL, "entityId" TEXT NOT NULL, ' +
      '"profileId" TEXT NOT NULL, "status" TEXT DEFAULT \'PENDING\', ' +
      '"embeddedAt" TEXT, "error" TEXT, ' +
      '"createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL);',
  );
  // The Salon fixture predates the roleplay-templates table; P4.30 threads the
  // chat's template into every rendered message, so the sidebar's template
  // picker, the Templates settings tab and the by-id GET all read
  // `roleplay_templates` — with it absent the create dialog answers a bare
  // `sqlite error: no such table: roleplay_templates`. An EMPTY table is the
  // honest state (the fixture has no templates); the DDL is fresh_schema.json's,
  // verbatim. IF NOT EXISTS keeps it a no-op when a future fixture regen carries
  // it — schema materialization, NOT a fixture regen (the terminal_sessions
  // precedent).
  runCliWrite(
    cli,
    'CREATE TABLE IF NOT EXISTS "roleplay_templates" (' +
      '"id" TEXT PRIMARY KEY NOT NULL, "userId" TEXT, "name" TEXT NOT NULL, ' +
      '"description" TEXT, "systemPrompt" TEXT NOT NULL, ' +
      '"isBuiltIn" INTEGER DEFAULT 0, "tags" TEXT DEFAULT \'[]\', ' +
      '"delimiters" TEXT DEFAULT \'[]\', "renderingPatterns" TEXT DEFAULT \'[]\', ' +
      '"dialogueDetection" TEXT, "narrationDelimiters" TEXT DEFAULT \'*\', ' +
      '"createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL);',
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
  reconcileSeededBuiltinStores(cli);
  // The d68638b4-round §4 wire: a Tools/ roster in Aria's vault, which lights
  // the composer's Custom-tools button and self-activates the probe-guarded
  // salon-custom-tools-flow beat (mount-partition rows only — the userId
  // rewrite below does not touch them).
  seedPascalToolsFixture(cli);
  // The P4.9a wire: two photos/ gallery entries with distinctive captions, so
  // the My Photos walk finds, filters, and deletes only its OWN rows (the
  // characters walk deletes gallery tiles on the same shared server).
  // Mount-partition rows only — the userId rewrite below does not touch them.
  seedPhotosFixture(cli);
  // The P4.D64 wire: a tombstoned character (plus a group and a chat that hold
  // it) so the archive's READ surfaces have something to walk. INERT until
  // P4.D63 lands `characters.archivedAt` — the seeder probes for the column and
  // writes nothing without it, because an unarchived Marchpane would turn up in
  // every picker and redden sibling specs. It writes SINGLE_USER_ID directly, so
  // the rewrite loop below has nothing to do for these rows.
  if (seedArchivedCharacter(cli)) {
    console.log('[e2e] seeded the archived-character island (P4.D64 tombstone beats are live)');
  }
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

  // P4.24 unification wire: the seeded Inspector rows are dated 2026-02-01, and
  // with LLM_LOG_CLEANUP now REGISTERED the boot tick's cleanup job would sweep
  // them under the schema default (enabled, 30 days) before the first beat runs
  // — the cross-lane blast radius only the unified tree could show (lane B owed
  // no SPA gate; lanes C/D ran without the registration). Pin the e2e user's
  // retention to v4's "0 = keep forever" arm: logging stays ENABLED (the LIVE
  // Inspector walk depends on new rows being written), the handler's zero gate
  // returns without deleting, and — as a bonus — that arm is now exercised live
  // at every e2e boot. Full three-key bag in v4's schema key order
  // (LLMLoggingSettingsSchema: enabled / verboseMode / retentionDays).
  runCliWrite(
    cli,
    `UPDATE chat_settings SET llmLoggingSettings = ` +
      `'{"enabled":true,"verboseMode":false,"retentionDays":0}' ` +
      `WHERE userId = '${SINGLE_USER_ID}';`,
  );

  // Point the fixture's OPENAI_COMPATIBLE profile at the M4 mock LLM — this must
  // happen BEFORE the server launches (the CLI write-lock refuses a live holder),
  // so the mock listens on the fixed MOCK_LLM_PORT and the spec starts it there.
  runCliWrite(
    cli,
    `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
  );

  // Mark the mock profile as the user's default (the fixture ships isDefault=0
  // everywhere): the Brahma Console create-with-no-profile beat resolves via
  // repos.connections.findDefault, which has NO fallback — without a default it
  // answers 400 (the P4.9I1B AT-UNIFY wire, workspace-tabs remainder round).
  runCliWrite(
    cli,
    `UPDATE connection_profiles SET isDefault = 1 WHERE provider = 'OPENAI_COMPATIBLE' ` +
      `AND NOT EXISTS (SELECT 1 FROM connection_profiles WHERE isDefault = 1);`,
  );

  // P4.42: make `search_web` clear BOTH inventory gates for the salon-dialogs
  // web-search beat, deterministically (not by luck of an earlier beat's state).
  // (a) the profile gate: `allowWebSearch = 1` on every profile; and
  // (b) the chat gate: `build_chat_context` reads the ACTIVE CHARACTER
  //     participant's OWN `connectionProfileId` → that profile's allowWebSearch,
  //     so point Solo Voyage's participants at the (now web-enabled) default
  //     profile via json_set. Absent this, the picker lists `search_web` DISABLED
  //     ("Web search must be enabled in the connection profile"). The server is
  //     also launched with SERPER_API_KEY + QUILLTAP_SERPER_BASE_URL below, so the
  //     run executes through the in-worker mock Serper.
  runCliWrite(cli, `UPDATE connection_profiles SET allowWebSearch = 1;`);
  // P4.59 (dogfood #98): the Serper provider is REGISTERED now, so the per-call
  // key comes from the user's `api_keys` row — the way v4 tells a user to
  // configure it (Settings → API Keys) — and NOT from `SERPER_API_KEY`, which
  // the launch below no longer sets. Seed that row so the search beat exercises
  // the configured path end to end; without it the run would answer v4's
  // "No API key configured for Serper Web Search…" sentence, which is the whole
  // finding.
  runCliWrite(
    cli,
    `INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, createdAt, updatedAt) ` +
      `SELECT 'a1000000-0000-4000-8000-0000000005e6', userId, 'E2E Serper', 'SERPER', ` +
      `'e2e-mock-serper-key', 1, '2026-02-01T00:00:00.000Z', '2026-02-01T00:00:00.000Z' ` +
      `FROM connection_profiles LIMIT 1;`,
  );
  runCliWrite(
    cli,
    `UPDATE chats SET participants = (` +
      `SELECT json_group_array(json_set(value, '$.connectionProfileId', ` +
      `(SELECT id FROM connection_profiles WHERE provider = 'OPENAI_COMPATIBLE' LIMIT 1))) ` +
      `FROM json_each(chats.participants)) WHERE title = 'Solo Voyage';`,
  );

  // P4.17: seed a tool-run turn on "Solo Voyage" so `salon-tool-message-flow
  // .spec.ts` walks the tool-result card affordance over real data. v4/v5 both
  // persist tool results as `role:'TOOL'` rows (`saveToolMessages`), but the
  // committed salon fixture builder does not seed them into this chat, and this
  // lane may not touch the committed fixture DBs (`crates/**`); a CLI write here
  // is the owned, faithful stand-in.
  //
  // The three rows land AFTER the fixture seedTimestamp (every fixture row shares
  // `2026-02-01T00:00:00.000Z`, so their relative order among the ties is
  // DB-defined); the DISTINCT, ascending timestamps here make the turn
  // deterministic regardless of that tie order:
  //
  //  1. A host assistant turn (the calling character) — guaranteed the last row
  //     before the tool, so the fold has a valid host.
  //  2. A character-initiated run (participantId = the same character, no
  //     systemSender): `groupToolMessagesIntoAssistants` folds it into row 1 and
  //     it renders as an EMBEDDED card.
  //  3. A user-initiated Prospero run (systemSender='prospero'): stays a
  //     collapsed announcement chip that expands to the standalone card.
  //
  // The assistant carries no token counts, so it adds no `qt-token-badge` (the
  // token-cost flow asserts badges per-row, never an absolute count).
  runCliWrite(
    cli,
    `INSERT INTO chat_messages (id, chatId, type, role, content, participantId, provider, modelName, createdAt) VALUES (` +
      `'d1000000-0000-4000-8000-000000000100', 'c1000000-0000-4000-8000-000000000001', 'message', 'ASSISTANT', ` +
      `'Let me roll for that.', 'b1000000-0000-4000-8000-000000000001', 'OPENAI_COMPATIBLE', 'mock-model', ` +
      `'2026-02-01T00:00:50.000Z');`,
  );
  runCliWrite(
    cli,
    `INSERT INTO chat_messages (id, chatId, type, role, content, participantId, createdAt) VALUES (` +
      `'d1000000-0000-4000-8000-000000000101', 'c1000000-0000-4000-8000-000000000001', 'message', 'TOOL', ` +
      `'{"toolName":"rng","success":true,"result":"Rolled 1d20: [17]","prompt":"1d20","arguments":{"type":20}}', ` +
      `'b1000000-0000-4000-8000-000000000001', '2026-02-01T00:01:00.000Z');`,
  );
  runCliWrite(
    cli,
    `INSERT INTO chat_messages (id, chatId, type, role, content, systemSender, systemKind, createdAt) VALUES (` +
      `'d1000000-0000-4000-8000-000000000102', 'c1000000-0000-4000-8000-000000000001', 'message', 'TOOL', ` +
      `'{"tool":"search","toolName":"search","initiatedBy":"user","operatorName":"Charles","prompt":"lighthouse lore","result":"Found 3 references.","success":true}', ` +
      `'prospero', 'tool-run', '2026-02-01T00:02:00.000Z');`,
  );

  // Launch the real server (no env pepper → locked) serving the built SPA.
  // P4.42 + P4.59: the host builds the web-search provider at unlock because the
  // native Serper provider is REGISTERED (the site-plugins gate is unset here, as
  // on a default install), so `search_web` runs and the inventory advertises it.
  // `SERPER_API_KEY` is deliberately NOT set: the key comes from the seeded
  // `api_keys` row above, which makes the search beat a live proof of dogfood
  // #98's configured path rather than of the deprecated env fallback.
  // QUILLTAP_SERPER_BASE_URL points that provider's real blocking HTTP transport
  // at the in-worker mock Serper (no live call, no spend).
  const logFd = openSync(SERVER_LOG, 'w');
  const child = spawn(
    web,
    ['--host', '127.0.0.1', '--port', String(PORT), '--data-dir', INSTANCE_DIR, '--spa-dir', dist],
    {
      stdio: ['ignore', logFd, logFd],
      detached: true,
      env: {
        ...withoutPepper(),
        QUILLTAP_SERPER_BASE_URL: `http://127.0.0.1:${MOCK_SERPER_PORT}/search`,
      },
    },
  );
  child.unref();
  writeFileSync(PID_FILE, String(child.pid));

  await waitForHealth();
}

/**
 * P4.D130 — reconcile the built-in stores the courier seeding drags in, so boot
 * neither duplicates them nor loses what they hold.
 *
 * `seedCourierImagesFixture` copies the courier fixture's whole
 * `doc_mount_points` table, and that fixture carries its own "Quilltap General"
 * and "Quilltap Uploads". It copies MOUNT-partition tables only, so the matching
 * `instance_settings` pointers never arrive — and `ensure_builtin_mounts` is
 * idempotent by the POINTER, not by name (v4's migrations are too; that is the
 * contract, not a v5 quirk). With no pointer, boot minted a SECOND store of each
 * name. That is the duplicate-"Quilltap General" collision P4.D122 recorded live
 * (`sameName=2`), reproduced here running that one spec alone.
 *
 * It is NOT an ensure-or-adopt idempotence hole: measured on an isolated
 * instance, the provisioner mints on the first boot and adopts on the second,
 * leaving one store. So the fix belongs to the seeding, and it is decided per
 * store by what the row actually holds rather than by a hard-coded list:
 *
 *   - nothing references it (no folders, links, chunks or project links) → it is
 *     dead weight the seeder never meant to bring; DROP it and let boot mint its
 *     own. Measured today: the courier "Quilltap General" is exactly this.
 *   - something references it → ADOPT it by writing the pointer, so boot reuses
 *     the row instead of minting a rival. Measured today: "Quilltap Uploads"
 *     holds the ingested courier image (1 link, 1 folder); dropping it would
 *     orphan that image and the boot reaper would sweep it.
 *
 * Dropping the empty one rather than adopting it also keeps boot's mint ORDER
 * unchanged, so no other spec's "first enabled ordinary store" moves under it.
 * Read by NAME: the seeder remaps pinned ids.
 */
function reconcileSeededBuiltinStores(cli: string): void {
  const POINTERS: Array<[string, string]> = [
    ['Quilltap General', 'generalMountPointId'],
    ['Quilltap Uploads', 'userUploadsMountPointId'],
    ['Lantern Backgrounds', 'lanternBackgroundsMountPointId'],
  ];
  for (const [name, key] of POINTERS) {
    const rows = readMountRows(
      cli,
      `SELECT mp.id AS id, (` +
        `(SELECT COUNT(*) FROM doc_mount_folders f WHERE f.mountPointId = mp.id) + ` +
        `(SELECT COUNT(*) FROM doc_mount_file_links l WHERE l.mountPointId = mp.id) + ` +
        `(SELECT COUNT(*) FROM doc_mount_chunks c WHERE c.mountPointId = mp.id) + ` +
        `(SELECT COUNT(*) FROM project_doc_mount_links p WHERE p.mountPointId = mp.id)` +
        `) AS refs FROM doc_mount_points mp WHERE mp.name = '${name}'`,
    );
    // More than one already would mean the collision reached the committed
    // fixture itself — loud, not silently papered over.
    if (rows.length > 1) {
      throw new Error(`e2e fixture already carries ${rows.length} stores named ${name}`);
    }
    const row = rows[0];
    if (!row?.id) continue;
    if (Number(row.refs ?? 0) === 0) {
      runCliWrite(cli, `DELETE FROM doc_mount_points WHERE id = '${row.id}';`, {
        mountPoints: true,
      });
      console.log(`[e2e] dropped the seeded, unreferenced built-in store ${JSON.stringify(name)}`);
    } else {
      runCliWrite(
        cli,
        `INSERT INTO instance_settings ("key", "value") VALUES ('${key}', '${row.id}') ` +
          `ON CONFLICT("key") DO UPDATE SET "value" = excluded."value";`,
      );
      console.log(
        `[e2e] adopting the seeded built-in store ${JSON.stringify(name)} → ${row.id} ` +
          `(${row.refs} references)`,
      );
    }
  }
}

/** One `--json` raw-SQL read against the instance's MOUNT-INDEX partition. */
function readMountRows(cli: string, sql: string): Array<Record<string, unknown>> {
  const res = spawnSync(cli, ['db', '--data-dir', INSTANCE_DIR, '--mount-points', '--json', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    throw new Error(`mount-index read failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
  return JSON.parse(res.stdout || '[]') as Array<Record<string, unknown>>;
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
