# Survey: the two boot rulings of 2026-10-01 (P4.135) — D184 goes FATAL + v4 bug 175; the ledger gate is v4 bug 176 + a recorded v5 divergence

**Date:** 2026-10-01 · **v4:** `f6426e196` (clean) · **v5 `main`:** `6d44cfae2`
**Kind:** read-only measurement
Surveyor: read-only agent. All line numbers are at those two commits. v4's
migration runner lives at the repo root (`migrations/index.ts`, imported by
`instrumentation.ts:410` as `'./migrations'`), NOT under `lib/database/` as the
brief assumed — every v4 citation below uses the real path.

**The finding in one line.** Ruling 1 is a one-site flip with no red test to
flip (nothing pins the soft line; the collapse binary has no failure arm) — the
lane adds a FATAL-class arm next to the three in `host_boot_hardness.rs`, and
while there fixes a `context` byte the soft line already gets wrong
(`migrations.` where v4 writes `migration.`); Ruling 2 needs no v5 code, only
the filing plus a doc/test pin that v5 re-ensures EVERY boot where v4 skips a
ledgered migration before `shouldRun`. The ruling's "resumable, unstamped"
premise was MEASURED and holds (§D.4) — but only because ordinary victims are
not keyed until deleted, which a first read of `:403` hides.

---

## A. v4 at `f6426e196`

### A.1 The runner: skip-before-`shouldRun`, break-on-failure (`migrations/index.ts`)

The loop, verbatim structure (`:124-207`):

- **The ledger skip comes first** (`:125-129`):
  ```ts
  // Check if already completed
  if (isMigrationCompleted(state, migration.id)) {
    migrationsSkipped++;
    continue;
  }
  ```
  `isMigrationCompleted` is `state.completedMigrations.some(m => m.id === migrationId)`
  (`migrations/state.ts:210-212`). "Completed" means a row in the main DB's
  `migrations_state` table, created by `ensureSQLiteMigrationsTable`
  (`state.ts:44-62`):
  ```sql
  CREATE TABLE IF NOT EXISTS "migrations_state" (
    "id" TEXT PRIMARY KEY,
    "completedAt" TEXT NOT NULL,
    "quilltapVersion" TEXT NOT NULL,
    "itemsAffected" INTEGER NOT NULL DEFAULT 0,
    "message" TEXT
  )
  ```
  plus `migrations_metadata (key, value)`. A row is written ONLY by
  `recordCompletedMigration` (`state.ts:217-238`), which the runner calls only
  on `result.success` (`:162-163`). The record is `{id, completedAt:
  result.timestamp, quilltapVersion, itemsAffected, message}` (`:221-227`).
- **`shouldRun()` comes second** (`:131-148`); a THROW there is not fatal — ERROR
  `Error checking if migration should run` `{context: 'migrations.runMigrations',
  migrationId, error}` and `continue` (`:140-147`).
- **`run()` returning `success: false`** (`:171-182`):
  ```ts
  } else {
    failed.push(migration.id);
    logger.error('Migration failed', {
      context: 'migrations.runMigrations',
      migrationId: migration.id,
      error: result.error,
      message: result.message,
    });
    // Stop on first failure - critical migrations must succeed
    endMigration();
    break;
  }
  ```
- **`run()` throwing** (`:183-205`): ERROR `Migration threw an exception`
  `{context, migrationId, error}`, a synthetic failed result, `break`.
- After the loop: INFO `Migration runner completed` `{context:
  'migrations.runMigrations', success: allSucceeded, migrationsRun,
  migrationsSkipped, failed, totalDurationMs}` (`:212-219`), then the result
  with `success: allSucceeded` (`:221-228`), `allSucceeded = failed.length === 0`
  (`:210`).

### A.2 `instrumentation.ts`: the exit (`:417-429`)

```ts
const migrationResult = await migrationRunner.runMigrations();

if (!migrationResult.success) {
  logger.error('Migrations failed - cannot start server', {
    context: 'instrumentation.register',
    failedMigrations: migrationResult.failed,
    error: migrationResult.error,
    migrationsRun: migrationResult.migrationsRun,
    migrationsSkipped: migrationResult.migrationsSkipped,
  });
  // Exit with code 1 to prevent container from starting with incompatible data
  process.exit(1);
}
```

