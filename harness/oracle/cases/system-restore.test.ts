/**
 * @jest-environment node
 *
 * P4.9G5 restore ORACLE — drives v4's REAL restore code over the COMMITTED
 * archive family (`crates/quilltap-web/tests/fixtures/restore-archives/`), the
 * same bytes the Rust side reads.
 *
 * ── PART 1: preview (`lib/backup/restore/preview.ts:20`) ─────────────────────
 * `previewRestore(zipPath)` is filesystem-only — it extracts, counts, and
 * cleans up, touching no database. Each case emits either the 41-key
 * `RestoreSummary` or the thrown message, verbatim: the preview route leaks
 * `error.message` to the client (`system/restore/route.ts:176`), so the
 * malformed-archive wording is part of the contract.
 *
 *   preview_full              the whole archive
 *   preview_legacy            + outfit-presets.json and the legacy
 *                             equippedOutfit shape (both parse-time folds)
 *   preview_minimal           every OPTIONAL data file absent — the [] fallbacks
 *   preview_missing_required  data/tags.json absent — `readJsonArrayFile` throws
 *   preview_malformed         no quilltap-backup-* root, no manifest.json
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysrestore-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/system-restore.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_RESTORE_ARCHIVES=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *   QT_ORACLE_OUT=/tmp/oracle-system-restore.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-restore
 */

import * as fs from 'fs';
import { join, dirname } from 'node:path';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface PreviewCase {
  name: string;
  archive: string;
}

const PREVIEW_CASES: PreviewCase[] = [
  { name: 'preview_full', archive: 'restore-archive.zip' },
  { name: 'preview_legacy', archive: 'restore-archive-legacy.zip' },
  { name: 'preview_minimal', archive: 'restore-archive-minimal.zip' },
  { name: 'preview_missing_required', archive: 'restore-archive-missing-required.zip' },
  { name: 'preview_malformed', archive: 'restore-archive-malformed.zip' },
  // [P4.D46] The compact archive previews like any other; the six omitted
  // data files read as empty (the optional readers), and previewRestore never
  // sets `embeddingReconcile`.
  { name: 'preview_compact', archive: 'restore-archive-compact.zip' },
];

/**
 * ── PART 2: restore, BOTH modes (`lib/backup/restore/restore.ts:35`) ────────
 *
 * Consumed by `system_restore_state`. This half is ALSO the standing evidence for
 * three v4 restore bugs the lane found by running v4's own restore against v4's
 * own backup of a modern instance — re-run it and read `summary.warnings`:
 *
 *  1. Every `doc_mount_points` and `doc_mount_file_links` row is rejected by Zod
 *     (`dumpMountIndexTable`, `backup-service.ts:72`, is a RAW `SELECT *`, so the
 *     archive carries `includePatterns` as JSON *text* and `enabled`/`allowEmbed`
 *     as INTEGER 0/1, and `restore.ts` feeds those straight into
 *     schema-validating `create`s).
 *  2. Every user file is missed (`getFileFromExtractedBackup`, `archive.ts:334`,
 *     gates the storageKey lookup on `backupFormat === 2` while a modern manifest
 *     declares `4`).
 *  3. Phase 5 runs before the stores it writes into exist at all, so both bridges
 *     throw regardless of (2) — `user-uploads-bridge.ts:98` and
 *     `project-store-bridge.ts:131`.
 *
 * v5 diverges from all three (human ruling, 2026-07-25 — `status-log.md` →
 * "Ruling — the two v4 restore bugs"), and `system_restore_state` asserts each
 * divergence in BOTH directions so neither side can drift unnoticed.
 *
 * A tier-2 DB-STATE differential, not a summary diff: the archive is restored
 * into a FRESHLY PROVISIONED, EMPTY instance and every table in all three
 * partitions is dumped row by row. A row-count map would pass on a graph whose
 * foreign keys all point at nothing; the row dump would not.
 *
 * The fresh instance is built exactly the way `build-provision-oracle.ts` builds
 * one (that differential proves a v5-provisioned instance matches it on schema
 * and seed rows — see the lane record for the baseline gap that remains).
 *
 *   restore_replace          the full archive
 *   restore_legacy_archive   the pre-rework archive, so both parse-time folds
 *                            reach the WRITE path (phase 22f-bis)
 *   restore_minimal          every optional collection absent
 *   restore_new_account      the full archive, `mode: 'new-account'` — no wipe,
 *                            every id remapped first
 *
 * P4.d23 adds four more over the two archives built for the file-replay dedupe
 * (`build-restore-archives-dedupe.test.ts`), the first that can reach the replay
 * with the archive's own store rows already in place:
 *
 *   restore_uploads_replace      / _new_account   a first-generation archive
 *                            whose one `files` row is store-backed
 *   restore_gen2_replace         / _new_account   a second-generation archive,
 *                            whose files already sit at `restored/…`
 *
 * P4.D152 adds one over the archive built for bug 117
 * (`build-restore-archive-bug117.test.ts`), the first whose `files` rows carry a
 * `sha256` that names bytes existing nowhere — on BOTH file branches:
 *
 *   restore_bug117_new_account   a legacy disk-key `portrait.png` through the
 *                            replay and a store-backed `plate.png` through
 *                            carried-store-rows
 *
 * P4.D31 adds two more over the archive built for the memory-id contract
 * (`build-restore-archives-memory-graph.test.ts`), the first carrying memories
 * that reference each other:
 *
 *   restore_memory_graph_replace / _new_account   four extra memories wired into
 *                            a graph, so `relatedMemoryIds` edges must still
 *                            resolve after the restore
 */
