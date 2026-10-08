# Survey — the `039f7017c` convergence (bugs 179/180/181), `f5e953a3f` daily optimize, the dependency moves, and the vendor inventory (2026-10-08)

Read-only planning survey for the `/setupphase` round over v4 `94fbb1ae3` → `f5e953a3f`
(`4.10.0-dev.112` → `4.10.0-dev.117`, 11 commits). Facts from the HUNKS; every claim
carries a file:line. v4 = `~/source/quilltap-server` at `f5e953a3f`; v5 = main at
`4895d1200`. Nothing was run (no cargo, no npm); the lockfile comparison was a
`python3 -I` over `git show <sha>:package-lock.json` copies in the scratchpad.

Commits in the span (newest first): `f5e953a3f` optimize · `039f7017c` bugs 179/180/181 ·
`06a70a76f` bug filings · `b3f937076` wardrobe tool pictures + 8 plugin rebuilds ·
`a9c99a4a0` merge (root dependency tree) · `7c8572869` wardrobe item images (#82) ·
`f9f1ba177` chat gallery backdrops · `3ee3b1342` wear ledger (#81) · `cc80dc89d` wardrobe
lists (#80) · `7c78abd49` wardrobe specs · `938144eb4` 4.10.0 release notes draft. The
wardrobe commits are surveyed in `2026-10-08-setupphase-wardrobe-spa.md`; this file covers
the rest and notes where the two overlap.

---

## A. `039f7017c` — bugs 179, 180, 181

### A.181 — the backup's memory embedding shape

#### v4 at HEAD (file:line facts)

- `lib/backup/backup-service.ts:53-65` `encodeEmbedding(embedding: Float32Array | number[] |
  Buffer | null | undefined): number[] | null` — PRE-EXISTING (it already served chunks `:257`
  and vector entries `:339`). Exact outputs: `null`/`undefined` → `null`; `Float32Array` →
  `Array.from(v)` (each f32 widened to a double, so `0.1f32` prints `0.10000000149011612`);
  plain array → `[...arr]` (copied, values untouched); `Buffer` → `Array.from(blobToFloat32(buf))`
  (header-aware); **anything else — including a `{"0":…}` object — falls through to `null`.**
- `:207-216` (the hunk): `memories = memoriesArrays.flat().map((memory) => ({ ...memory,
  embedding: encodeEmbedding(memory.embedding ?? null) }))`. Consequences the hunk does not
  spell out: (1) a repository-read memory's `Float32Array` becomes `number[]`; (2) a memory
  with NO embedding now carries an EXPLICIT `"embedding": null` key in `data/memories.json`
  (`JSON.stringify` keeps `null`), where before it followed the repository row's own key
  presence; (3) key POSITION: a spread-then-override keeps an existing `embedding` key where
  the row had it and APPENDS it last when the row lacked one. Compact mode
  (`compactBackupData`, `:557`) is unchanged — it still nulls the key afterwards.
- `lib/backup/restore/index-keyed-embedding.ts:1-30` (NEW) `decodeIndexKeyedEmbedding(value:
  unknown): unknown`, the whole rule set:
  - `null` / non-object → unchanged (`:12`); `Array.isArray` or `ArrayBuffer.isView` →
    unchanged (`:13`); prototype ≠ `Object.prototype` → unchanged (`:14`);
  - **`{}` → returned UNCHANGED** (`:17` `entries.length === 0`) — so an empty object still
    reaches `MemorySchema` and is REFUSED (the memory is lost, as before the fix);
  - each key must match `/^(0|[1-9]\d*)$/` (`:22`; `"00"`, `"-1"`, `"dims"` → unchanged),
    `index < entries.length` and unseen (`:24`; a gap → unchanged), each value
    `typeof === 'number' && Number.isFinite` (`:25`; `"x"`, `NaN` → unchanged);
  - otherwise → a dense `number[]` in index order (`:26-29`; keys in any order).
- `lib/backup/restore/restore.ts:262-281`: `let memoriesRestored = 0` before the loop; per
  memory, after the `personaId` strip, `decodeIndexKeyedEmbedding(cleanMemoryData.embedding)`;
  when the result differs from the input: DEBUG **`Decoded index-keyed memory embedding`**
  `{ memoryId: id, dimensions: decoded.length }` and the embedding is replaced; then
  `repos.memories.create(…, { id })`; `memoriesRestored++` AFTER a successful create (`:279`).
  Summary `:1185` `memories: memoriesRestored` (was `data.memories.length`). The
  `Failed to restore memory` WARN + warning string (`:282-283`) are untouched.
- `lib/backup/types.ts:75-80` NEW `SerializedMemory = Omit<Memory,'embedding'> & { embedding:
  number[] | null }`; `:288-289` `BackupData.memories: SerializedMemory[]`. ALL of types.ts's
  11 changed lines are 181 — none are wardrobe. `archive.ts:23,187` and `uuid-remap.ts:22,337`
  are pure type swaps (`Memory` → `SerializedMemory`), no behaviour.
- `help/system-backup-restore.md:213-222` (+11): one paragraph — pre-October-2026 archives
  are now read correctly; the **memories** summary figure "counts the memories actually
  restored, not merely those the archive offered".
- Jest: `__tests__/unit/lib/backup/index-keyed-embedding.test.ts` (NEW, 59 lines) — the
  `JSON.parse(JSON.stringify(new Float32Array([0.25,-0.5,1])))` round trip; keys in any order;
  an `it.each` of eleven unchanged inputs (`null`, `undefined`, `number[]`, a string, `{}`, a
  gap, `"00"`, `"-1"`, a non-numeric key, a non-number value, `NaN`); a `Float32Array` passes
  by identity; `MemorySchema.safeParse` refuses the raw shape and accepts the decoded one.
  `restore-field-fidelity.test.ts:708-762` — a `{"0":0.25,"1":-0.5}` archive row lands as
  `[0.25,-0.5]` with `summary.memories === 1`; two rows with the first `create` rejected
  (`invalid_union`) → `summary.memories === 1` + a `Failed to restore memory` warning.
  `compact-backup.test.ts:221-226` — the full-mode memory's embedding is an `Array` of
  length 3 (`toBeCloseTo(0.1)`), not the index-keyed object.

#### What the commit did NOT do (181)

- Did not touch `MemorySchema` (`lib/schemas/memory.types.ts:73-84` — the union is still
  `Float32Array | number[] | Buffer | string`, nullable/optional).
- Did not change the chunk / vector-entry encoders (already `encodeEmbedding`), compact mode,
  `.qtap` export (`lib/export/ndjson-writer.ts:145` still STRIPS `embedding`), the restore
  preview, or any other summary count (characters / chats / tags / profiles still count the
  archive's rows).
- Did not decode `{}` (left for the schema to refuse) and did not log anything on a refused
  shape — the only new line is the DEBUG on a successful decode.
- No migration, no schema move, no `backup-service.ts:665` (`writeJsonArrayFile`) change.

#### v5 main (file:line facts)

- `crates/quilltap-core/src/db/memories_read.rs:100-109` `embedding_to_value(blob)` builds the
  **index-keyed OBJECT** `{"0":…}` (v4's pre-fix `JSON.stringify(Float32Array)` shape), via
  `js_number_to_json(*f as f64)` per element; `:150-158` the `embedding` key is OMITTED on a
  NULL/empty BLOB unless `keep_nulls` (the raw-SQL path).
- `crates/quilltap-core/src/services/backup/collect.rs:406-420` `encode_embedding(blob) ->
  Value` already exists (null → `Null`, blob → `Array` of `js_number_to_json(f as f64)`) and
  serves chunks (`:867`) and vector entries (`:895`); memories are collected at `:527-532`
  straight from `memories_read::find_by_character_id` — so **v5's full backup writes the
  pre-fix object shape today**. The fix is a map over `memories` at `:532` with
  `encode_embedding`, PLUS the explicit-`null` key for un-embedded memories (v4's new
  `embedding: null`), minding key position (see the v4 note above). `:434-460` compact mode
  nulls the key (unchanged in v4).
- `crates/quilltap-core/src/services/backup/restore/rows.rs:88-120`
  `decode_index_keyed_embedding(&mut Map) -> bool` — the RULED divergence (the human,
  2026-10-07). Edge rules vs v4's: identical on gaps, `"00"`, `"-1"`, non-index keys,
  non-number values, arrays, strings, `Null`, absent; **differs on `{}` — v5 decodes it to
  `[]` (`:141-142`, stored as SQL NULL → the memory RESTORES), v4 returns it unchanged (the
  memory is REFUSED)**. Non-finite values cannot occur in serde_json input (moot). v5 logs
  nothing on decode; v4 now logs the DEBUG. Called from
  `restore/orchestrator.rs:894-897` before `parse_create_memory` (`:917`).
- `restore/orchestrator.rs:3711` `memories: data.memories.len()` — still the ARCHIVE count;
  `restore/preview.rs:27` likewise (v4's preview is unchanged, so only the summary moves).
- `crates/quilltap-core/src/db/memories.rs:152-163` `parse_create_memory`'s embedding arms:
  `Array` → `Vec<f32>`, `String` → JSON-parsed array, everything else → `None` (no object arm;
  the decode must run first — it does).

#### Harness families + recipes (181)

- `crates/quilltap-harness/tests/system_restore_state.rs` — `INDEX_KEYED_EMBEDDING` table
  `:5489-5493` (one row: `restore_memory_refusals_replace`,
  `ad000000-…-000000000009`, `[0.25]`), `carve_index_keyed_embedding` `:5495-5610`, called
  at `:1609-1627`. It asserts BOTH ways: v5 lands the id with the decoded BLOB and logs none
  of v4's lines; **v4 REFUSES the id with `Failed to restore memory` + three repository
  ERRORs — the `:5536` arm fires `v4 converged` the moment the oracle is regenerated at the
  new pin.** The ruled divergence RETIRES: drop the carve, make the row a plain comparand,
  and add v4's new DEBUG line + the written-rows summary count to the comparands.
- `system_backup_equivalence.rs` (`QT_ORACLE_SYSTEM_BACKUP`, header `:54`) — the
  `memories.json` bytes diff; it will go red on the container shape AND on the new explicit
  `embedding: null` keys. `system_restore_equivalence.rs` (`QT_ORACLE_SYSTEM_RESTORE`,
  `:28`), `system_restore_guards_equivalence.rs` (`QT_ORACLE_RESTORE_GUARDS`, `:29-33`),
  `restore_vintage_state.rs` (standalone, no oracle), `backup_uuid_remap_equivalence.rs`,
  `backup_mount_index_coercion_equivalence.rs`. Recipes live in each test's header; the
  sweep driver is `harness/tools/recipe_sweep.py` (pass `--v5w ~/source/quilltap-v5`).

#### Premise corrections / surprises (181)

- The task asked how `encodeEmbedding` treats a `{"0":…}` object: it returns **`null`** — it
  is not a decoder. Only the restore decodes.
- v4's `{}` rule is the OPPOSITE of v5's; the lane must rule whether to converge on v4's
  loss of an empty-vector memory (an `Float32Array(0)` only arises from a zero-length
  vector — effectively never) or keep v5's recovery as a recorded divergence.
- v4 now writes `embedding: null` on every un-embedded memory; v5 omits the key. A key-
  presence diff the backup family will catch, with a key-ORDER subtlety (append-last vs
  in-place) that depends on whether v4's repository row carried the key — measure in the
  oracle, do not guess.

---

### A.180 — the LLM-logs cold-open ladder

#### v4 at HEAD (file:line facts)

- `lib/database/backends/sqlite/cold-open-retry.ts:1-60` (NEW):
  `COLD_OPEN_RETRY_BACKOFF_MS = [200, 600, 1500]` (`:18`); `openWithColdOpenRetry<T>(label,
  path, moduleLogger, attempt, backoffMs = …): ColdOpenResult<T>` (`:33-60`);
  `maxAttempts = backoffMs.length + 1` (= 4); per failed attempt with a backoff left, WARN
  **`` `${label} cold-open failed — retrying` ``** with keys IN ORDER `{ path, attempt: i+1,
  maxAttempts, backoffMs: backoff, error: message }` (`:48-54`), then `sleepSync(backoff)`
  (`lib/utils/sleep.ts:15-20` — a busy-wait that blocks the event loop); returns
  `{ ok:true, value, attempts }` or `{ ok:false, error: lastError, attempts: maxAttempts }`.
  Never throws. Labels: `'LLM logs'`, `'Mount index'`.
- `llm-logs-client.ts:46-85` (NEW `attemptOpenLLMLogs`): `new Database(path)` → key
  (`applySqlcipherKey`, DEBUG `SQLCipher key set on LLM logs database` — **now fires on EVERY
  attempt**, since it sits inside the attempt) → `registerTextCodecFunction` → **the verify
  probe `db.prepare('SELECT count(*) AS cnt FROM sqlite_master').get()`** (`:59`) → the
  pragmas (journal_mode default truncate, busy_timeout, cache, mmap 256 MB, temp_store) →
  `configured = true`; `finally` closes an unconfigured connection. `:95-126`
  `getLLMLogsSQLiteClient`: INFO `Initializing LLM logs database connection {path, walMode}`
  (unchanged) → the ladder → on ok: INFO **`LLM logs database connection established`
  `{ path, attempts }`** (gains `attempts`) → else ERROR **`Failed to initialize LLM logs
  database — entering degraded mode`** `{ path, attempts, error }` (gains `attempts`, always 4).
- `mount-index-client.ts`: deletes its local `OPEN_RETRY_BACKOFF_MS` and `sleepSync` import,
  calls the shared ladder (`:109-111`). **No text change**: `Mount index cold-open failed —
  retrying`, the INFO `{path, attempts}`, the ERROR `{path, attempts, error}` are byte-identical.
  `attemptOpenMountIndex` (with its pre-existing probe) untouched.
- `__tests__/unit/lib/database/backends/sqlite/cold-open-retry.test.ts` (NEW, 149 lines):
  the ladder's first-success `{ok:true, value, attempts:2}` with the WARN's exact object
  `{path:'/p', attempt:1, maxAttempts:4, backoffMs:200, error:'flake'}`; four attempts with
  sleeps `[200,600,1500]` and three WARNs; the LLM-logs client recovering from a failed probe
  (bad connection closed, `attempts: 2` on the INFO); degrading with one ERROR
  (`{attempts:4, error:'file is not a database'}`) after four `new Database` calls.

#### What the commit did NOT do (180)

- The MAIN database open (`getSQLiteClient`) has no ladder — unchanged. The integrity checks
  (`*-protection.ts` `quick_check`) are unchanged. No `isLLMLogsDegraded` semantics change.
- No change to `backend.ts`'s `connect()` order (LLM logs first, then the mount index).
- Did not make the sleep async or injectable; the boot still blocks ~2.3 s on a dead file.

#### v5 main (file:line facts)

- `crates/quilltap-core/src/db/runtime.rs:45-51` module doc records the ASYMMETRY as v4's;
  `:93-94` `MOUNT_INDEX_OPEN_BACKOFF_MS = [200, 600, 1500]`; `:235-285` `open_mount_index`
  (the ladder: WARN `Mount index cold-open failed — retrying` at `:261-269` with keys
  `path, attempt, maxAttempts, backoffMs, error`; ERROR `…entering degraded mode` `:276-282`
  with `attempts = max_attempts`); `:287-327` `open_llm_logs` — **ONE attempt, no probe**:
  INFO `Initializing…` (`:294-299`, `walMode=false`), DEBUG `SQLCipher key set on LLM logs
  database` logged ONCE ahead of the open (`:300-304`), INFO `established` WITHOUT `attempts`
  (`:307-312`), ERROR WITHOUT `attempts` (`:316-322`). The probe question is already ruled
  R-D (`:67-73`): `Writer::open_writable` fails at `journal_mode` with the same
  `file is not a database` bytes v4's probe raises.
- `crates/quilltap-host/tests/host_boot_hardness.rs` pins: `:1498-1505` `llm_failed()`
  (no `attempts` key); `:1605-1627` `a_garbage_llm_logs_file_degrades_with_one_error_and_boots`
  — `assert_silent("cold-open failed")` for the LLM leg (`:1620`); `:1680-1683` the sound
  path's `LLM logs database connection established module=… path=…` without `attempts=1`
  (the mount line at `:1688` carries `attempts=1`); the `NEVER_ON_SOUND` list `:313-314`.
  `MOUNT_DEGRADED_PROBLEM` / `LLM_DEGRADED_PROBLEM` (`:1456-1459`) unchanged by v4.
- `host_boot_hardness.rs:1367-1378` and `runtime.rs:45-51` cite `llm-logs-client.ts:49-98`
  for the one attempt — both citations go stale.

#### Harness families + recipes (180)

- `crates/quilltap-harness/tests/degraded_sibling_open_equivalence.rs` (header `:1-46`;
  oracle `harness/oracle/cases/degraded-sibling-open.test.ts`, staged outside `.claude/`,
  `QT_ORACLE_OUT` → `QT_ORACLE_DEGRADED_SIBLING_OPEN`): compares EVERY `module=database:*`
  line — count, order, level, keys IN ORDER, values — per partition × plant. At the new pin
  the `llmLogs × garbage` row gains three WARNs + `attempts=4` on the ERROR, the
  `llmLogs × sound` row gains `attempts=1` on the INFO, and the DEBUG `SQLCipher key set…`
  repeats per attempt (4× on garbage). Regenerate at the pin; v5 goes red by design until
  `open_llm_logs` carries the ladder.
- `host_boot_hardness.rs` (host crate, no oracle): the three pins above flip.

#### Premise corrections / surprises (180)

- v5's shared-ladder refactor is natural (one `open_sibling_with_ladder(label, …)`), but the
  DEBUG-per-attempt detail is the trap: v5 logs `SQLCipher key set…` once BEFORE the open;
  v4 now logs it inside each attempt — on a garbage file that is FOUR DEBUGs before the ERROR.
- The mount-index side needs no byte change; only the shared-helper shape is new.

---

### A.179 — the job-child outfit overlay

#### v4 at HEAD (file:line facts)

- `lib/background-jobs/child/child-repositories-proxy.ts`: `JobScope.equippedOutfits:
  Map<string, EquippedSlots>` (`:58-65`), created empty per `runWithJobScope` (`:77`);
  `equippedOutfitKey(chatId, characterId)` = `` `${chatId}:${characterId}` `` when both are
  strings, else `null` (`:174-178`); `recordBufferedOutfitWrite(scope, fqn, args)` (`:186-194`)
  — ONLY for `fqn === 'wardrobeWear.commitEquippedOutfit'`, reads `args[0].{chatId,
  characterId, nextSlots}`, stores `cloneEquippedSlots(nextSlots)` (`lib/schemas/
  wardrobe.types.ts:286-288`, pre-existing), DEBUG `Recorded buffered outfit write for in-job
  reads {jobId, key}`; `bufferedOutfitFor(scope, args)` (`:197-202`) answers a CLONE for
  `(args[0], args[1])` or `undefined`. Wired at `:481-484` — the read branch of `wrapRepo`
  short-circuits `chats.getEquippedOutfitForCharacter` to `Promise.resolve(buffered)` BEFORE
  the `Read after buffered write` warning check — and at `:515-516` after `appendWrite` on the
  write branch. The overlay dies with the scope (per job; separate jobs start from the
  snapshot). `METHOD_OVERRIDES` comment `:245-250` updated.
- `docs/developer/BACKGROUND_JOBS_CHILD.md:66` (+2): one paragraph naming the overlay as the
  ONE table with read-your-writes; "the parent still replays every commit against the true
  prior state, so the wear ledger is unaffected".
- `__tests__/unit/lib/background-jobs/child-proxy-wardrobe-wear.test.ts:94-189` (+100): two
  garments in one job → both in the replayed `top` and the snapshot read ONCE; a take-off
  after a put-on sees the put-on; characters/chats/jobs kept apart; results are copies.
- `docs/developer/features/complete/wardrobe-wear-ledger.md` (+6): cross-reference — "neither
  of the two fixes above" (the ledger spec had proposed threading slots through the op loop
  or collapsing to one commit).

#### What the commit did NOT do (179)

- Did not change `lib/wardrobe/outfit-displacement.ts` (the primitives still `loadSlots` per
  op), the wear-handler's op loop, `commitEquippedOutfit`'s whole-slot replay, or the parent's
  diff. The in-process (request-path) code path is untouched: outside a job scope the proxy is
  not in play and every read hits the real repository.

#### v5 main (file:line facts) — the in-process measurement

- v5 has NO child, NO buffered-write proxy, NO overlay to port
  (`crates/quilltap-core/src/services/job_runner.rs:7-24` — "No child, no buffer, no
  reflection"; the job-level write buffer "deliberately dropped").
- The wardrobe tools run INSIDE ONE `Db::write` closure on the WRITER connections:
  `tools/executor.rs:1794-1807` `run_wardrobe_wear` → `wardrobe_write(db, |main, mount| …)`
  (`:2077-2083`, `.write(move |writers| …)`), calling `tools/wardrobe_wear.rs:134` `execute(main:
  &Connection, mount, …)`; each primitive (`tools/wardrobe_shared.rs:318-343` `equip_item`,
  `:345` `replace_item`, `:371` `add_to_slot`, `:399` `remove_from_slot`) RMWs through
  `load_current_wardrobe_state` (`:185-193`, `ChatOutfitsRepository::
  get_equipped_outfit_for_character` on the same writer connection) and `persist` (`:415-427`,
  `set_equipped_outfit`). **So a second op in one `wardrobe_wear` call — and a second
  `wardrobe_wear` call in one autonomous turn — reads the first's committed row. Two outfit
  changes compound correctly in v5 today.** v4's bug cannot arise here.
- There is NO wear ledger in v5 (`wardrobe_wear_stats` / `commit_equipped_outfit`: zero hits
  under `crates/`) — the ledger is `3ee3b1342`, unprocessed wardrobe drift.

#### Harness families + recipes (179)

- None apply; no v5 twin of `child-proxy-wardrobe-wear.test.ts`. A v5 pin that two ops in one
  `wardrobe_wear` call compound (wardrobe_wear.rs's `run` loop) would document the
  no-port — check `tools/wardrobe_wear.rs` tests / `wardrobe_tier3` families before adding one.

#### Premise corrections / surprises (179)

- Bug 179 is NO-PORT by architecture (the unbuffered in-process runtime is the ruled
  divergence from P3). The only v5 deliverable is a status-log line + the drift ledger row;
  the `BACKGROUND_JOBS_CHILD.md` mirror re-vendor rides the docs vendor pass (§D).

---

## B. `f5e953a3f` — daily database optimize before migrations

### v4 at HEAD (file:line facts)

`lib/startup/daily-db-optimize.ts` (NEW, 305 lines; `module: 'startup:daily-db-optimize'`):

- Targets `OPTIMIZE_TARGET_KEYS = ['main', 'llm-logs', 'mount-points']` (`:31-33`); state file
  **`data/db-optimize-state.json`** (`OPTIMIZE_STATE_FILENAME` `:36`, `path.join(getDataDir(),
  …)` `:55-57`); shape `Partial<Record<key, 'YYYY-MM-DD'>>` written as
  `JSON.stringify(state, null, 2) + '\n'` (`:81`); read drops non-string values and unknown
  keys, a missing/corrupt/array file reads `{}` with WARN **`Could not read database optimize
  state; treating every database as due`** `{statePath, error}` (`:60-77`); write failure WARN
  **`Could not write database optimize state; optimize will repeat on next launch`** (`:83`).
- Day gate: `localDateStamp(now)` = `getFullYear()-MM-DD` in the **process's local zone**
  (`:48-53`; the test pins `new Date(2026,0,5,23,59)` → `2026-01-05`); `isOptimizeDue(state,
  key, today) = state[key] !== today` (`:90-92`). Calendar, not clock (help text).
- `optimizeDatabase(db, label)` (`:98-122`): `run('VACUUM', db.exec)` && `run('ANALYZE',
  db.exec)` && `run('PRAGMA optimize', db.pragma('optimize'))` — **stop at first failure**
  (short-circuit `&&`); per step DEBUG **`Optimize step complete`** `{database, step, ms}` or
  ERROR **`Optimize step failed`** `{database, step, ms, error}`; returns `{ok, steps:
  [{name, ok, ms, error?}]}`.
- `runDailyDbOptimize(now)` (`:147-305`): due filter; nothing due → DEBUG **`Databases already
  optimized today; skipping`** `{today, state}` and return (`:153-156`); dynamic imports of
  `@/migrations/lib/database-utils` (`isSQLiteBackend`, `getSQLiteDatabase`, `getSQLitePath`,
  `getLlmLogsDbPath`, `openLlmLogsDbIfPresent`, `openMountIndexDbIfPresent`) and
  `@/lib/database/backends/sqlite/physical-backup` (`createPhysicalBackup`,
  `createLLMLogsPhysicalBackup`, `createMountIndexPhysicalBackup`); `!isSQLiteBackend()` →
  DEBUG `Not a SQLite backend; nothing to optimize` (dead — `detectDatabaseBackend()` returns
  `'sqlite'`, `database-utils.ts:16`).
- Specs (`:178-205`): `main` opens via **`getSQLiteDatabase()`** — the MIGRATION layer's
  cached, lock-holding handle (`database-utils.ts:92-116`: acquires the instance lock, then
  `openEncryptedSqlite(path, {foreignKeys:true})`), `owned:false`; `llm-logs` /
  `mount-points` via `openLlmLogsDbIfPresent()` / `openMountIndexDbIfPresent()`
  (`database-utils.ts:171-209`: `null` when the file does not exist, else
  `openEncryptedSqlite`), `owned:true` (closed in `finally`, `:279-288`, WARN `Error closing
  database after optimize`). **`openEncryptedSqlite` (`database-utils.ts:138-166`) sets
  `journal_mode = WAL`** on all three — the migration-layer family, NOT the runtime clients'
  TRUNCATE — plus key, `registerTextCodecFunction`, `foreign_keys` (main only), `busy_timeout
  5000`.
- Startup progress: `startupProgress.setCurrent('subsystem:db-optimize:start')` (`:208`),
  INFO **`Daily database optimize starting`** `{today, databases:[labels]}`;
  `setSubProgress([{current:i+1, total, unit:'databases'}])` per DB (`:220`); absent file →
  DEBUG **`Database file not present; nothing to optimize`** `{database}` and **stamped
  today** (`:225-229`); pre-backup `await spec.backup(db)` → DEBUG **`Pre-optimize backup
  step done`** `{database, backupPath: path | '(skipped — a recent backup exists, or the
  backup failed; see above)'}` (`:232-236`), a THROW → WARN **`Pre-optimize backup threw;
  optimizing anyway (VACUUM is transactional)`** (`:238`); sizes via `fs.statSync` (0 on
  error, `:124-130`); after the steps, **main only** (`!spec.owned`) `db.pragma('wal_
  checkpoint(TRUNCATE)')` with WARN **`Post-optimize WAL checkpoint failed`** (`:247-258`);
  `ok` → stamped + `optimized++` + `totalReclaimed += max(0, before-after)` (`:261-265`);
  INFO **`Database optimize finished`** `{database, ok, sizeBefore, sizeAfter, steps}` either
  way (`:266-272`); the outer `catch` ERROR **`Database optimize failed; will retry on next
  launch`** `{database, error}` (`:274-277`) — reached only by an open/size throw, since the
  backup has its own catch and the steps return. `writeOptimizeState` once at the end
  (`:292`); INFO **`Daily database optimize complete`** `{optimized, attempted,
  reclaimedBytes, elapsedMs}` (`:295-300`); `startupProgress.publish({ rawLabel:
  'subsystem:db-optimize:complete', detail: `${optimized} of ${work.length} databases
  optimized in ${(elapsedMs/1000).toFixed(1)} s` })` (`:301-304`).
- `instrumentation.ts:404-418` **PHASE 0.75** — AFTER PHASE 0.5 (the version guard, `:379-
  402`) and BEFORE PHASE 1 (migrations, `:420`); `try { await runDailyDbOptimize() } catch`
  → ERROR **`Daily database optimize failed — continuing startup`** `{context:
  'instrumentation.register', error}`. Nothing before PHASE 2/3 has opened the RUNTIME clients
  yet; the backend's own PHASE-2 startup backup (`backend.ts:563-569`,
  `createPhysicalBackup(db).then(applyRetentionPolicy)`, async) then finds the fresh file and
  skips.
- `lib/startup/prettify.ts:29-30`: `'subsystem:db-optimize:start'` → **`Giving the ledgers
  their morning dusting`**; `'subsystem:db-optimize:complete'` → **`Ledgers dusted and squared
  away`** (between `unlocking` and `migrations:start`).
- `help/database-protection.md:130` (+2): a paragraph under `db optimize` — "You will seldom
  need to… once a day, as it starts up (see *Daily Tidying on Startup* below)"; `:247-254`
  (+8) the new **`### Daily Tidying on Startup`** section (calendar not clock; the state file;
  delete it to re-tidy; seconds added to the first launch; cloud-sync re-upload note).
- The physical-backup helper it calls — `lib/database/backends/sqlite/physical-backup.ts`
  (PRE-EXISTING, untouched): `BACKUP_INTERVAL_MS = 24 h` (`:169`); `shouldCreateBackup`
  compares `Date.now()` with the NEWEST filename matching the per-DB regex (`:175-218`; DEBUGs
  `No existing ${label} backups found, backup needed` / `Recent ${label} backup exists,
  skipping {lastBackup, ageHours}` / `Last ${label} backup is old enough, backup needed`);
  dir `getBackupsDir()` = `data/backups` (`lib/paths.ts:337-339`); names
  `quilltap-YYYY-MM-DDTHHmmss.db`, `quilltap-llm-logs-…`, `quilltap-mount-index-…` (`:31-37`,
  local time, `:46-61`); `createPhysicalBackup(db)` (`:239-297`): INFO `Starting physical
  database backup {destination}` → **`VACUUM INTO '<path>'`** → INFO `Startup physical backup
  created {path, sizeBytes}` → returns the path; failure ERROR `Physical database backup
  failed` + partial-file cleanup → `null`. Retention (`applyRetentionPolicy` `:440+`: 7 days
  all / 4 weekly / 12 monthly / yearly) is **NOT called by the daily pass**.
- The CLI twin `packages/quilltap/lib/db-commands.js:1426-1480` `optimizeOneDb`: its OWN
  connection (`opener(dataDir, pepper, {readonly:false})`), the same three steps via
  `runStep` (prints, throws on failure → returns `skipped:false` with the steps), closes in
  `finally`, prints `size after … (reclaimed X | grew by X | no change)`. Shared by comment
  only ("Keep the two step lists in step", `daily-db-optimize.ts:7-8`).
- Jest `lib/startup/__tests__/daily-db-optimize.test.ts` (89 lines): `localDateStamp`; missing
  / `{not json` / `[1,2]` → `{}`; round-trip ignoring `bogus`; `isOptimizeDue`; the three-step
  order on a fake `db`; stop-at-first-failure (`pragma` never called). **No test runs
  `runDailyDbOptimize`.**

### What the commit did NOT do

- Did not call `applyRetentionPolicy`; did not remove or reorder the PHASE-2 startup backup;
  did not change `physical-backup.ts`, `database-utils.ts` (the WAL-mode openers), or the CLI.
- Did not add an integrity check, did not touch `startupState`, did not make the pass async
  w.r.t. boot (it is awaited, so the day's first launch is slower by the VACUUMs).
- Did not vendor the labels into the SPA beyond `prettify.ts`.

### v5 main (file:line facts)

- Boot order (no version guard, no migration runner — `CLAUDE.md:113`): the engine's
  `open_ready` (`crates/quilltap-core/src/api/engine.rs:7127-7160`): `assembler.pre_open`
  (`:7136`, the instance lock — `quilltap-host/src/host.rs:532-549`) → `Db::open(paths,
  pepper)` (`:7151`, the three writable opens; `db/runtime.rs:408-465`, LLM logs first) →
  `assembler.assemble` (`:7154`) = `host.rs:549-600`: `seed_built_ins(db)` (`:563` — the
  boot ensures, v4's migration twins, on a fresh OS thread via `write_blocking`) → the
  structural pass (`:579-580`, PHASE 3.1) → `reconcile_help_docs_at_boot` (`:593`, 3.66) →
  `reconcile_embedding_dimensions_at_boot` (`:597`, 3.7) → pumps. **The PHASE-0.75 slot is
  between `Db::open` and `seed_built_ins`** — i.e. the head of `assemble` (host) or between
  `:7151` and `:7154` (engine). Cadence ownership says host.
- Writer API: `db/runtime.rs:487` `Db::write(|ws: &mut WriterSet| …)` (async), `:508`
  `write_blocking`; `WriterSet::main()/mount_index()/llm_logs()` (`:139-156`);
  `Writer::connection()` (`db/mod.rs:297`). **No transaction wrapper** in `runtime.rs` /
  `mod.rs` (no `BEGIN`/`transaction(` hits) — a closure is autocommit, so `VACUUM` can run
  there; read-pool connections are idle between reads.
- `journal_mode`: `db/mod.rs:259-273` — the writable open sets **`TRUNCATE`** (CLAUDE.md's
  cloud-sync rule). v4's optimize handles are WAL (above), so v4's `wal_checkpoint(TRUNCATE)`
  is meaningful there and a no-op here (returns `0|0|0`-ish; harmless).
- VACUUM/ANALYZE/optimize in v5 today: only `services/embedding_reapply_profile.rs:223-247`
  (`VACUUM INTO` backup + post-write `VACUUM`, non-fatal) and the `run_sql` deny-list
  (`tools/run_sql.rs:52`); `services/collapse_stale_chat_caches.rs:43` calls `db optimize`
  "an unported CLI surface". No `ANALYZE`, no `PRAGMA optimize`, no `wal_checkpoint` anywhere.
- **No startup physical backup in v5 at all**: no `createPhysicalBackup` twin, no 24-h gate,
  no retention; `data/backups` is only READ by the Almanack (`almanack/phase1_premises.rs:
  47-48, 207-223`; `host/almanack_services.rs:240`). Porting B drags in the backup trio (or a
  deliberate carve).
- CLI: `crates/quilltap-cli/src/db_cmd.rs:25-39` `DB_VERBS` lists `optimize`, `backup`,
  `integrity`; `:291` every one of them exits `recognized but not yet available`; help text
  already advertises them (`src/help/db_help.txt:46,101-102`; completions). The Tier R
  oracle is v4's `db-commands.js`.
- Startup progress: v5 has NO progress-label stream — `crates/quilltap-web/src/lib.rs:708`
  `boot_startup_status` yields `Running | LockConflict | <failure>` only; zero hits for
  `rawLabel` / `prettyLabel` / `subsystem:` in `crates/` and `apps/web/src`. The two pretty
  labels have nowhere to land until a startup-progress surface exists.
- Local zone: P4.140 threads the host's display-zone VALUE (`crates/quilltap-core/src/
  host_zone.rs`) as a required spine argument; `localDateStamp`'s stamp must use that same
  source, not UTC.
- Data dir: `base_dir.join("data")` (`quilltap-host/src/lock.rs:740`,
  `almanack_services.rs:75`).

### Harness families + recipes (B)

- None exist for the optimize or the physical backup. Candidates: a tier-2 real-DB family over
  `runDailyDbOptimize` (state file bytes, the three `module=startup:daily-db-optimize` lines
  per DB, the `data/backups/` filename shape, `PRAGMA page_count` before/after on a planted
  DB with deleted rows) with v4 run through `migrations/lib/database-utils` on a copy; a
  tier-1 over `localDateStamp` / `readOptimizeState` / `isOptimizeDue`; the boot-hardness host
  test for the no-fatal path (a read-only `data/`; a corrupt state file). The sibling opens
  inside v4's pass are the WAL-mode migration openers — any oracle must open the same way or
  the `journal_mode` header write will show in the file bytes.

### Premise corrections / surprises (B)

- "The physical backup helper" the task guessed at is `lib/database/backends/sqlite/
  physical-backup.ts`, and v5 has never ported it — v4's own PHASE-2 startup backup
  (`backend.ts:563`) is an unported surface too. B is therefore not a 300-line port but the
  backup trio + gate + optimize + state file, plus the ruling on where in v5's boot the
  VACUUM runs (before the structural pass; the writer is already open, unlike v4).
- The optimize's connections are v4's MIGRATION family (WAL), not the runtime clients
  (TRUNCATE). On v4 the first boot of a day flips each file's journal header WAL → … →
  TRUNCATE twice; v5 need not reproduce that, but the oracle's byte comparands must not
  include the header.
- The daily pass does NOT run retention; the PHASE-2 backup does. A v5 port that only ports
  the daily pass leaves `data/backups/` growing without bound.

---

## C. The dependency tree (`a9c99a4a0` + `b3f937076`'s eight bundle rebuilds)

### v4 at HEAD (file:line facts)

Root `package.json` (`:3` `4.10.0-dev.112` → `dev.117`; `:172` the `@openrouter/sdk@1.3.28`
trusted-dependency entry → `@openrouter/sdk@1.4.25`); direct deps' RANGES unchanged (`"openai":
"^7.23.0"` `:95`, `"mammoth": "^1.12.3"` `:91`, `"next": "^16.3.6"` `:92`, `"sharp": "^0.35.4"`
`:112`, `"pdf-parse": "^2.4.5"` `:96`, `"@openrouter/sdk": "^1.3.28"` `:122`). Root
`package-lock.json`: 99 entries moved; the ones that matter —

| package | `94fbb1ae3` | `f5e953a3f` |
|---|---|---|
| `openai` | 7.23.0 | **7.30.0** |
| `@openrouter/sdk` | 1.3.28 | **1.4.25** |
| `@anthropic-ai/sdk` | — (plugin-only) | UNMOVED 0.115.0 |
| `@google/genai` | — (plugin-only) | UNMOVED 1.52.0 |
| `mammoth` | 1.12.3 | 1.13.0 |
| `sharp` (+ `@img/sharp-*`, libvips 1.3.3→1.3.4) | 0.35.4 | 0.35.5 |
| `next` (+ `@next/*`, `eslint-config-next`) | 16.3.6 | 16.4.0 |
| `micromark` | 4.0.2 | 4.0.3 |
| `micromark-core-commonmark` | 2.0.3 | 2.0.4 |
| `micromark-factory-space` | 2.0.1 | 2.1.0 |
| `micromark-util-types` | 2.0.2 | 2.0.3 |
| `micromark-util-edit-map` | absent | NEW 1.0.0 |
| `mdast-util-from-markdown` | 2.0.3 | 2.1.0 |
| `mdast-util-to-markdown` | 2.1.2 | 2.2.0 |
| `mdast-util-find-and-replace` | 3.0.2 | 3.0.3 |
| `mdast-util-gfm-strikethrough` | 2.0.0 | 2.0.1 |
| `pdf-parse` | 2.4.5 | UNMOVED |
| `better-sqlite3-multiple-ciphers` | 12.11.1 | UNMOVED |
| `@quilltap/plugin-utils` | 2.6.2 | 2.6.3 |
| `katex` | 0.18.9 | 0.18.10 |
| `@types/node` | 24.13.6 | 24.19.1 |
| `typescript-eslint` family | 8.70.1 | 8.71.1 |
| `ws` | 8.21.3 | 8.22.0 |

`packages/quilltap/package-lock.json`: UNCHANGED (only `packages/quilltap/package.json:3`
version bumped). `plugins/dist/*/package.json`: UNCHANGED (e.g. `qtap-plugin-openrouter/
package.json` still `1.0.66`, `"@openrouter/sdk": "^1.3.28"`). `packages/plugin-utils/
package.json` already `2.6.3` at the baseline (the lock caught up).

The eight rebuilt bundles (`b3f937076`: anthropic, curl, default-system-prompts, google, mcp,
ollama, openrouter, search-serper; ~2,530 lines each, 14,548+/5,740−): the diff is the
**`openai` SDK code inside each bundle moving `var VERSION = "7.23.0"` → `"7.30.0"`** (new
`/skills/*` resource methods, 556 `resolveResourceRequestOptions` lines) — `@quilltap/plugin-
utils` bundles `openai` into every plugin that uses it, provider or not. The six OpenAI-SDK
PROVIDER plugins (`openai`, `openai-compatible`, `deepseek`, `grok`, `nanogpt`, `z-ai`) were
**NOT rebuilt** — their bundles still read `VERSION = "7.23.0"` and their
`plugins/dist/*/node_modules/openai` is 7.23.0. The openrouter bundle still embeds
`sdkVersion: "1.3.28"`, `genVersion: "2.914.0"` (its `node_modules/@openrouter/sdk` is 1.3.28).
Installed at the LIVE checkout: root `node_modules/openai` 7.30.0, root `@openrouter/sdk`
1.4.25; every plugin install unmoved (`@anthropic-ai/sdk` 0.115.0, `@google/genai` 1.52.0).

`finish_reason` in v4 is read ONLY at `lib/llm/extract-finish-reason.ts:17` (`choices[0]
.finish_reason`) — plus the plugins' stream decoders, whose sources are not in this repo
(`plugins/` holds `dist/` + guides only). `lib/llm/moderation-finish-reason.ts:62`,
`lib/services/dangerous-content/refusal.ts:166`, `lib/services/chat-message/route-trail.ts:155`
only FORMAT the string. The `openai` 7.30.0 changelog claim was NOT checked (no install, no
fetch).

Users of the moved markdown/docx packages in v4: `lib/mount-index/converters/docx-converter.ts`
(mammoth) and `lib/services/markdown-renderer.service.ts` (unified/remark → mdast/micromark);
the rest are React components.

### What the span did NOT do (C)

- No direct-dependency RANGE moved; `a9c99a4a0` is a lock refresh. `zod`, `better-sqlite3-
  multiple-ciphers`, `pdf-parse` unmoved. The six OpenAI provider bundles and every plugin
  `node_modules` unmoved — the provider wire the oracle records through those plugins is
  still built by `openai` 7.23.0.

### v5 main (file:line facts)

- `crates/quilltap-harness/tests/provider_sdk_version_guard.rs:66-80` constants:
  `RECORDED_OPENAI_SDK = "7.23.0"`, `RECORDED_ANTHROPIC_SDK = "0.115.0"`,
  `RECORDED_GOOGLE_GENAI_SDK = "1.52.0"`, `RECORDED_OPENROUTER_SDK = "1.3.28"`,
  `RECORDED_NODE = "v24.13.1"`. Test 1 (`:119-196`) reads `package.json` under
  `<checkout>/node_modules` AND every `plugins/dist/*/node_modules` (`:103-117`), comparing
  each present package: **at the live checkout it now FAILS** (root `openai` 7.30.0 ≠ 7.23.0;
  root `@openrouter/sdk` 1.4.25 ≠ 1.3.28), while the plugin locations still match. At the
  pinned worktree (its own `npm ci --offline` at `94fbb1ae3`) it is green. Test 2 (`:299-377`)
  checks the `x-stainless-package-version` / OpenRouter UA stamps in
  `request-envelopes/request-envelopes.recorded.ndjson`, `image-dialects/
  image-dialects.recorded.ndjson`, `request-envelopes/google-wire.recorded.ndjson`; test 3
  (`:380+`) the Node runtime stamps. The locator is `common::v4_root` (`QT_V4_CHECKOUT`).
- Decoders: `crates/quilltap-core/src/model/decoders/chat_completions_sse.rs:132-137` (the
  `finish_reason` field + `stream_finish_reason` = the LAST non-empty string
  `choices[0].finish_reason`, v4 OpenRouter's rule `:297-298`), `:293-300` (capture), `:412,
  :442, :480` (the assembled `done` object), `:465` (the finish); `decoders/
  responses_api_sse.rs`; `finish_reason.rs:1-40` (`extract_finish_reason`, v4's probe order);
  `moderation_finish_reason.rs`. The port consumes wire JSON, never the SDK — an SDK bump
  touches v5 only through (a) request-header stamps and (b) any request-shape change the SDK
  makes, both visible only by re-recording.
- Fixtures: `harness/oracle/fixtures/request-envelopes/{request-envelopes,google-request,
  google-wire}.recorded.ndjson`; `fixtures/image-dialects/image-dialects.recorded.ndjson`;
  `fixtures/streams/{anthropic_sse,chat_completions_sse,google_parts,ollama_ndjson,
  responses_api_sse}/`; `fixtures/primary-stream-tier3.json` (built by
  `fixtures/build-primary-stream-fixture.ts`, header `:12-18`: Node 24, `npx tsx` from the v4
  checkout). Cases: `harness/oracle/cases/finish-reason.ts` (`:7-9`, `npx tsx` from the server
  checkout), `moderation-finish-reason.ts` (`:12-14`), `primary-stream-tier3.test.ts`,
  `openrouter-sdk-pricing.test.ts`, `openai-chaining-fallback-tier3.test.ts` (jest, staged).
- docx: v5 has NO docx extractor — `generators/file_content.rs:19` "the pdf/docx extractor is
  deferred by P4.6y — the refusal prints"; `file_content_extractor_equivalence.rs` covers the
  PDF arm through the `DocumentTextExtractor` seam (P4.D251). The `mammoth` move is inert for
  v5 until docx is ported. markdown: v4's `markdown-renderer.service.ts` is oracled by
  `conversation_markdown_equivalence.rs` (and cited by `ai_import_tier3_equivalence.rs`,
  `character_wizard_tier3_equivalence.rs`, `orchestrator_tier3_equivalence.rs`,
  `file_content_extractor_equivalence.rs`); `markdown_transcript_equivalence.rs` /
  `markdown_frontmatter_equivalence.rs` are the other markdown families. A `mdast-util-to-
  markdown` 2.1.2 → 2.2.0 move can change rendered bytes — those families are the tripwire.

### Harness families + recipes (C)

- `provider_sdk_version_guard` (no recipe stage; `cargo test -p quilltap-harness --test
  provider_sdk_version_guard`) — red at the live root today. The recorders for the three
  corpora are the per-family headers (`request_envelopes_*`, `image_dialects_*`,
  `google_wire_*` — grep `request-envelopes.recorded` in `crates/quilltap-harness/tests/`).
- The markdown families above; `sync_engine_equivalence.rs:827` only MENTIONS docx.

### Premise corrections / surprises (C)

- The "eight plugin rebuilds" are the eight plugins that do NOT use `openai` as a provider;
  they now carry `openai` 7.30.0 code via `plugin-utils`, while the six OpenAI-SDK providers
  still ship 7.23.0 bundles. Whether a re-record moves the `x-stainless-package-version`
  stamps therefore depends on which bundle builds each request — the OpenAI provider rows
  should still stamp 7.23.0; only `plugin-utils`-originated calls (if any reach the wire)
  would stamp 7.30.0. Measure before bumping `RECORDED_OPENAI_SDK`.
- `@openrouter/sdk` moved at the ROOT only; the openrouter PLUGIN (the oracle's path) is
  still 1.3.28, so the recorded UA `1.3.28 2.914.0 1.0.0` should be stable at a re-record.
- The guard's test 1 fails on the live checkout for a reason unrelated to the plugins — a
  SKIP-vs-FAIL design question for the lane (the pinned worktree is what the sweep uses).

---

## D. Vendor inventory (help / docs / schemas)

### v4 at HEAD (file:line facts)

`git diff --stat 94fbb1ae3 f5e953a3f -- help/ docs/ public/schemas/`: 21 files, 2,395+/19−.

help (8; the tree is **130 files at HEAD, 129 at the baseline** — ONE new file):
- `A help/wardrobe-images.md` (+57) — wardrobe item images (`7c8572869`/`b3f937076`).
- `M help/wardrobe.md` (+62 −), `M help/project-wardrobe.md` (+4), `M help/profile-avatar.md`
  (+4), `M help/chat-gallery.md` (±4), `M help/image-generation-profiles.md` (±6) — wardrobe /
  gallery drift.
- `M help/system-backup-restore.md` (+11, bug 181, `:213-222`) — THIS round.
- `M help/database-protection.md` (+10, optimize, `:130` and `:247-254`) — THIS round.

docs (12): `M docs/CHANGELOG.md` (+190), `M docs/developer/API.md` (+61), `M docs/developer/
DDL.md` (+65, the wear-ledger tables), `M docs/developer/BACKGROUND_JOBS_CHILD.md` (+4, the
bug-179 overlay paragraph), `M docs/developer/bugs.md` (±7), `A docs/developer/bugs/fixed/
bug-179-buffered-outfit-overwrite.md`, `A …/bug-180-llm-logs-no-cold-open-retry.md`,
`A …/bug-181-restore-drops-embedded-memories.md`, `A docs/developer/features/complete/
wardrobe-item-images.md` (412), `A …/wardrobe-list-legibility.md` (324),
`A …/wardrobe-wear-ledger.md` (482), `A docs/releases/4.10.0.md` (404).

schemas (1): `M public/schemas/qtap-export.schema.json` (+55 −): `wardrobeWear` arrays on the
character and project export shapes + a `wardrobeWear` count, the `WardrobeWear` `$def`, and
the wardrobe item's `imageFileId` / `_imageFiles` — ALL wardrobe drift, nothing from this
round's three commits.

### What the span did NOT do (D)

- No help file was deleted or renamed; no `docs/v4`-relevant developer doc beyond the five
  above; `help/system-backup-restore.md` gained a paragraph, not a section.

### v5 main (file:line facts)

- The vendored tree is `<repo>/help/` (129 files, **byte-identical to the baseline for all
  129**; `help/wardrobe-images.md` absent), embedded at build time by
  `crates/quilltap-host/build.rs` into `help_content::EMBEDDED_HELP`
  (`crates/quilltap-host/src/help_content.rs:1-23`, `embedded_help_count()` `:23`), read
  through `files_store::embedded_help_source_files()` (`files_store.rs:360-368`) by the boot
  reconcile (`host.rs:1135-1140`) and the reindex handler (`host.rs:665`). The core's
  `tools/help.rs` does not reference `help/` paths (the grep in the task finds nothing there).
- Guards that count: `crates/quilltap-harness/tests/help_tree_embed_guard.rs:76`
  `VENDORED_FILE_COUNT: usize = 129` (embedded table == on-disk tree, both 129, `:86-99`);
  `crates/quilltap-host/tests/host_help_docs_boot.rs:105` `assert_eq!(expected, 129, …)` (+
  the module doc `:7-17`); `crates/quilltap-harness/tests/help_tree_equivalence.rs:122-126`
  compares `files.len()` with the ORACLE's synced count (moves with the oracle; recipe
  `:18-32` — `help-tree-sync.test.ts` staged, `QT_ORACLE_HELP_TREE`). **A whole-tree
  re-vendor at `f5e953a3f` makes 130, not 132** — the task's premise of three new help files
  is wrong; the three NEW files in the span are `docs/developer/features/complete/*.md`.
- `docs/v4/` mirror: `CHANGELOG.md`, `developer/API.md`, `developer/DDL.md`,
  `developer/bugs.md`, `developer/BACKGROUND_JOBS_CHILD.md` are all `== 94fbb1ae3`;
  `developer/features/complete/` (96 files) has `Wardrobe UX Overhaul.md`,
  `archived-scenarios-and-wardrobe.md`, `wardrobe-hair-slot.md` but none of the three new
  wardrobe docs; `developer/bugs/fixed/` (178 files) ends at `bug-178-pdf-domatrix-build-
  path.md` — no 179/180/181; `releases/` ends at `4.9.2.md` — no `4.10.0.md`.
- Schema mirror: `crates/quilltap-core/src/generators/qtap-export.schema.json` is byte-equal
  to the baseline's `public/schemas/qtap-export.schema.json` (0 differing lines) and differs
  from HEAD by exactly the wardrobe additions.

### Harness families + recipes (D)

- `help_tree_embed_guard` (no oracle), `host_help_docs_boot` (host, hermetic),
  `help_tree_equivalence` (oracle, recipe `:18-32`), plus the help families that read specific
  pages (`help_doc_ensure_equivalence`, `help_doc_sync_equivalence`, `help_snippet_
  equivalence`, …) — re-vendoring the two THIS-round pages (`system-backup-restore`,
  `database-protection`) moves no count; the wardrobe pages + the new file move it to 130
  and belong to the wardrobe round.

### Premise corrections / surprises (D)

- 130, not 132. Only `wardrobe-images.md` is new.
- Of the eight help moves only TWO belong to this round's commits; vendoring them alone keeps
  the count at 129 and the three guards green. Vendoring the tree WHOLE at `f5e953a3f` pulls
  the wardrobe pages in ahead of their port — the standing rule (vendor at the baseline the
  code is ported to) says split them.
- The export schema mirror is untouched by this round; it moves with the wardrobe port.

---

## Cross-cutting notes for the order writer

1. **A.181 retires a ruled divergence by convergence**: `INDEX_KEYED_EMBEDDING`'s v4-refuses
   arm trips at the new pin by design. The lane must drop the carve and port the two genuine
   deltas (the backup's `number[]` + explicit `null`; the written-rows summary count), then
   rule the `{}` edge.
2. **A.180 is a byte-pin round**: the ladder shape exists (`open_mount_index`); the work is
   the shared helper, the per-attempt DEBUG, `attempts` on both LLM-logs terminal lines, the
   three host pins, the `degraded_sibling_open_equivalence` regen, and the stale citations.
3. **A.179 is NO-PORT** (status-log + ledger row); v5 already compounds.
4. **B is bigger than its line count**: physical backup trio + 24-h gate (never ported), the
   state file, the three-step optimize on the writer, no progress-label surface to land the
   two pretty labels, and the TRUNCATE-vs-WAL journal difference to keep out of the comparands.
   The CLI `db optimize`/`backup` verbs (`db_cmd.rs:291`) can follow the same core unit.
5. **C needs a measurement before any `RECORDED_*` bump**: which bundle stamps which request.
6. **D**: vendor the two this-round help pages + `BACKGROUND_JOBS_CHILD.md` + `bugs.md` + the
   three `bugs/fixed/` files now; leave the wardrobe pages, the three feature docs, the schema
   and `4.10.0.md` to the wardrobe round.