Preceded by INFO `Running startup migrations` `{context: 'instrumentation.register'}`
(`:413-415`) and the runner's own INFO `Waiting for sqlite to be ready`
(`migrations/index.ts:98-100`) and INFO `Running migration` `{context,
migrationId, description}` (`:151-155`). **`process.exit(1)` sits INSIDE the
outer `try` (`:126`–`:984`), so the outer catch's ERROR `Fatal error
initializing services` (`:984-989`) is NEVER reached on a migration failure** —
the process is gone. `migrationResult.error` is `undefined` on a per-migration
failure (the runner sets `error` only on the `waitForDatabaseReady` arm,
`:104-116`); `failedMigrations` is `['collapse-duplicate-avatar-rolls-v1']`.

**The ordered v4 lines on a failed collapse**, all ERROR unless marked:

1. `Failed to collapse duplicate avatar rolls` `{context:
   'migration.collapse-duplicate-avatar-rolls', error: <message>}` — the
   migration's own catch (§A.3).
2. `Migration failed` `{context: 'migrations.runMigrations', migrationId:
   'collapse-duplicate-avatar-rolls-v1', error: <message>, message: 'Failed to
   collapse duplicate avatar rolls'}` — the runner.
3. INFO `Migration runner completed` `{…, success: false, failed:
   ['collapse-duplicate-avatar-rolls-v1'], …}`.
4. `Migrations failed - cannot start server` `{context:
   'instrumentation.register', failedMigrations: [...], error: undefined,
   migrationsRun, migrationsSkipped}`.
5. exit code 1. No `Fatal error initializing services`. `/health` never answers
   (the process is dead); before the exit it would have answered 503
   `startupPhase: 'migrations'` (`app/api/health/route.ts:164-175`).

### A.3 The collapse migration (`migrations/scripts/collapse-duplicate-avatar-rolls-v1.ts`)

- `MIGRATION_ID = 'collapse-duplicate-avatar-rolls-v1'` (`:69`); `dependsOn:
  ['add-file-generation-key-column-v1']` (`:327`).
- `shouldRun` (`:329-348`): sqlite backend, `files` exists, `generationKey`
  column exists, and at least one row matching `AVATAR_FILENAME_PREDICATE AND
  category = 'IMAGE' AND generationPrompt IS NOT NULL AND generationPrompt != ''
  AND generationKey IS NULL`.
- `run()` (`:350-662`): the whole body sits in ONE `try` (`:355`); **no
  `db.transaction(...)` anywhere in the file** (measured: `grep transaction`
  finds only the header's "Resumable rather than transactional", `:46`). The
  catch (`:642-659`):
  ```ts
  } catch (error) {
    const durationMs = Date.now() - startTime;
    const errorMessage = error instanceof Error ? error.message : String(error);

    logger.error('Failed to collapse duplicate avatar rolls', {
      context: 'migration.collapse-duplicate-avatar-rolls',
      error: errorMessage,
    });

    return {
      id: MIGRATION_ID,
      success: false,
      itemsAffected: 0,
      message: 'Failed to collapse duplicate avatar rolls',
      error: errorMessage,
      durationMs,
      timestamp: new Date().toISOString(),
    };
  }
  ```
  **It does NOT stamp the ledger on failure** — the stamp is the runner's
  `recordCompletedMigration`, reached only on `success` (§A.1). Note the
  `context` is **`migration.`** (singular) — v5's soft line writes
  `migrations.` (§B.1).
- Order inside `run()`: keying BEFORE deleting. `setKey = db.prepare('UPDATE
  files SET generationKey = ? WHERE id = ?')` at `:424`, applied to the
  `survivors` list — each group's survivor (`:395`) plus the PROTECTED victims
  only (`:403`, inside the `protectedIds.has(blobId)` branch `:398-409`);
  ordinary victims go to `victims` (`:411-412`) and stay UNKEYED until
  `deleteFileRow` removes them (`:569`). The "nothing to collapse" early return
  at `:430-435` (`success: true`). See §D.4 for why this makes the pass
  genuinely resumable.

### A.4 The ledger gate as a v4 defect (the bug-176 material)

**The mechanism.** Because `:125-129` runs before `shouldRun`, a migration whose
row exists in `migrations_state` is never consulted again — not even its own
`shouldRun`, which for the structural migrations is exactly the "is the table /
column / index still there" test:

| Migration (id, file) | Its `shouldRun` re-checks | After the row exists, v4 reaches the table only through |
|---|---|---|
| `create-help-doc-chunks-table-v1` (`migrations/scripts/create-help-doc-chunks-table.ts:28`) | `!sqliteTableExists('help_doc_chunks')` (`:33-39`) | `HelpDocChunksRepository extends AbstractBaseRepository` (`lib/database/repositories/help-doc-chunks.repository.ts:17`) — lazy `getCollection` → `safeQuery('Failed to ensure collection exists')`, no fallback, rethrows (`base.repository.ts:113-124`, prior survey §A.3); every read site is a `safeQuery` (`:63`, `:81`, `:112`, `:154`, `:168`, `:185`, `:207`) |
| `add-doc-mount-link-groups-v1` (`add-doc-mount-link-groups.ts:33`, `:40`) | `columnExists(db, 'doc_mount_file_links', 'linkGroupId')` or orphans present (`:46-62`) | the lazy `onTableEnsured` of the file-links repo (`doc-mount-file-links.repository.ts:397-408`: `ensureLinkGroupColumn(db)` at `:400`, `ensureLinkNocaseUniqueIndex(db)` at `:408`) — `ensureLinkGroupColumn` itself is `PRAGMA table_info` + `ALTER TABLE … ADD COLUMN "linkGroupId"` + `CREATE INDEX IF NOT EXISTS` (`mount-index-case-repair.ts:314-328`) |
| `provision-general-mount-v1` / `provision-user-uploads-mount-v1` / `provision-lantern-backgrounds-mount-v1` (`provision-general-mount.ts:33`, `provision-user-uploads-mount.ts:43`, `provision-lantern-backgrounds-mount.ts:39`) — each carries its own `ensureMountIndexTables` (`provision-general-mount.ts:94`, `provision-user-uploads-mount.ts:107`, `provision-lantern-backgrounds-mount.ts:103`, also `convert-project-files-to-document-stores.ts:202`) | `sqliteTableExists('instance_settings')` + the pointer (`provision-general-mount.ts:150-152`) | the mount-index repositories' lazy `ensureTable` (`dedicated-db.repository.ts:134-152`) |
| `add-doc-mount-file-links-v1` (`add-doc-mount-file-links.ts:47`, `:118`) — the `CREATE TABLE IF NOT EXISTS "doc_mount_file_links"` at `:168` | `tableExists(db, 'doc_mount_files')` and the chunks shape (`:124-136`) | the same lazy `ensureTable` |
| `collapse-duplicate-avatar-rolls-v1` (§A.3) | an unkeyed avatar row remains | nothing — a data pass; once stamped it never runs again, by design on BOTH sides (the ledger is the once-only guard v5 honours too, §B.2) |

**The lazy path, quoted** (`lib/database/repositories/dedicated-db.repository.ts:134-152`):

```ts
private async ensureTable(db: DatabaseType): Promise<void> {
  if (this.tableEnsured) return;

  try {
    const ddlStatements = generateDDL(this.collectionName, this.schema);
    for (const sql of ddlStatements) {
      db.exec(sql);
    }
    this.onTableEnsured(db);
    this.tableEnsured = true;
  } catch (error) {
    logger.error(`Failed to ensure ${this.collectionName} table in ${DB_LABELS[this.dbTarget]} database`, {
      error: extractErrorMessage(error),
    });
    throw error;
  }

  await this.afterTableReady(db);
}
```

`DB_LABELS.mountIndex = 'mount index'` (`:54-57`). `tableEnsured` is set only on
success (`:143`), so a damaged table re-runs and **re-logs on every access**,
and the rethrow lands in the caller's `withRawDb` (`:198-226`), whose default
`'fallback'` mode answers the fallback through `safeQuery` (`:225`) — the
caller's own ERROR line (`Error querying joined file links`, `Error finding
document by mount point and path`, … — P4.131's recorded set,
`crates/quilltap-harness/tests/mail_carina_tools_equivalence.rs:942-948`) follows
the `ensureTable` line on each read. P4.131's oracle plants ran this exact
rename against v4's REAL repositories and saw `Failed to ensure <table> table …`
"tripping on the very column the plant renamed" (`:937-941`).

**`/health` after boot** (`app/api/health/route.ts:188-200`): once the phase is
past `file-storage`, the service checks are `checkJsonStoreHealth` (`:197`) and
`checkFileStorageHealth` (`:200`) — nothing touches the mount index or
`help_doc_chunks` (`grep -n mount route.ts` → no hits). So the server reports
healthy while every document-store read degrades.

**A concrete v4 reproduction** (for the filing's "How to verify"):

1. Boot a fresh instance on v4 `f6426e196` once; stop it. `migrations_state`
   now holds `add-doc-mount-file-links-v1`, `add-doc-mount-link-groups-v1`,
   `provision-general-mount-v1`, `create-help-doc-chunks-table-v1` (and the
   rest).
2. On the mount-index partition run
   `ALTER TABLE doc_mount_file_links RENAME COLUMN relativePath TO relativePath_x`
   (the exact #134 plant, `host_boot_hardness.rs:266`; any sqlite3mc-capable
   client keyed with the instance pepper — v5's `Writer::open_writable` is what
   the hardness binary uses, `:137-140`).
3. Boot v4 again. The runner logs `Migration runner completed`
   `{success: true, migrationsRun: 0, migrationsSkipped: N}` — `N` counts every
   ledgered migration (`:125-129`), nothing re-runs, nothing repairs.
   `instrumentation.ts` reaches `All services initialized successfully`
   (`:955-961`). The boot-time reads that touch the table (phase 3.3b's sweep,
   the scenarios/state.json ensures — prior survey §A.1 rows 18, 23, 24) each log
   `Failed to ensure doc_mount_file_links table in mount index database`
   `{error: 'no such column: relativePath'}` + their own fallback line and
   continue.
4. `GET /api/health` → 200 `healthy`.
5. Every later document-store request repeats the pair of lines (`tableEnsured`
   never flips) and answers the empty fallback: the Scriptorium lists nothing,
   every vault read is empty — forever, with no boot-time signal.

Severity for the filing: Medium (silent, total loss of document-store reads
behind a healthy `/health`; data is intact, so not data loss). Provenance:
**Pinned** — v5 re-ensures at boot (§B.5) and the hardness binary's three FATAL
arms carry the "v5 HARDER" claim (§B.6); the day v4 re-checks `shouldRun` for
ledgered structural migrations, nothing on the v5 side trips (there is no v4
boot oracle, §C.2), so the filing's row should say "Pinned (v5 side only; no v4
oracle)" rather than claim a tripwire.

### A.5 Filing mechanics (`docs/developer/bugs.md` + `docs/developer/bugs/`)

- **Convention** (`bugs.md:871-893`): one file per bug, `bugs/bug-<n>-<short-title>.md`
  while open (`:873-878`); numbers permanent and sequential, "A new bug takes
  the next unused number" (`:879-880`); every file opens with the metadata
  table (`:881-884`); "Filing a new bug means writing the file *and* adding its
  row" (`:885-886`). Nothing deleted (`:891-893`). Provenance values —
  **Pinned / Faithful / Inert** — defined at `:924-942`.
- **The register** (`:982-988`). Header row and separator, verbatim:
  ```
  | # | Bug | Found | Fixed | Severity | What goes wrong | Fix site | v5 |
  |---|---|---|---|---|---|---|---|
  ```
  Newest row (174, `:1164`), the template:
  ```
  | 174 | [vault images reach Z.AI and NanoGPT as an internal relative URL](bugs/fixed/bug-174-vault-image-relative-url-to-provider.md) | 2026-10-01 | 2026-10-01 | Medium | `loadMountFileAsAttachment` set `url` to `/api/v1/mount-points/…/blobs/…` beside the base64 `data`; the Z.AI and NanoGPT plugins preferred `url`, so every document-store image went out as an unfetchable path and the provider 400'd (`…file must contain at least one of file_id, file_url, or file_data`) | `lib/chat-files-v2.ts` +2 plugins | Not assessed |
  ```
  An OPEN row (bug 173 as filed in `aa92cf91c`) has `—` in **Fixed** and links
  to `bugs/bug-173-…md` (not `fixed/`). **175 and 176 are unused** (`grep '^| 175\|^| 176' bugs.md` → nothing). `docs/developer/bugs/` currently holds ONLY `fixed/` — every bug is fixed, so the lane's two files are the only open ones.
- **The file's metadata keys, in order** (bug 174, `bugs/fixed/bug-174-vault-image-relative-url-to-provider.md:3-14`): **Status**, **Found**, **Fixed**, **Severity**, **Who it bites**, **Provenance**, **Defect site**, **Fix site**, **v5 status**, **Index**. The brief's list omitted **Fixed** and **Fix site** — both are present on an open filing too (bug 173 at `aa92cf91c`: `| **Status** | **Open** |`, `| **Fixed** | — |`, `| **Fix site** | Not yet fixed. Likely … |`, `| **Index** | [bugs.md](../bugs.md) |` — note the open file's Index link is `../bugs.md`, the fixed file's is `../../bugs.md`). Body sections after the `---`: **Symptom**, **Root cause**, **Why it survived**, **The fix**, **How to verify** (bug 174 `:27`, `:48`, `:87`, `:96`, `:108`); an optional v5-coordination paragraph may close it.
- **What the lane creates/edits in the v4 checkout, and nothing else:**
  1. NEW `docs/developer/bugs/bug-175-<short-title>.md` (suggested
     `bug-175-collapse-failure-exits-process.md`).
  2. NEW `docs/developer/bugs/bug-176-<short-title>.md` (suggested
     `bug-176-ledger-skips-shouldrun.md`).
  3. EDIT `docs/developer/bugs.md`: two rows appended after `:1164`, `Fixed`
     = `—`, `v5` = `Converged (fatal, P4.135)` for 175 and `Pinned (v5 re-ensures at boot)` for 176.
  The human commits them (memory note `v4-bugs-doc-location.md:26-41`: the
  v4-first pattern — "human fixes v4, v5 absorbs it in a drift catch-up"; the
  pre-commit hook rebuilds plugins but stages only what is `git add`-ed,
  `:45-46`). The lane never touches `lib/`, `app/`, `packages/`, `plugins/`,
  `migrations/`, `instrumentation.ts`, or anything under `help/`.

---

## B. v5 on `main` (`6d44cfae2`)

### B.1 The soft block (`crates/quilltap-host/src/host.rs:1646-1719`)

Inside `seed_built_ins`' one writer closure (`:1200-1844`), in the mount-aware
block (`if let Some(mi) = ws.mount_index()`, `:1600`), after the P4.D152
realign (`:1602-1645`, propagates with `?` at `:1635`). The comment and the
guard, verbatim with line numbers:

```
1661                // v4's migration body sits inside a `try/catch` that logs
1662                // `Failed to collapse duplicate avatar rolls` and reports
1663                // `success: false`; its runner records the failure, writes NO
1664                // ledger row, and the instance boots on. A `?` here would abort
1665                // the boot where v4 carries on — so the error is logged in v4's
1666                // words and swallowed, and the next boot tries again (the pass
1667                // is resumable by design, and no row was stamped).
1668                // RULING PENDING (P4.134): the "boots on" above is FALSE — v4's
1669                // runner BREAKS on `success: false` and `instrumentation.ts`
1670                // then calls `process.exit(1)` (`migrations/index.ts:162-181`,
1671                // `instrumentation.ts:417-431` at `ca363178d`), so v5 is SOFTER
1672                // than v4 here; escalated for a ruling, guard left unchanged.
1673                match quilltap_core::db::avatar_rolls_collapse_heal::collapse_duplicate_avatar_rolls(
1674                    main,
1675                    Some(mount_index),
1676                    &quilltap_core::clock::now_iso(),
1677                ) {
1678                    Ok(quilltap_core::db::avatar_rolls_collapse_heal::CollapseOutcome::Ran {
      …
1706                            "Collapsed duplicate avatar rolls into one image per configuration"
1707                        );
1708                    }
1709                    Ok(_) => {}
1710                    Err(error) => {
1711                        tracing::error!(
1712                            target: "quilltap::boot",
1713                            context = "migrations.collapse-duplicate-avatar-rolls",
1714                            error = %error,
1715                            "Failed to collapse duplicate avatar rolls"
1716                        );
1717                    }
1718                }
1719                // === end P4.D184 ===
```

Three facts about the `Err` arm:

1. It swallows: the closure continues to P4.D175 (`:1720`), the mounts
   (`:1763`), state.json (`:1787`), P4.82 (`:1801`). `assemble` succeeds.
2. **Its `context` byte is wrong today**: `migrations.collapse-duplicate-avatar-rolls`
   vs v4's `migration.collapse-duplicate-avatar-rolls` (§A.3, `:647`). Nothing
   pins it (§B.3). The lane corrects it when it flips the arm.
3. `error = %error` renders `DbError`'s Display, i.e. `sqlite error: <bare>` —
   the P4.134 convention for v4-shaped lines is the BARE driver message via
   `quilltap_core::db::fallback::error_text(&e)` (`:1775`, `:1797`; `db/fallback.rs:34`).
   v4's `error` field is `errorMessage` (`:644`), the bare message. Fix with the
   flip.

The fatal mapping already exists: a `?` out of the closure becomes
`seed_built_ins`' `Err(format!("built-in seed failed: {e}"))` (`:1844`), which
`assemble` propagates at `:530` into `HostError::Boot` → the string
`engine assembly failed: built-in seed failed: sqlite error: <bare>` the three
FATAL arms already assert (`host_boot_hardness.rs:484-487`, `:512-516`, `:536-539`).

### B.2 The collapse heal's gate and stamp (`crates/quilltap-core/src/db/avatar_rolls_collapse_heal.rs`)

- Entry `collapse_duplicate_avatar_rolls(main, mount, now_iso)` (`:596-690`):
  the ledger check FIRST (`:601-608`, "exactly as v4's runner orders it"), then
  the `shouldRun` twin (`:610-627`), then `run_pass` (`:629`), then — only on
  `Ran` — the ledger INSERT (`:669-678`) and the two metadata upserts
  (`:679-687`). **An `Err` from `run_pass` propagates before any stamp**
  (`:629` `?`): v5 matches v4's "no row on failure".
- `run_pass` order: `protected_blob_ids` (`:696`; reads
  `characters.defaultImageId`, `:210-211`, and the mount join, `:226-231`), the
  avatar SELECT (`:699-706`, names `storageKey`), keying `UPDATE files SET
  generationKey = ?1 WHERE id = ?2` (`:797`), victim `DELETE FROM files WHERE
  id = ?1` (`:876`). Header `:44-50`: "NOT one transaction — resumable by
  design", mirroring v4 `:46`.
- The predicate: `AVATAR_FILENAME_PREDICATE = r"originalFilename LIKE 'avatar\_%' ESCAPE '\'"` (`:99`).

### B.3 Test coverage of the soft behaviour (what flips, and what does not)

`crates/quilltap-host/tests/host_boot_avatar_rolls_collapse.rs` (396 lines):

- Four arms (`:11-23`): `boot_collapses_duplicate_rolls_once` (`:268`, arms 1+2),
  `boot_keeps_an_album_copy_of_a_collapsed_roll` (`:309`, arm 4),
  `a_ledger_row_from_v4_stops_the_pass` (`:373`, arm 3). **None asserts "logs
  and boots on"; none plants a failure; the binary installs NO capture layer**
  (no `CaptureLayer`, no `tracing` subscriber — `boot()` at `:257-264` is
  `Host::start(config).unwrap()` + a `Health` dispatch asserting `ready`).
- `grep -rn 'Failed to collapse duplicate avatar rolls' crates/` outside the
  heal and `host.rs:1662` → **nothing**. The soft line is unpinned; the flip
  reds no existing test.
- The seed (`plant`, `:54-201`): a hand-built `files` table **without
  `generationKey`** (`:61-68`) — the column arrives on boot from P4.D182's
  ensure (`host.rs:1310-1324` → `db/files_generation_key_repair.rs:87`, guarded
  by `PRAGMA table_info` at `:106`); no `characters` table (so
  `protected_blob_ids` answers empty at `:205-207`); the five mount tables at
  their real column lists (`:113-148`); `storageKey` = `mount-blob:mount-1:blob-<id>`
  (`:87`), which the P4.D152 realign DOES read first
  (`files_sha256_realign_heal.rs:204-206` `SELECT id, sha256, storageKey … WHERE
  storageKey LIKE 'mount-blob:%'`, propagating at `host.rs:1635`).
- **Which plant fails the collapse and nothing before it.** A column rename on
  `files` is the wrong tool: `storageKey` is read by the realign first
  (above); `originalFilename`/`category`/`generationPrompt` make the `shouldRun`
  twin answer `NotApplicable` (no unkeyed row matches), not `Err`. The precise
  plant is a TRIGGER on the collapse's own write, which no earlier step issues:
  ```sql
  CREATE TRIGGER qt_plant_collapse BEFORE UPDATE OF generationKey ON files
  BEGIN SELECT RAISE(ABORT, 'planted collapse failure'); END;
  ```
  fires at the first keying `UPDATE` (`:797`), BEFORE any delete, so the
  fixture is left exactly as planted (an honest "nothing changed" check is then
  available: both `files` rows present, both keys NULL, no ledger row). The
  P4.D182 ensure issues `ALTER TABLE … ADD COLUMN` + `CREATE INDEX IF NOT
  EXISTS` on that column (`files_generation_key_repair.rs:72`, `:87`), never an
  `UPDATE`, so the trigger is inert until the collapse. (A `BEFORE DELETE ON
  files` trigger also works and fires AFTER the survivor is keyed — the plant
  for a "resumes on the next boot" arm, §D.4 — but its readback is "survivor
  keyed, victim present", not "nothing changed".) The error reaches the host as
  `sqlite error: planted collapse failure` (RAISE(ABORT) → the message as the
  driver text).

### B.4 The hardness binary's reusable helpers (`crates/quilltap-host/tests/host_boot_hardness.rs`)

- `SERIAL: tokio::sync::Mutex<()>` (`:67`) — every arm takes it first (the
  capture is process-global, `:41-43`).
- `capture()` (`:69-79`) installs the global `CaptureLayer` once.
- `enum Substrate { Fresh, PostOffice }` (`:85-92`): `Fresh` =
  `provision_fresh_instance(&data, PEPPER)` (`:120`); `PostOffice` = the
  committed `quilltap-web/tests/fixtures/post-office-{main,mount}.db` pair +
  an `instance_settings` table (`:121-134`).
- `boot_planted(substrate, plants: &[(&str, &str)]) -> Booted` (`:115-157`):
  runs each `(partition, sql)` through `Writer::open_writable(…).execute_batch`
  (`:136-141`), clears the capture, `Host::start`, keeps `lines`; asserts
  `ready` only if the boot succeeded (`:145-150`).
- `Booted::host()` (`:160-168`, panics if the boot FAILED), `boot_error()`
  (`:170-178`, panics "the boot SUCCEEDED — this step is v4-fatal and must stay
  fatal"), `assert_line` (exactly one equal line, `:181-189`), `assert_silent`
  (`:192-195`), `assert_tool_folder_restored` (`:199-214`).
- The three FATAL arms the new one sits beside: `a_failed_migration_counterpart_still_fails_the_boot`
  (`:466-488`, a VIEW over `help_doc_chunks` → `… sqlite error: views may not be
  indexed`), `a_failed_store_provision_still_fails_the_boot` (`:490-517`),
  `a_failed_mount_index_ddl_still_fails_the_boot` (`:519-540`).
- Captured line format (from the existing asserts): `LEVEL target message
  key=value key=value` in emission order, e.g. `:272`
  `ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath`
  and `:274` `WARN quilltap::boot Error ensuring general state.json, continuing startup context=instrumentation.register error=no such column: l.relativePath`.
  So the flipped collapse line, emitted as `context` then `error`, captures as
  `ERROR quilltap::boot Failed to collapse duplicate avatar rolls context=migration.collapse-duplicate-avatar-rolls error=planted collapse failure`.
- **A `Fresh` substrate has no avatar rows**, so the new arm's plant must ALSO
  insert two unkeyed avatar rows into the fresh `files` table (shape:
  `host_boot_avatar_rolls_collapse.rs:77-91` — `originalFilename
  'avatar_Friday_<id>.webp'`, `category 'IMAGE'`, `source 'GENERATED'`, a
  non-empty `generationPrompt`, `generationKey` left NULL; give them a
  non-`mount-blob:` `storageKey` so the realign ignores them). The fresh `files`
  DDL may carry NOT NULL columns the hand-built fixture lacks — read
  `provisioning/fresh_schema.json` for `files` before writing the INSERT.

### B.5 v5's per-boot ensures that correspond to ledgered v4 migrations

Two classes inside `seed_built_ins`:

**(i) Structural ensures, re-run EVERY boot, guarded by `PRAGMA table_info` /
`IF NOT EXISTS`, propagating with `?`** (v4: a ledgered migration, run once):
`fictional_clock_anchor_repair` (`host.rs:1227-1230`),
`character_archive_repair` (`:1239`), `chat_settings_composer_repair` (`:1251`),
`…impersonation_voice_repair` (`:1264`), `connection_profiles_prefill_repair`
(`:1277`), `connection_profiles_fallback_repair` (`:1289`),
`chat_messages_route_trail_repair` (`:1299`), `chats_cycle_order_repair`
(`:1308`), `files_generation_key_repair` (`:1323`),
`chats_transcript_version_repair` (`:1339`),
`chats_moderation_refusal_ledger_repair` (`:1353`; `pragma_table_info` at
`db/chats_moderation_refusal_ledger_repair.rs:130`), `help_doc_chunks_repair::
ensure_help_doc_chunks_table` (`:1460`; `CREATE TABLE IF NOT EXISTS` +
`CREATE INDEX IF NOT EXISTS`, `db/help_doc_chunks_repair.rs:36`, `:51`),
`chat_informs::ensure_chat_informs_table` (`:1531`), and inside
`builtin_mounts::ensure_builtin_mounts_with` (`:1763-1767`): **25a** the
mount-index DDL (`services/builtin_mounts.rs:171-195`, `CREATE TABLE IF NOT
EXISTS` ×2 + the unique index), **25f** `sweep_orphaned_link_content`
(`:242`, "a ledger-gated v4 migration step"), **25h** the three store
provisions `ensure_one_mount` (`:140-142`, `:270`). None of these reads
`migrations_state` (`grep -c migrations_state` → 0 for
`help_doc_chunks_repair`, `files_generation_key_repair`,
`character_archive_repair`, `chat_informs`). The link-group column
(`mount_index_case_repair.rs:500-517`, `PRAGMA table_info` + `ADD COLUMN` +
`CREATE INDEX IF NOT EXISTS`) runs every boot too, but under `LogAndContinue`
(`builtin_mounts.rs:227-229`) — v4's lazy home.

**(ii) Data passes gated by v4's `migrations_state` row, honoured in both
directions** (no divergence): `thinking_prefill_retire_heal`,
`chat_activity_recompute_heal`, `folders_unique_path_repair`,
`generated_image_placeholder_heal`, `files_sha256_realign_heal`,
`avatar_rolls_collapse_heal`, `scenario_seeded_summary_heal` (each reads
`migrations_state`; `grep -c` 17/13/3/10/12/3/4).

The divergence Ruling 2 records is class (i) only: v5 re-checks the SHAPE every
boot; v4 trusts the row.

### B.6 The existing "v5 HARDER" pins

- `host_boot_hardness.rs:466-471` (doc comment on the help_doc_chunks FATAL arm):
  "a failed migration exits v4 (on an instance whose ledger lacks it — a
  ledger-complete instance skips it before `shouldRun`,
  `migrations/index.ts:125-129`; v5's ensures run every boot, the ledger-gate
  divergence P4.134 named)".
- `:490-496` (store provision): "a ledger-complete v4 instance reaches
  `doc_mount_folders` lazily and boots (v5 HARDER — the ledger-gate divergence,
  recorded)".
- `:519-523` (mount-index DDL): "on a ledger-complete v4 instance the migration
  is skipped and a same-named VIEW only fails the lazy index DDL per access
  (`views may not be indexed`), so v4 boots where v5 does not (the ledger-gate
  divergence, recorded)".
- `services/builtin_mounts.rs:81-93` (`LazyRepairFailures` doc): the same
  statement on the core side.
- The phase plan names it as an OPEN order: `phase-4.md:6971-6973` ("the
  ledger-gate divergence order (P4.134's item 10, now wider: …)") and the
  ruling at `:6978-6983`.

**None of these is a TEST of the re-ensure cadence** — they are doc comments on
arms that prove fatality. Nothing boots twice after a plant.

---

## C. Proof shape

### C.1 Ruling 1 — the fatal flip (host, one site; test, one new arm)

1. `host.rs:1661-1672`: rewrite the comment (the ruling, the v4 lines, the bug-175
   pointer); `:1710-1717`: keep v4's ERROR line with `context =
   "migration.collapse-duplicate-avatar-rolls"` and `error = %error_text(&error)`,
   then `return Err(error)` (or restructure to `let outcome = …?` after
   logging). No new `HostError` variant: the existing `built-in seed failed:`
   wrapper (`:1844`) is the fatal path the three FATAL arms already assert.
   Whether to ALSO emit v4's runner lines (`Migration failed`, `Migrations
   failed - cannot start server`, §A.2 items 2–4) is the order's call; the
   minimal honest port is item 1 only — v5 has no runner, and the wrapper
   string is v5's own fatal envelope — recorded as such.
2. **Red-first arm** in `host_boot_hardness.rs`, FATAL class, host level:
   `Substrate::Fresh` + plants `[(MAIN, "<two avatar INSERTs>; <the trigger>")]`
   (§B.3, §B.4). On `main` today the boot SUCCEEDS → `boot_error()` panics
   "the boot SUCCEEDED" — red-first by construction. After the flip:
   `assert_eq!(booted.boot_error(), "engine assembly failed: built-in seed failed: sqlite error: planted collapse failure")`,
   `booted.assert_line("ERROR quilltap::boot Failed to collapse duplicate avatar rolls context=migration.collapse-duplicate-avatar-rolls error=planted collapse failure")`,
   plus the nothing-changed readback through `Writer::open_writable` on
   `MAIN` (two rows, both `generationKey IS NULL`, no `migrations_state` row
   for the id). A silence leg is free: `GUARDED_MESSAGES` (`:228-237`) does not
   list the collapse line, and `an_unplanted_boot_logs_none_of_the_guarded_lines`
   (`:242`) should gain it.
3. Optional mutation proof: re-soften the arm (`Err(_) => {}`) → the arm reds on
   `boot_error()`; the doc on the arm says so, as `:46` does for the other three.
4. Nothing in `host_boot_avatar_rolls_collapse.rs` changes. If the lane wants
   the "a second boot after a failure RESUMES and completes" claim (the
   filing's premise, §D.4), it is a hardness arm: plant the `BEFORE DELETE`
   trigger, boot (fatal), `DROP TRIGGER` through `Writer::open_writable`, boot
   again, assert the survivor alone remains and the ledger row exists. Optional
   (Tier 2): it proves the premise bug 175 states rather than asserting it.

### C.2 Ruling 2 — "pinned both ways" without a v4 boot oracle

- **No v4 oracle exists**: no jest test or harness case drives
  `instrumentation.ts` (`register()` calls `process.exit`; `host_boot_hardness.rs:5-6`,
  P4.134 order `:16-20`). The v4 half of the pin is therefore the FILING
  (bug 176 with the §A.4 recipe), and the "both ways" is: (a) v5-side tests
  that assert the cadence, (b) the existing FATAL arms, (c) a
  recorded-divergence entry naming the convergence condition.
- **(a) The cadence pin** (new arm or arms in `host_boot_hardness.rs`): boot
  the Fresh substrate with the #134 plant, `booted.host()`, drop it; **boot the
  SAME tempdir again** (a second `Host::start` over `booted.data` — `Booted`
  keeps `_dir` alive) and assert the SAME `Failed to ensure doc_mount_file_links
  …` line is logged AGAIN (`assert_line` once per boot, with the capture
  cleared between). That is the observable difference: v5 re-ensures and
  re-logs at EVERY boot; v4 logs it per ACCESS and never at the ledger. The
  arm's doc states both cadences with the v4 lines (`:125-129`, `:134-152`).
  `boot_planted` needs a small sibling (`reboot(&Booted) -> Booted`, or a
  `plants` + `boots: usize` parameter) — the lane's choice.
- **(b)** the three FATAL arms stand; their doc comments get the bug-176
  number.
- **(c) The recorded-divergence entry**: `dogfood-findings.md` / the
  `phase-4.md` divergence list (where the other "recorded divergence" rows
  live: `phase-4.md:310`, `:7234`, `:8307`) — one row: "v5 re-runs every
  structural ensure each boot (`host.rs` class (i), §B.5); v4 skips a
  ledgered migration before `shouldRun` (`migrations/index.ts:125-129`) — v4
  bug 176; v5 KEEPS its ensures; converge = nothing to do on the v5 side when
  v4 re-checks, except retire this row." Also fix the two docstrings
  P4.134's close-out left false: `doc_mount_file_links.rs`'s reaper note ("v4
  offers no equivalent", P4.134 order `:12`).
- **(d) Optional, Tier 3**: a real v4-side pin through v4's importable runner
  (`migrations/index.ts` exports `MigrationRunner`; `runMigrations()` needs
  `waitForDatabaseReady` + the sqlite backend from `migrations/lib/database-utils`)
  over a jest-init DB with `migrations_state` pre-stamped for
  `create-help-doc-chunks-table-v1` and `help_doc_chunks` renamed: assert
  `migrationsSkipped` counts it and the table stays damaged. It would trip
  when v4 fixes 176. Cost: a new oracle case under the jest-oracle traps
  (registry/resetModules, the lazy help ensure — memory notes). Defer by name
  unless the round has budget.

### C.3 The filings

Two new files + two rows (§A.5), bytes to carry: for 175 the §A.2 ordered lines
and the `:642-659` catch, the "resumable" header (`:46`) with its measured proof (§D.4);
for 176 the `:125-129` skip, `state.ts:210-212`, `dedicated-db.repository.ts:134-152`,
the four migrations table (§A.4), the recipe, `/health` `:188-200`.

---

## D. Traps

### D.1 The §2 probe and the lane's own dirt

- The ledger's §2 probe (`drift-ledger.md:102-122`) must PASS at lane start:
  `git -C ~/source/quilltap-server status --short` EMPTY, HEAD `f6426e196`
  (measured clean at survey time). STOP on a failure.
- After the lane's filing, the v4 tree is dirty by EXACTLY three paths, all
  docs: `docs/developer/bugs.md` (M), `docs/developer/bugs/bug-175-*.md` (??),
  `docs/developer/bugs/bug-176-*.md` (??). The unifier's probe distinguishes
  them by that list; per the ledger `:118-122` docs-only dirt "still gets
  recorded" — the lane record names the three paths so the probe does not
  re-alarm. Anything else under `lib/`, `app/`, `packages/`, `plugins/`,
  `migrations/` is a lane error.
- The human commits in v4; the lane does not `git commit` there (memory note
  `v4-bugs-doc-location.md:26-28`). Nor does the lane run any v4 `npm`/`jest`.

### D.2 `host.rs` ownership

No other item in the human's next-round list (`phase-4.md:6984-6991`) lives in
`quilltap-host`: the bug-174 catch-up is `lib/chat-files-v2.ts` + two plugins
→ v5 `crates/quilltap-core/src/services/chat_files.rs` and the Z.AI/NanoGPT
request builders; the five silent API-key reads are `crates/quilltap-core/src/services/dangerous_content/provider_routing.rs:141`'s
shape across core services; P4.D243-F1 is `build_context`
(`services/message_context.rs`); the orchestrator mock lift and the
`memory_pipeline_jobs_tier3` blindness are harness/oracle
(`crates/quilltap-harness/tests/{orchestrator_tier3,memory_pipeline_jobs_tier3}_equivalence.rs`
+ `harness/oracle/`); the two unscoped key re-resolutions are core engine; the
sweep self-test is `harness/tools/recipe_sweep.py`. So P4.135 owns
`crates/quilltap-host/src/host.rs` and `crates/quilltap-host/tests/host_boot_hardness.rs`
outright; the round's §R ownership table should say so and mark
`host_boot_avatar_rolls_collapse.rs` as read-only for everyone. The host crate
bumps `0.0.171 → 0.0.172` (`crates/quilltap-host/Cargo.toml:3`) — the only
lane bumping host, so no silent same-bump merge this round (the `ca363178d`
record's catch).

### D.3 The collapse binary's seed

`host_boot_avatar_rolls_collapse.rs` seeds `files` WITHOUT `generationKey`
(`:61-68`) and relies on P4.D182's ensure to add it at boot
(`files_generation_key_repair.rs:87`, guarded by `PRAGMA table_info` `:106`).
A FATAL arm placed in THAT binary would still pass the ensure (ALTER, no
UPDATE), but the binary has no capture layer and its `boot()` `unwrap`s
`Host::start` (`:258`) — put the arm in `host_boot_hardness.rs` instead (§B.4).
Do not add a global subscriber to the collapse binary: two binaries sharing a
`set_global_default` pattern is fine (each is its own process), but its three
arms would then need `SERIAL` too.

### D.4 "Resumable, unstamped" — the ruling's premise, measured (it HOLDS)

Both headers say the pass is resumable because it is not one transaction
(v4 `:46`; v5 `:44-50`). The surveyor's first reading of `:403`
(`survivors.push({ id: victim.id, key })`) suggested every victim is keyed
before the delete loop, which would make `shouldRun`'s `generationKey IS NULL`
test (`:342`) answer `false` after a mid-delete failure and strand the
duplicates. **That reading is wrong**: `:403` sits inside the
`protectedIds.has(blobId)` branch (`:398-409`) — only a victim still serving
as a character's portrait is keyed (and kept, `:404`); every ordinary victim
is pushed to `victims` (`:411-412`) and stays UNKEYED until its own
`deleteFileRow` (`:569`). So a failure anywhere in the delete loop leaves the
not-yet-deleted victims unkeyed; the next boot's `shouldRun` finds them, the
runner re-runs the pass, the earlier-keyed survivors regroup under the same v0
key, and the pass completes. v5 is identical (`:797` keys the same list;
`:876` deletes; the twin at `:621`). The premise bug 175 states — "the pass is
resumable and writes no row on failure, so exiting the process buys nothing" —
is TRUE on both sides and can be filed as written. The `UPDATE OF
generationKey` trigger (§B.3) is still the better plant for the FATAL arm
because its readback is "nothing changed"; the `BEFORE DELETE` trigger is the
plant for an optional resume arm (§C.1 item 4).

### D.5 Smaller traps

- The `context` byte (§B.1 item 2) and the `sqlite error:` prefix (item 3)
  are pre-existing defects of the soft line — fix them WITH the flip and pin
  them in the new arm, or the arm will pin the wrong bytes.
- `assert_line` requires EXACTLY one hit (`:181-189`); a line logged by both
  the heal and the host would count two. The heal module logs nothing on
  `Err` (its `Err` propagates, `:629`); only the host site logs — one hit.
- v4's `error` on `Migrations failed - cannot start server` is `undefined` on
  this path (§A.2); do not invent a value if the lane chooses to emit that line.
- A `RENAME COLUMN` plant on `files` is caught by the realign first
  (`:1635`, propagating) — a wrong plant would prove the realign's fatality,
  not the collapse's. The trigger plant is step-specific.
- The recorded-divergence rows elsewhere call the shape "deliberate divergence,
  pinned both ways" — here "both ways" is (a)+(b)+(c) of §C.2, and the order
  must say so explicitly, since there is no tripwire that fires on v4's fix.

---

## What the order should say

**Tier 1 — must land**
1. `host.rs:1661-1717`: the collapse `Err` arm goes FATAL — log v4's line
   (`context = "migration.collapse-duplicate-avatar-rolls"`, bare `error_text`),
   then propagate; comment rewritten to the ruling with the bug-175 pointer.
2. `host_boot_hardness.rs`: a fourth FATAL arm (`Fresh` + two unkeyed avatar
   rows + the `BEFORE UPDATE OF generationKey` trigger), red-first on `main`,
   asserting the boot error string, the captured line, and the nothing-changed
   readback; the collapse line added to `GUARDED_MESSAGES`' silence sweep.
3. The cadence pin: an arm that boots TWICE after the #134 plant and sees the
   `Failed to ensure doc_mount_file_links …` line on BOTH boots, doc'd against
   v4's once-per-access `:134-152` and the `:125-129` skip (bug 176).
4. File v4 bugs 175 and 176 (two files + two register rows, §A.5), in the v4
   checkout, uncommitted; name the three dirty paths in the lane record.
5. Host `0.0.172`; CHANGELOG; status-log lane record; the recorded-divergence
   row (§C.2(c)); the bug numbers added to the three FATAL arms' and
   `LazyRepairFailures`' doc comments; the stale reaper docstring in
   `doc_mount_file_links.rs` corrected.

**Tier 2 — should land**
6. Decide and record whether v5 also emits v4's runner/instrumentation lines
   (§A.2 items 2–4) on the fatal path; default NO, recorded as v5's own fatal
   envelope (`built-in seed failed:`).
7. A mutation note on the new arm (re-soften → red), as the other FATAL arms carry.
8. The resume arm (§C.1 item 4): `BEFORE DELETE` trigger → fatal boot → drop
   the trigger → second boot completes the pass and stamps the row — the
   measured proof of bug 175's premise (§D.4).

**Tier 3 — explicit deferrals**
9. The v4-side runner pin through `MigrationRunner` (§C.2(d)) — defer by name.