const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';
const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

const RESTORE_CASES: Array<{
  name: string;
  archive: string;
  mode?: 'replace' | 'new-account';
  alignUploadsPointer?: boolean;
  /** Run `collapse-duplicate-folders-v1` on the TARGET first (see the case). */
  collapseFolders?: boolean;
}> = [
  { name: 'restore_replace', archive: 'restore-archive.zip' },
  { name: 'restore_legacy_archive', archive: 'restore-archive-legacy.zip' },
  { name: 'restore_minimal', archive: 'restore-archive-minimal.zip' },
  // `new-account` restores ALONGSIDE what is already there: no wipe, and every
  // id in the archive is rewritten first (`remapBackupData`). The fresh instance
  // is the "already there", so this also proves the restore does not collide with
  // the seeded user / chat settings / embedding profile / built-in mounts.
  { name: 'restore_new_account', archive: 'restore-archive.zip', mode: 'new-account' },

  // ── P4.d23: the four file-replay-dedupe cases ────────────────────────────
  //
  // The five archives above cannot reach the replay at all in `replace` mode:
  // none carries a Quilltap Uploads mount, so the surviving `userUploadsMount-
  // PointId` dangles and both engines warn instead of restoring. `alignUploads-
  // Pointer` repoints the freshly provisioned target at the ARCHIVE's own
  // uploads mount before the restore, which is what disaster recovery is —
  // restoring your own backup onto your own instance. Read out of the archive on
  // both sides, so it is setup, not normalization. In `new-account` mode it is
  // deliberately NOT applied: nothing is wiped, ids are remapped, and the replay
  // correctly lands in the target's OWN uploads mount — which still shares the
  // archive's CONTENT rows, since `doc_mount_files` is global and keyed by sha.
  { name: 'restore_uploads_replace', archive: 'restore-archive-uploads.zip', alignUploadsPointer: true },
  { name: 'restore_uploads_new_account', archive: 'restore-archive-uploads.zip', mode: 'new-account' },
  { name: 'restore_gen2_replace', archive: 'restore-archive-gen2.zip', alignUploadsPointer: true },
  { name: 'restore_gen2_new_account', archive: 'restore-archive-gen2.zip', mode: 'new-account' },

  // ── P4.D31: the memory-id contract, both modes ───────────────────────────
  //
  // v4 `4ac66c29` stopped restore minting a fresh id for every memory. The seven
  // archives above all carry two memories with `relatedMemoryIds: []`, which can
  // only catch that in `replace` mode (where the archived id is a literal in the
  // archive and so is COMPARED rather than normalized). In `new-account` mode
  // both a correctly-remapped id and an incorrectly-minted one are UUIDs absent
  // from the archive, so the differential's origin-based normalizer labels each
  // `<minted-N>` and the arm is blind — verified by mutation.
  //
  // `restore-archive-memory-graph.zip` carries four extra memories wired into a
  // graph (edges both ways, across characters, two of them `aboutCharacterId`-
  // scoped). uuid-remap rewrites `id` and `relatedMemoryIds` through ONE memo,
  // so a correct restore lands a row whose id is the SAME UUID as the edge that
  // points at it — one shared `<minted-N>` label — while a fresh mint splits
  // them into two. That is what makes the `new-account` arm sensitive.
  // ── P4.D152 (bug 117): the archive that carries a `files.sha256` lie ─────
  //
  // `restore-archive-bug117.zip` gives BOTH file branches a row whose `sha256`
  // names bytes that exist nowhere — the damage v4 `0b0617fee` describes. The
  // legacy disk-key `portrait.png` goes through the REPLAY (post-fix the row
  // records the bridge's hash), and the store-backed `plate.png` — a real PNG
  // whose blob is WebP, so the input and stored hashes genuinely differ — goes
  // through the CARRIED-STORE-ROWS branch, which never sees a bridge and must
  // resolve the archived `doc_mount_blobs.sha256` by the parsed blob id. Every
  // other committed archive carries a `sha256` that already agrees with its
  // bytes, so a restore that copies the archive's value and one that asks the
  // bridge write the same row and the arm is vacuous.
  //
  // `new-account` only, deliberately. In `replace` mode the archived
  // `doc_mount_points` row restores verbatim, so v5's uploads mount keeps the
  // ARCHIVE's cached rollups where v4 refreshes them — the standing
  // `refreshStats` deferral, in a shape `V5_STATS_GAP`'s zero-fileCount
  // assertion cannot express. `new-account` restores into the target's OWN
  // freshly provisioned uploads mount, where the existing carve-out fits, and it
  // still exercises BOTH of bug 117's branches: `portrait.png` (legacy disk key)
  // replays through the bridge and `plate.png` (store-backed) takes the carried
  // branch.
  { name: 'restore_bug117_new_account', archive: 'restore-archive-bug117.zip', mode: 'new-account' },

  { name: 'restore_memory_graph_replace', archive: 'restore-archive-memory-graph.zip' },
  {
    name: 'restore_memory_graph_new_account',
    archive: 'restore-archive-memory-graph.zip',
    mode: 'new-account',
  },

  // ── P4.28 (dogfood #58): the referentially-broken archive ────────────────
  //
  // `restore-archive-orphan-links.zip` carries 9 `doc_mount_file_links` rows,
  // 7 `doc_mount_folders` rows and their 4 chunks whose `doc_mount_points`
  // parent is NOT in the archive (a document store deleted without its
  // children — measured on the real dogfood instance, 2026-08-03). On this
  // oracle's fresh generateDDL target the tables carry no foreign keys, so v4
  // inserts every orphan SILENTLY; v5 deliberately skips each one with a
  // sentence naming what is missing (the standing backup/restore ruling —
  // v5 fixes v4's bugs). The Rust side asserts the divergence in BOTH
  // directions, so this case is the tripwire that fires the day v4 grows its
  // own orphan handling.
  { name: 'restore_orphan_links_replace', archive: 'restore-archive-orphan-links.zip' },

  // ── P4.D46 (`7189a968`): the compact restore tail ────────────────────────
  //
  // `restore-archive-compact.zip` is built by v4's REAL
  // `createBackup(userId, {compact: true})` over the widened fixture: memory
  // embeddings nulled, the six derived embedding data files ABSENT,
  // `manifest.compact: true`. Restoring it reaches step 24a — the full
  // re-index enqueued BEFORE step 25's reconcile, so the reconcile's dedupe
  // sees it — plus the compact warning; every restore case (compact or not)
  // now also carries step 25's `summary.embeddingReconcile`. The
  // `alignUploadsPointer` reasoning is the P4.d23 cases' verbatim: the
  // archive carries its own Quilltap Uploads mount.
  { name: 'restore_compact_replace', archive: 'restore-archive-compact.zip', alignUploadsPointer: true },
  { name: 'restore_compact_new_account', archive: 'restore-archive-compact.zip', mode: 'new-account' },

  // ── P4.D126 (`e000d6bfc`, bug 103): the columns an older archive predates ─
  //
  // `restore-archive-legacy-profiles.zip` is the ONE archive that can see the
  // seeding: measured 2026-08-26, all ten other committed archives carry
  // exactly one profile, `OPENAI_COMPATIBLE`, with `supportsImageUpload`
  // STORED and `multiCharacterPrefill` absent — so the flag's seeding arm is
  // invisible in every one of them. This archive carries six profiles spanning
  // v4's own `restore-field-fidelity.test.ts` 4.9 block plus the two arms a
  // state diff can carry that a repository mock cannot: a stored `false` on a
  // historically-capable provider (which a truthiness seeding condition would
  // flip back on) and a lowercase `provider` (which the map must match
  // case-insensitively). It is a DERIVATION of `restore-archive-minimal.zip`,
  // because an archive older than a column is not a thing v4's writer can
  // still produce — see the builder's header.
  //
  // ⚠ The `multiCharacterPrefill` half is deliberately NOT pinned here: on a
  // freshly-provisioned (generateDDL) target that column has no DEFAULT, so
  // omitting it and writing an explicit NULL land the same cell. Its pin is
  // `restore_vintage_state`, against the migrated shape's `DEFAULT 1`.
  { name: 'restore_legacy_profiles_replace', archive: 'restore-archive-legacy-profiles.zip' },

  // ── P4.D145 (`a5df98b3f`, bug 114): the quiet duplicate-folder drop ───────
  //
  // A backup taken before `collapse-duplicate-folders-v1` ran can carry many
  // rows for one (userId, projectId, path). The unique index rejects the
  // extras; the first one restored survives and the rest are dropped QUIETLY —
  // no warning, no skipped counter, `foldersRestored` not incremented.
  //
  // `collapseFolders` runs v4's REAL migration on the freshly-provisioned
  // TARGET before the baseline is dumped, which is the only way the index gets
  // there: `generateDDL` builds indexes from a plain column list and cannot
  // express `COALESCE(...)`, so a fresh target is pre-index by construction and
  // the arm would be silently unreachable. It also models reality on both
  // sides — an instance has booted (v4's runner / v5's boot ensure) before
  // anyone restores into it. The v5 harness calls
  // `ensure_folders_unique_path_index` at the same point.
  //
  // All eleven other committed archives carry exactly ONE folder row, so none
  // of them can see this (measured 2026-09-02).
  { name: 'restore_duplicate_folders_replace', archive: 'restore-archive-duplicate-folders.zip', collapseFolders: true },
];

