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
  /**
   * [P4.143 Tier 2 item 10] Also emit the refused chat create's three
   * repository ERRORs + the per-chat WARN as `repoLogs` (projected from the
   * case's `logs` recording since P4.158).
   */
  recordRepoLogs?: boolean;
  /**
   * [P4.147 item 10(c)] Rename these main-partition columns on the TARGET
   * before the baseline is dumped (`[table, from, to]`) — the P4.131 plant
   * shape, so ONE restore insert per table fails on a real SQLite error.
   */
  renameColumns?: Array<[string, string, string]>;
  /**
   * [P4.158 R-G] The same plant in ANY partition (`[partition, table, from,
   * to]`, partition `main` / `mountIndex` / `llmLogs`).
   */
  renameColumnsIn?: Array<[string, string, string, string]>;
  /** [P4.158 R-G] Raw SQL run on a partition (`[partition, sql]`) — a trigger plant. */
  plantSqlIn?: Array<[string, string]>;
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

  // ── P4.D208 (`da9c4f34f`, bug 158): the seed a stale backup carries ──────
  //
  // `restore-archive-bug158.zip` is `restore-archive.zip` with its two chats
  // given the two columns: the first SEEDED (`contextSummary` byte-identical to
  // its own `scenarioText`, the shape pre-fix creation produced) and the second
  // a real summary that QUOTES the scenario. It is a DERIVATION for the same
  // reason `restore-archive-legacy-profiles.zip` is one — every other committed
  // archive carries `contextSummary: null` and `scenarioText: null` on both its
  // chats, so the strip is invisible in all of them.
  //
  // The restore corrects the seed on the way in: the heal that cleared those
  // rows will not run again, so restoring the instance EXACTLY would restore
  // the defect with it. The second chat is the other direction — a real summary
  // is not the seed, however much of the scenario it quotes.
  { name: 'restore_bug158_replace', archive: 'restore-archive-bug158.zip' },

  // ── P4.D226 (`4d370a90f`, #75): a backup from before the three states ──────
  //
  // `restore-archive-concierge-legacy.zip` is `restore-archive.zip` with its
  // chats given the four rows v4's `deriveConciergeModeFromLegacy` table tells
  // apart (UNCENSORED → unmoderated/operator/migration; OFF + dangerous →
  // locked, the override winning; dangerous alone → unmoderated/concierge/
  // classifier) plus a 4.10 row whose `conciergeMode` is already set and is
  // left alone over a stale override. `restore.ts:207` wraps the create in
  // `withConciergeModeFromLegacy`. Built by
  // `fixtures/derive-restore-archive-concierge-legacy.py`.
  { name: 'restore_concierge_legacy_replace', archive: 'restore-archive-concierge-legacy.zip' },

  // ── P4.130 (P4.124 item 14's plant): a chat v4's schema refuses ───────────
  //
  // `restore-archive-concierge-bogus.zip` is `restore-archive.zip` plus ONE
  // message-less clone carrying `conciergeMode: 'bogus'`. `restore.ts` leaves
  // a non-null mode alone, `repos.chats.create` → `validate` throws, and the
  // per-chat catch skips it with `Failed to restore chat "The Bogus Room":
  // <ZodError message>` — `summary.warnings` carries the ZodError bytes. Built
  // by `fixtures/derive-restore-archive-concierge-bogus.py`.
  {
    name: 'restore_concierge_bogus_replace',
    archive: 'restore-archive-concierge-bogus.zip',
    recordRepoLogs: true,
  },

  // ── P4.143 item 2: the restore's serde arm, planted ──────────────────────
  //
  // `restore-archive-chat-serde-arm.zip` is `restore-archive.zip` plus ONE
  // message-less clone (`c…0006`, "The Serde Room") carrying `scenarioText:
  // 5` with VALID Concierge columns. `stripScenarioSeededSummary` reads the
  // column only through `typeof … !== 'string'`, so the number reaches
  // `repos.chats.create` → `validate` (`ChatMetadataBaseSchema`), which throws
  // the ZodError (`invalid_type` at `["scenarioText"]`), and the per-chat
  // catch skips it with `Failed to restore chat "The Serde Room": <ZodError
  // message>`. v5 skips it too, at its typed decode, with serde's sentence —
  // the recorded divergence the Rust side pins both ways. Built by
  // `fixtures/derive-restore-archive-chat-serde-arm.py`.
  {
    name: 'restore_chat_serde_arm_replace',
    archive: 'restore-archive-chat-serde-arm.zip',
    recordRepoLogs: true,
  },

  // ── P4.D251 (`07b8f0209`): a backup that still carries the voice toggle ──
  //
  // `restore-archive-voice-legacy.zip` is `restore-archive.zip` with FIVE
  // settings-row clones beside the original: the retired boolean as `true`,
  // `false` and the INTEGER `1`, the boolean beside an explicit `'always'`,
  // and a current record carrying only the mode. `restore.ts:404-413` chains
  // `withImpersonationVoiceModeFromLegacy` after the Concierge translation
  // (`true`/`1` → `'ask'`, else `'off'`, an explicit mode kept, a current
  // record returned as the same reference). Every other committed archive's
  // settings row carries NEITHER key, so none of them can see the chain.
  // Built by `fixtures/derive-restore-archive-voice-legacy.py`.
  { name: 'restore_voice_legacy_replace', archive: 'restore-archive-voice-legacy.zip' },

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

  // ── P4.D158 (`2edd823c0`): the additions that ride INSIDE an existing column ─
  //
  // v4's framing, and the reason these need an archive of their own: *a new
  // column announces itself with a migration; a new key in a JSON bag or a
  // widened enum domain is invisible to every schema check.* v4 pinned four of
  // them with jest mocks over `restore.ts`; a DB-state diff can only see them
  // if some archive carries them, and NONE of the thirteen others does
  // (measured 2026-09-05).
  //
  //   1 `chats.conciergeOverride: 'UNCENSORED'` — the widened domain, on chat 1,
  //     with `'OFF'` on chat 2 so a NARROWING and a DROP are different failures.
  //   2 `chat_settings.cheapLLMSettings.allowCheapFallback: true` — default
  //     `false`, so losing it reads as a declined stand-in.
  //   3 `image_profiles.parameters.loras` — an unvalidated bag, carried beside
  //     the pre-existing `steps`.
  //   4 the `memoryRecall` instance-settings row — upserted by RAW SQL, so the
  //     value travels as an opaque string.
  //
  // Both modes: `replace` is the ordinary path, and `new-account` is the one
  // that remaps every id first — the mode in which a per-row rebuild is most
  // likely to reconstruct a record from the fields it knows about and quietly
  // leave a bag key behind.
  { name: 'restore_bag_keys_replace', archive: 'restore-archive-bag-keys.zip' },
  { name: 'restore_bag_keys_new_account', archive: 'restore-archive-bag-keys.zip', mode: 'new-account' },

  // ── P4.147 (dogfood #142): a FRESH target, no pointer alignment ──────────
  //
  // The three archives that carry their own Quilltap Uploads store, restored
  // into a freshly provisioned target whose built-in pointers name the
  // TARGET's own (wiped) stores — the disaster-recovery shape. v4 resolves
  // Uploads through the target's surviving pointer, misses, and warns
  // `Quilltap Uploads mount has not been provisioned` for each project-less
  // file it would have to replay; v5 (the P4.147 ruled divergence) pre-applies
  // the archive's built-in pointers after 22a and restores them into the
  // ARCHIVE's store. The aligned twins above stay the convergent controls.
  { name: 'restore_uploads_fresh_replace', archive: 'restore-archive-uploads.zip' },
  { name: 'restore_compact_fresh_replace', archive: 'restore-archive-compact.zip' },
  { name: 'restore_gen2_fresh_replace', archive: 'restore-archive-gen2.zip' },

  // ── P4.147 item 8 (P4.146 item 13): the fallback arm's property bags ─────
  //
  // `restore-archive-bag-nulls.zip` drops the project and group STORES, so
  // both entities take the fresh-store fallback arm, and widens the project
  // to all sixteen property keys (explicit nulls among them) and the group to
  // `color: null, icon: "⚙"`. Built by
  // `fixtures/derive-restore-archive-bag-nulls.py`.
  { name: 'restore_bag_nulls_replace', archive: 'restore-archive-bag-nulls.zip' },

  // ── P4.147 items 9 + 10(b): the archived informs + a malformed message ──
  //
  // `restore-archive-informs.zip` carries seven inform rows (`permanent`
  // true / false / absent / null / "true" / 1, one consumed) and one message
  // whose `content` is a number. v4's `ChatInformSchema` refuses the three
  // malformed flags; the restore's WARN + DEBUG lines are recorded. Built by
  // `fixtures/derive-restore-archive-informs.py`.
  {
    name: 'restore_informs_replace',
    archive: 'restore-archive-informs.zip',
  },

  // ── P4.147 item 10(a)+(c): a real SQLite error on two restore inserts ────
  //
  // `restore-archive.zip` into a target whose `chats.rightPaneVerticalSplit`
  // and `chat_documents.displayTitle` columns were RENAMED after provisioning
  // (the P4.131 plant shape): every chat create and every chat-document create
  // fails on SQLite's own `table … has no column named …`, so the per-chat
  // catch (its warning + WARN) and the 22i per-row catch are both reached with
  // a real database error rather than a validation one.
  {
    name: 'restore_sqlite_tail_replace',
    archive: 'restore-archive.zip',
    renameColumns: [
      ['chats', 'rightPaneVerticalSplit', 'rightPaneVerticalSplitPlanted'],
      ['chat_documents', 'displayTitle', 'displayTitlePlanted'],
    ],
  },

  // ── P4.158 item 1 (ruling R-A): a preserved store missing a managed file ──
  //
  // `restore-archive-damaged-store.zip` is `restore-archive.zip` with
  // `description.md` removed from Lorian's vault, the project store and the
  // group store. v4 never preserves — it projects every managed field into a
  // FRESH store, so each description comes back from the archived row; v5
  // keeps the archive's store and BACKFILLS the missing file from the same row
  // (`PRESERVE_BACKFILL`). Built by `fixtures/derive-restore-archive-damaged-
  // store.py`.
  { name: 'restore_damaged_store_replace', archive: 'restore-archive-damaged-store.zip' },

  // ── P4.158 R-G (+ R-H): every phase's per-row catch, planted ───────────
  //
  // `restore-archive-legacy.zip` into a target with ONE written column renamed
  // in every phase's table, in all three partitions (the P4.131 plant shape,
  // P4.147 item 10(c)'s idiom widened): each restore insert there fails on
  // SQLite's own `table … has no column named …`, so the per-row catch — its
  // `summary.warnings` line AND its `moduleLogger.warn` — is reached with a
  // real database error on both sides. Characters, projects and groups fail
  // too, so no store is provisioned and the mount-family tables can be planted
  // without cascading; the legacy presets then reach 22f-bis's no-mount arm
  // (R-H — the wardrobe refusal's message bytes). `instance_settings` and
  // `embedding_profiles` stay unplanted: every built-in pointer read and the
  // step-25 reconcile read them.
  {
    name: 'restore_phase_warns_replace',
    archive: 'restore-archive-legacy.zip',
    renameColumnsIn: [
      ['main', 'tags', 'nameLower', 'nameLowerPlanted'],
      ['main', 'connection_profiles', 'sortIndex', 'sortIndexPlanted'],
      ['main', 'image_profiles', 'tags', 'tagsPlanted'],
      ['main', 'embedding_profiles', 'tags', 'tagsPlanted'],
      ['main', 'memories', 'reinforcedImportance', 'reinforcedImportancePlanted'],
      ['main', 'prompt_templates', 'content', 'contentPlanted'],
      ['main', 'roleplay_templates', 'narrationDelimiters', 'narrationDelimitersPlanted'],
      ['main', 'provider_models', 'experimental', 'experimentalPlanted'],
      ['main', 'projects', 'officialMountPointId', 'officialMountPointIdPlanted'],
      ['main', 'groups', 'officialMountPointId', 'officialMountPointIdPlanted'],
      ['main', 'plugin_configs', 'enabled', 'enabledPlanted'],
      ['main', 'folders', 'path', 'pathPlanted'],
      ['main', 'character_plugin_data', 'data', 'dataPlanted'],
      ['main', 'conversation_annotations', 'characterName', 'characterNamePlanted'],
      ['main', 'vector_entries', 'embedding', 'embeddingPlanted'],
      ['main', 'conversation_chunks', 'participantNames', 'participantNamesPlanted'],
      ['main', 'tfidf_vocabularies', 'includeBigrams', 'includeBigramsPlanted'],
      ['main', 'embedding_status', 'embeddedAt', 'embeddedAtPlanted'],
      ['main', 'text_replacement_rules', 'sortOrder', 'sortOrderPlanted'],
      ['mountIndex', 'doc_mount_points', 'conversionError', 'conversionErrorPlanted'],
      ['mountIndex', 'doc_mount_folders', 'parentId', 'parentIdPlanted'],
      ['mountIndex', 'doc_mount_files', 'fileType', 'fileTypePlanted'],
      ['mountIndex', 'doc_mount_file_links', 'description', 'descriptionPlanted'],
      ['mountIndex', 'doc_mount_documents', 'plainTextLength', 'plainTextLengthPlanted'],
      ['mountIndex', 'doc_mount_chunks', 'headingContext', 'headingContextPlanted'],
      ['mountIndex', 'project_doc_mount_links', 'projectId', 'projectIdPlanted'],
      ['mountIndex', 'group_doc_mount_links', 'groupId', 'groupIdPlanted'],
      ['mountIndex', 'group_character_members', 'characterId', 'characterIdPlanted'],
      ['llmLogs', 'llm_logs', 'provider', 'providerPlanted'],
    ],
    // v5's chat-settings insert DROPS a column the table lacks (its
    // `tolerant_insert`, for vintage schemas), so a rename cannot reach its
    // catch — a trigger refuses the insert on both sides instead.
    // Characters fail by trigger too: v5's vault resolvers read NAMED
    // character columns, so a rename would break the reads 22f-bis makes (v4's
    // `findByIdRaw` is `SELECT *`) and miss R-H's no-mount arm.
    plantSqlIn: [
      [
        'main',
        `CREATE TRIGGER "planted_chat_settings_failure" BEFORE INSERT ON "chat_settings" ` +
          `BEGIN SELECT RAISE(ABORT, 'planted chat settings failure'); END`,
      ],
      [
        'main',
        `CREATE TRIGGER "planted_characters_failure" BEFORE INSERT ON "characters" ` +
          `BEGIN SELECT RAISE(ABORT, 'planted characters failure'); END`,
      ],
      // No General pointer on the target: the SHARED legacy preset then takes
      // 22f-bis's no-mount arm too (R-H), rather than a vault write into the
      // target's wiped General store, whose internal statement order is not
      // this order's surface.
      ['main', `DELETE FROM "instance_settings" WHERE "key" = 'generalMountPointId'`],
      // v5's `save_meta` pre-reads every named meta column (v4's
      // `findMetaByCharacterId` does not), so a rename trips the read first.
      [
        'main',
        `CREATE TRIGGER "planted_vector_meta_failure" BEFORE INSERT ON "vector_indices" ` +
          `BEGIN SELECT RAISE(ABORT, 'planted vector index meta failure'); END`,
      ],
    ],
  },

  // ── P4.158 item 2 (ruling R-B): two entities claiming one archived store ──
  //
  // `restore-archive-two-claimants.zip` points Riya at Lorian's vault and the
  // group at the project's store. v4 mints a fresh store for every entity;
  // v5's first claimant keeps the archived store and the second takes the
  // v4-convergent fresh arm. Built by `fixtures/derive-restore-archive-two-
  // claimants.py`.
  { name: 'restore_two_claimants_replace', archive: 'restore-archive-two-claimants.zip' },

  // ── P4.158 item 2 (ruling R-B): a duplicated archived store id ───────────
  //
  // `restore-archive-dup-store-id.zip` appends a second `doc_mount_points` row
  // carrying Lorian's vault id as a `documents` store. 22a keeps the FIRST row
  // on both sides and refuses the duplicate on its primary key. Built by
  // `fixtures/derive-restore-archive-dup-store-id.py`.
  { name: 'restore_dup_store_id_replace', archive: 'restore-archive-dup-store-id.zip' },


  // ── P4.158 item 3 (ruling R-C): a shared legacy item vs the General pointer ─
  //
  // `restore-archive-general-pointer.zip` is `restore-archive-gen2.zip` plus
  // ONE shared (`characterId: null`) outfit preset, restored into a FRESH
  // target whose General pointer names the target's own (wiped) store. 22f-bis
  // files the archetype in Quilltap General through that pointer. Built by
  // `fixtures/derive-restore-archive-general-pointer.py`.
  { name: 'restore_general_pointer_fresh_replace', archive: 'restore-archive-general-pointer.zip' },
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