/** jest.setup stubs the file-storage manager; the restore file phase IS the
 *  thing under test, so the real modules are restored (`jest-real-db-oracle`). */
function applyMocks(): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/file-storage/project-store-bridge', () =>
    jest.requireActual('@/lib/file-storage/project-store-bridge'),
  );
  // [P4.D46] Step 24a resolves the default embedding profile through the
  // GLOBALLY-mocked embedding service (jest.setup pins
  // getDefaultEmbeddingProfile to null — the P4.20/P4.36 stale-mock class),
  // and its enqueue would wake the job dispatcher, which then CLAIMS the
  // fresh row mid-dump. Real service + stubbed wake, exactly as the
  // import-execute oracle does.
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );
  jest.doMock('@/lib/background-jobs/processor', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/background-jobs/processor'),
    ensureProcessorRunning: () => {},
  }));
  // The reconcile invalidates per-character HNSW handles through the
  // vector-store manager, which jest.setup also mocks — the mock made the
  // whole reconcile THROW into its catch (a null/zero result) the moment a
  // restored corpus actually carried vectors.
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
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

/** Every table in one partition, in rowid (insertion) order. BLOB columns are
 *  reported as `sha256:<hex>` so bytes are diffed without being carried. */
function dumpPartition(db: import('better-sqlite3').Database): Record<string, unknown> {
  const { createHash } = require('node:crypto');
  const tables = (
    db
      .prepare(
        `SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name`,
      )
      .all() as Array<{ name: string }>
  ).map((r) => r.name);
  const out: Record<string, unknown> = {};
  for (const t of tables) {
    const rows = db.prepare(`SELECT * FROM "${t}"`).all() as Array<Record<string, unknown>>;
    out[t] = rows.map((row) => {
      const o: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(row)) {
        o[k] = Buffer.isBuffer(v)
          ? `sha256:${createHash('sha256').update(v).digest('hex')}`
          : (v ?? null);
      }
      return o;
    });
  }
  return out;
}