/**
 * [P4.143 Tier 2 item 10] The messages a refused chat create logs on restore:
 * v4's THREE repository ERRORs (`base.repository.ts` `validate`, `_create`'s
 * rethrowing `safeQuery`, `chats.repository.ts`'s own `safeQuery` — restore
 * runs OUTSIDE `withStrictRepositoryFailures`, so no `strictFailures`) and
 * `restore.ts:239`'s `Failed to restore chat {chatId, error}` WARN (its
 * `error` the Error OBJECT — recorded as its `message`).
 */
const REPO_LOG_MESSAGES = new Set([
  'Data validation failed',
  'Error creating entity',
  'Failed to create chat',
  'Failed to restore chat',
]);

/**
 * [P4.158 R-G] Every message `lib/backup/restore/restore.ts` logs through its
 * `moduleLogger` — all 63 sites, each message distinct (counted at the pin:
 * 44 warn, 5 info, 14 debug, 0 error). EVERY restore case records them, at
 * every level, so the census is compared across the whole corpus.
 */
const RESTORE_TS_MESSAGES = [
  "All entities restored with preserved IDs - no reconciliation needed",
  "Failed to enqueue reindex after compact restore",
  "Failed to restore LLM log",
  "Failed to restore character",
  "Failed to restore character plugin data",
  "Failed to restore chat",
  "Failed to restore chat document",
  "Failed to restore chat inform",
  "Failed to restore chat settings",
  "Failed to restore connection profile",
  "Failed to restore conversation annotation",
  "Failed to restore conversation chunk",
  "Failed to restore doc mount blob",
  "Failed to restore doc mount chunk",
  "Failed to restore doc mount document",
  "Failed to restore doc mount file",
  "Failed to restore doc mount file link",
  "Failed to restore doc mount folder",
  "Failed to restore doc mount point",
  "Failed to restore embedding profile",
  "Failed to restore embedding status",
  "Failed to restore file",
  "Failed to restore folder",
  "Failed to restore group",
  "Failed to restore group character member",
  "Failed to restore group doc mount link",
  "Failed to restore image profile",
  "Failed to restore instance setting",
  "Failed to restore memory",
  "Failed to restore npm plugin",
  "Failed to restore plugin config",
  "Failed to restore project",
  "Failed to restore project doc mount link",
  "Failed to restore prompt template",
  "Failed to restore provider model",
  "Failed to restore roleplay template",
  "Failed to restore tag",
  "Failed to restore text replacement rule",
  "Failed to restore tfidf vocabulary",
  "Failed to restore theme bundle",
  "Failed to restore themes-index.json",
  "Failed to restore vector entries batch",
  "Failed to restore vector index meta",
  "Failed to restore wardrobe item",
  "No npm plugins directory in backup",
  "No themes directory in backup",
  "Post-restore embedding reconcile complete",
  "Queued full re-index for compact backup restore",
  "Renamed connection profile on restore to avoid name collision",
  "Restore operation completed",
  "Restored chat informs",
  "Restored npm plugin",
  "Restored npm plugins",
  "Restored text replacement rules",
  "Restored theme bundle",
  "Restored user-installed theme bundles",
  "Seeded connection-profile columns the archive predates",
  "Skipped duplicate folder row during restore",
  "Skipping LLM logs restore — logs database is in degraded mode",
  "Skipping duplicate text replacement rule on restore",
  "Starting restore operation",
  "Translated pre-4.10 Concierge settings for restore",
  "Translated the retired impersonated-line voice toggle for restore",
];