async function runRestoreCase(
  c: {
    name: string;
    archive: string;
    mode?: string;
    alignUploadsPointer?: boolean;
    collapseFolders?: boolean;
  },
  archives: string,
  scratchRoot: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks();

  const work = mkdtempSync(join(scratchRoot, 'inst-'));
  mkdirSync(join(work, 'data'), { recursive: true });
  const mainPath = join(work, 'quilltap.db');
  // The mount-index must sit where BOTH the manager env var and
  // `getMountIndexDatabasePath()` resolve, or the mount-provisioning migrations
  // write a different file than the manager reads (the provision oracle's note).
  const miPath = join(work, 'data', 'quilltap-mount-index.db');
  const llPath = join(work, 'quilltap-llm-logs.db');
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = mainPath;
  process.env.SQLITE_MOUNT_INDEX_PATH = miPath;
  process.env.SQLITE_LLM_LOGS_PATH = llPath;
  process.env.QUILLTAP_DATA_DIR = work;
  delete process.env.SQLITE_WAL_MODE;

  const { initializeDatabase, rawQuery, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );

  try {
    await initializeDatabase();
    await rawQuery(
      'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
    );

    // Touch every repo so the lazily-created tables all exist (the fresh-instance
    // recipe from `build-provision-oracle.ts`).
    const repos = getRepositories() as Record<string, unknown>;
    for (const [key, repo] of Object.entries(repos)) {
      if (key === 'wardrobe') continue;
      const r = repo as { count?: () => Promise<number>; findAll?: () => Promise<unknown[]> };
      try {
        if (typeof r.count === 'function') await r.count();
        else if (typeof r.findAll === 'function') await r.findAll();
      } catch {
        /* vault-only / no table — not part of a fresh schema */
      }
    }
    const anyRepos = repos as Record<string, any>;
    const zero = '00000000-0000-0000-0000-000000000000';
    await anyRepos.chats?.getMessageCount(zero);
    await anyRepos.connections?.getApiKeysByUserId(zero);
    await anyRepos.vectorIndices?.findMetaByCharacterId(zero);
    await anyRepos.docMountBlobs?.findByFileId(zero);

    // The deterministic first-boot seed + the built-in templates and mounts.
    const { getOrCreateSingleUser } = await import('@/lib/auth/single-user');
    await getOrCreateSingleUser();
    await getOrCreateSingleUser();
    const { getSeedEmbeddingProfiles, prepareSeedEmbeddingProfile } = await import(
      '@/first-startup'
    );
    await anyRepos.embeddingProfiles.create(
      prepareSeedEmbeddingProfile(getSeedEmbeddingProfiles()[0], SINGLE_USER_ID),
    );
    await anyRepos.roleplayTemplates.seedBuiltInTemplates();
    const { provisionLanternBackgroundsMountMigration } = await import(
      '@/migrations/scripts/provision-lantern-backgrounds-mount'
    );
    const { provisionUserUploadsMountMigration } = await import(
      '@/migrations/scripts/provision-user-uploads-mount'
    );
    const { provisionGeneralMountMigration } = await import(
      '@/migrations/scripts/provision-general-mount'
    );
    await provisionLanternBackgroundsMountMigration.run();
    await provisionUserUploadsMountMigration.run();
    await provisionGeneralMountMigration.run();

    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { getRawMountIndexDatabase } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRawLLMLogsDatabase } = await import(
      '@/lib/database/backends/sqlite/llm-logs-client'
    );
    const dumpAll = () => ({
      main: dumpPartition(getRawDatabase()),
      mountIndex: dumpPartition(getRawMountIndexDatabase()),
      llmLogs: dumpPartition(getRawLLMLogsDatabase()),
    });

    // The PRE-restore baseline, dumped before anything is written. The two sides'
    // fresh instances are proven identical only as far as
    // `provisioning_equivalence` goes (schema + the seed user / chat settings /
    // embedding profile / roleplay templates / the three built-in mounts and
    // their folders) — NOT beyond it. Diffing the absolute post-state therefore
    // reports baseline differences as restore differences, which is what cost the
    // previous lane its time: v4's fresh instance carries 8 `doc_mount_chunks`
    // that no archive put there. The Rust side subtracts this from the post-state
    // and diffs only the DELTA, so the claim is "the two restores WRITE the same
    // rows" rather than "the two instances end up identical".
    // P4.d23: repoint the target at the archive's own uploads mount, BEFORE the
    // baseline is dumped so the two sides' baselines still describe the same
    // instance. `lib/instance-settings` exports no setter for this key (the
    // provisioning migration writes it directly), so use its own raw upsert.
    // P4.D145: give the target the bug-114 unique index, exactly as a booted
    // instance would have it — v4's own migration, run on v4's own target.
    if (c.collapseFolders) {
      const { collapseDuplicateFoldersMigration } = await import(
        '@/migrations/scripts/collapse-duplicate-folders'
      );
      const r = await collapseDuplicateFoldersMigration.run();
      if (!r.success) throw new Error(`collapse migration failed on target: ${r.message}`);
    }
    if (c.alignUploadsPointer) {
      const { parseBackupZip } = await import('@/lib/backup/restore/archive');
      const parsed = await parseBackupZip(join(archives, c.archive));
      rmSync(parsed.extractDir, { recursive: true, force: true });
      const row = (parsed.data.instanceSettings ?? []).find(
        (r: { key: string; value: string }) => r.key === 'userUploadsMountPointId',
      );
      if (!row) throw new Error(`${c.archive} carries no userUploadsMountPointId to align to`);
      await rawQuery(
        'INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?) ' +
          'ON CONFLICT("key") DO UPDATE SET "value" = excluded."value"',
        ['userUploadsMountPointId', row.value],
      );
    }

    const preState = dumpAll();

    const { restore } = await import('@/lib/backup/restore/restore');
    const summary = await restore(join(archives, c.archive), {
      mode: c.mode ?? 'replace',
      targetUserId: SINGLE_USER_ID,
    });

    return {
      name: c.name,
      summary,
      preState,
      state: dumpAll(),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const archives = process.env.QT_RESTORE_ARCHIVES;
  if (!archives) throw new Error('QT_RESTORE_ARCHIVES must point at the committed archive dir');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];

  const { previewRestore } = await import('@/lib/backup/restore/preview');
  for (const c of PREVIEW_CASES) {
    const zipPath = join(archives, c.archive);
    if (!fs.existsSync(zipPath)) throw new Error(`missing archive fixture: ${zipPath}`);
    try {
      const preview = await previewRestore(zipPath);
      outLines.push(JSON.stringify({ name: c.name, preview }));
    } catch (error) {
      outLines.push(
        JSON.stringify({
          name: c.name,
          error: error instanceof Error ? error.message : String(error),
        }),
      );
    }
  }

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-restore-oracle-'));
  try {
    for (const c of RESTORE_CASES) {
      outLines.push(JSON.stringify(await runRestoreCase(c, archives, scratchRoot)));
    }
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`system-restore oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('system-restore oracle', async () => {
  await main();
});