/**
 * [P4.147] The repository-level lines recorded beside them (validation, the
 * base `_create` rethrow, the chats wrap) — compared on the cases that wired
 * them (`restore_informs_replace`, `restore_sqlite_tail_replace`).
 */
const REPO_LEVEL_MESSAGES = ['Data validation failed', 'Error creating entity', 'Failed to create chat'];

/**
 * [P4.158 R-G] A recorded line carries EVERY context key, in the context's own
 * order (winston's), not a fixed list: an Error value is recorded as its
 * `message` (what v4's `error.message` renders), `undefined` is OMITTED (winston
 * drops it), and every other value is kept as-is (objects and arrays included).
 */
function recordContext(line: Record<string, unknown>, context?: Record<string, unknown>): void {
  for (const [key, v] of Object.entries(context ?? {})) {
    if (v === undefined) continue;
    line[key] = v instanceof Error ? v.message : v;
  }
}

/**
 * [P4.147] `withRepoLogs` widened to a caller-chosen message list and every
 * level (the restore's `Restored chat informs` is a DEBUG line) — recorded
 * before the level filter, so `LOG_LEVEL=error` does not hide it.
 */
async function withLogs<T>(
  messages: string[],
  body: () => Promise<T>,
): Promise<{ out: T; logs: Array<Record<string, unknown>> }> {
  const logs: Array<Record<string, unknown>> = [];
  const wanted = new Set(messages);
  const { Logger } = await import('@/lib/logger');
  const levels = ['error', 'warn', 'info', 'debug'] as const;
  const originals = Object.fromEntries(levels.map((l) => [l, Logger.prototype[l]]));
  for (const level of levels) {
    const original = originals[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (wanted.has(message)) {
        const line: Record<string, unknown> = { level, message };
        recordContext(line, context);
        logs.push(line);
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }
  try {
    return { out: await body(), logs };
  } finally {
    for (const level of levels) Logger.prototype[level] = originals[level] as never;
  }
}

async function runRestoreCase(
  c: {
    name: string;
    archive: string;
    mode?: string;
    alignUploadsPointer?: boolean;
    collapseFolders?: boolean;
    recordRepoLogs?: boolean;
    renameColumns?: Array<[string, string, string]>;
    renameColumnsIn?: Array<[string, string, string, string]>;
    plantSqlIn?: Array<[string, string]>;
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

    // [P4.147 item 10(c)] The column-rename plant, before the baseline so
    // both sides' baselines describe the same (planted) target.
    for (const [table, from, to] of c.renameColumns ?? []) {
      await rawQuery(`ALTER TABLE "${table}" RENAME COLUMN "${from}" TO "${to}"`);
    }
    // [P4.158 R-G] the per-partition plant.
    {
      const { getRawDatabase: rawMain } = await import('@/lib/database/backends/sqlite/client');
      const { getRawMountIndexDatabase: rawMount } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      const { getRawLLMLogsDatabase: rawLlm } = await import(
        '@/lib/database/backends/sqlite/llm-logs-client'
      );
      const dbs: Record<string, () => import('better-sqlite3').Database> = {
        main: rawMain,
        mountIndex: rawMount,
        llmLogs: rawLlm,
      };
      for (const [partition, table, from, to] of c.renameColumnsIn ?? []) {
        dbs[partition]().exec(`ALTER TABLE "${table}" RENAME COLUMN "${from}" TO "${to}"`);
      }
      for (const [partition, sql] of c.plantSqlIn ?? []) {
        dbs[partition]().exec(sql);
      }
    }

    const preState = dumpAll();

    const { restore } = await import('@/lib/backup/restore/restore');
    const run = () =>
      restore(join(archives, c.archive), {
        mode: c.mode ?? 'replace',
        targetUserId: SINGLE_USER_ID,
      });
    let summary: unknown;
    let repoLogs: Array<Record<string, unknown>> | undefined;
    let logs: Array<Record<string, unknown>>;
    // [P4.158 R-G] every case records the census; the refused-chat cases also
    // emit `repoLogs` (P4.143's shape), projected from the same recording.
    ({ out: summary, logs } = await withLogs(
      [...RESTORE_TS_MESSAGES, ...REPO_LEVEL_MESSAGES],
      run,
    ));
    if (c.recordRepoLogs) {
      repoLogs = logs
        .filter((l) => REPO_LOG_MESSAGES.has(l.message as string))
        .filter((l) => l.level === 'error' || l.level === 'warn')
        .map((l) => {
          const out: Record<string, unknown> = { level: l.level, message: l.message };
          for (const key of ['collection', 'chatId', 'error', 'strictFailures']) {
            if (key in l) out[key] = l[key];
          }
          return out;
        });
    }

    return {
      name: c.name,
      summary,
      preState,
      state: dumpAll(),
      ...(repoLogs ? { repoLogs } : {}),
      logs,
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
