# Survey — P4.150: host boot + model-layer + generators smalls

Surveyed 2026-10-05, read-only. v5 `main` `6c3a3c635`; v4 `07b8f0209` (the
oracle baseline, tree clean). No cargo / npm / jest / tsx run; every count
below is from `ggrep` / `sed -n` / `python3` over committed JSON.

Item letters follow the brief (A–E). Each item: source quote → v5 today →
v4 at `07b8f0209` → divergence → predicted hunks → proof → fixtures → risk.

The thread-scoped capture idiom every log-line pin below copies is
`quilltap_core::test_support::captured_with` (`crates/quilltap-core/src/
test_support.rs:123`; `CaptureLayer` at `:90`), used e.g. by
`model/plugin_catch_log.rs:674` `google_no_candidates_warn`. Boot-path lines
(writer/seed threads) cannot use the thread-scoped rig; they use the
process-GLOBAL `CaptureLayer` + `SERIAL` mutex of
`crates/quilltap-host/tests/host_boot_hardness.rs:82-95` (`assert_line`
`:220`, `assert_silent` `:242`, `assert_lines_in_order` `:926`).

---

## A1. P4.D248 Tier 3 item 18 — `doc_mount_points`' four ALTER self-heals

**Source** (`work-orders/p4.d248-bugs-175-176-resumable-defer-structural-tables-health.md:364-366`):
> 18. **v4's `doc_mount_points` ALTER self-heals** (`totalSizeBytes`,
> `conversionStatus`, `conversionError`, `storeType`) — DEFERRED by name; a
> boot-repair candidate for a later round (the P4.D63 precedent).

Also its Unification paragraph (`:3`) "OPEN by name: … Tier 3 items 18 (…)".

**v5 today — NO self-heal exists.** `ggrep "ADD COLUMN"` over
`crates/quilltap-core/src` + `crates/quilltap-host/src` finds only
`linkGroupId` (`db/mount_index_case_repair.rs:513`). The mount-index boot path:

- `crates/quilltap-host/src/host.rs:1881-1885` calls
  `builtin_mounts::ensure_builtin_mounts_with(main, mount_index, LogAndContinue)`
  inside `seed_built_ins` (`:1248`).
- `crates/quilltap-core/src/services/builtin_mounts.rs:198-226`
  `ensure_mount_index_tables`: `CREATE TABLE IF NOT EXISTS "doc_mount_points"`
  (all 18 columns, migration DDL) — a no-op on an existing table.
- then four `lazy_ensure` sub-steps (`:239-269`): folders NOCASE index,
  `ensure_link_group_column` (`["doc_mount_file_links","doc_mount_documents"]`),
  links NOCASE index, and `repair_mount_point_name_collisions`
  (`&["doc_mount_points"]`, `:267-269`). `lazy_ensure` (`:169-190`) logs v4's
  `Failed to ensure <table> table in mount index database` once and records the
  failure for the PHASE 3.1 pass.
- The pass then reports a pre-ALTER table: `db/table_shape.rs` `verify_structural_tables`
  (`:511-590`) → ERROR `Structural table check failed; …` with problem
  `table doc_mount_points is missing column totalSizeBytes` → `/health` 503
  `degraded`.

**v4 at `07b8f0209`:** `lib/database/repositories/doc-mount-points.repository.ts:37-70`
`onTableEnsured(db)` — ONE `PRAGMA table_info(doc_mount_points)` read, then four
guarded ALTERs, then `repairMountPointNameCollisions(db)`:

| col | DDL (exact) | INFO line (exact, root `@/lib/logger`, NO meta) |
|---|---|---|
| totalSizeBytes | `ALTER TABLE "doc_mount_points" ADD COLUMN "totalSizeBytes" INTEGER NOT NULL DEFAULT 0` | `Migrated doc_mount_points: added totalSizeBytes column` |
| conversionStatus | `… ADD COLUMN "conversionStatus" TEXT NOT NULL DEFAULT 'idle'` | `Migrated doc_mount_points: added conversionStatus column` |
| conversionError | `… ADD COLUMN "conversionError" TEXT DEFAULT NULL` | `Migrated doc_mount_points: added conversionError column` |
| storeType | `… ADD COLUMN "storeType" TEXT NOT NULL DEFAULT 'documents'` | `Migrated doc_mount_points: added storeType column` |

(`"${this.collectionName}"` interpolates to `"doc_mount_points"`.)
**Reachability: REACHABLE, once per process.** `onTableEnsured` runs inside
`ensureTable` (`dedicated-db.repository.ts:138-156`), which since `e5c6bd0c0`
PHASE 3.1's `verifyStructure` (`:174-200`) calls on EVERY boot before the shape
check — so v4 heals the table and then reports it SOUND. An ALTER throw lands in
the same `try`: `Failed to ensure doc_mount_points table in mount index database
{error}` and rethrow (→ the pass's ensure form). No generated index names any of
the four columns (`fresh_schema.json`'s only `doc_mount_points` index is
`idx_doc_mount_points_createdAt`), so the CREATE INDEX step cannot fail first.

A SECOND, distinct v4 helper exists and is NOT this item:
`migrations/lib/mount-index-schema.ts:23-55` `alignDocMountPointsSchema` (14
addable columns, SILENT, no lines) called from three ledger-gated provisioning
migrations (`provision-general-mount.ts:98`, `provision-user-uploads-mount.ts:114`,
`provision-lantern-backgrounds-mount.ts:110`) + `alignDocMountFileLinksSchema`
(3 policy columns). v5's re-homed provisioning (`ensure_one_mount`) ports
neither. Every v4-booted instance has run those migrations once, so the
per-boot-reachable surface is the four above.

**Divergence:** a `doc_mount_points` lacking any of the four columns is healed
(and logged) by v4 at boot and reported damaged (`degraded` 503) by v5.

**Predicted hunks:**
1. NEW `crates/quilltap-core/src/db/doc_mount_points_column_repair.rs` (the
   `character_archive_repair.rs` / `chats_cycle_order_repair.rs` precedent) —
   `ensure_doc_mount_points_columns(db) -> Result<(), DbError>`: `table_exists`
   gate (absent → `Ok`), ONE `PRAGMA table_info` read, the four ALTERs in v4's
   order, each followed by `tracing::info!(target: "quilltap::db", "Migrated
   doc_mount_points: added <col> column")` (no fields). One `pub mod` line in
   `db/mod.rs` (shared file, one line — flag). Alternatively put the fn in
   `db/mount_index_case_repair.rs` next to `ensure_link_group_column` (avoids
   the `db/mod.rs` line).
2. `services/builtin_mounts.rs:267` — a NEW `lazy_ensure(failures,
   &["doc_mount_points"], &mut collected, || …ensure_doc_mount_points_columns(mount_index))`
   IMMEDIATELY BEFORE the `repair_mount_point_name_collisions` step (v4's
   `onTableEnsured` order: ALTERs, then the repair). v4's one `try` logs ONE
   ensure line for the pair; v5 logs per sub-step (the cadence divergence
   already recorded for the link-group pair, `:249-256`).
3. Blast radius: one caller (`ensure_builtin_mounts_with`); `Propagate` mode
   callers (tests / provisioning) also pick it up — grep
   `ensure_builtin_mounts\b` callers when ordering.

Decide also: the 14-column `alignDocMountPointsSchema` → record NO-PORT
(migration-ledger-gated, silent, and a superset that v4 instances have all run)
— a ruling candidate, see §Open questions.

**Proof:**
- **Two-sided, already staged:** `crates/quilltap-harness/tests/table_shape_equivalence.rs:289-299`
  carries the EXPECTED divergence `points-pre-alter` (v4 `&[]`, v5
  `table doc_mount_points is missing column totalSizeBytes`), driven by
  `harness/oracle/fixtures/table-shape-spec.json`'s plant `{"name":
  "points-pre-alter", "divergence": true, "mount": ["ALTER TABLE
  doc_mount_points DROP COLUMN totalSizeBytes"]}` through v4's REAL container
  (`harness/oracle/cases/table-shape.ts:259-278`). The fix retires that row
  red-first (v5 → `&[]`). The spec's `"divergence": true` flag is echoed into
  the NDJSON → the spec edit + a regen at the pin (`QT_ORACLE_TABLE_SHAPE`,
  recipe header `table_shape_equivalence.rs:7-40`); the Rust side's
  `assert_eq!(plants.len(), 13, …)` (`:313`) moves if plants are added. Grow
  three plants (one per remaining column) + one "all four dropped" (SQLite
  `DROP COLUMN` works for all four — none indexed, none PK).
- **Lines:** `table-shape.ts` routes v4's console to stderr (`:82-89`) and
  records no logger calls. Either (a) add a `Logger.prototype.info` spy to the
  plant rows (the P4.144 `fold_episode_tier3` precedent) and compare v5's
  captured lines in order — but `every_plant_reports_what_v4s_pass_reports`
  replays through `ensure_builtin_mounts_with` on the test thread, so the
  thread-scoped `captured_with` works there; or (b) transcribe the four bytes
  into a `host_boot_hardness` arm (plant `ALTER TABLE doc_mount_points DROP
  COLUMN …` ×4 on the `Fresh` substrate's MOUNT; assert the four INFO lines in
  v4's order, `assert_silent("Structural table check failed")`, a SECOND boot
  `assert_silent("Migrated doc_mount_points")`). (a) is the stronger proof;
  (b) is needed anyway for the boot-path order (heal BEFORE the pass).
- **Silence leg:** the existing `Fresh` boots must not log `Migrated
  doc_mount_points` (add an `assert_silent` to a healthy-boot arm).

**Fixtures:** `table-shape-spec.json` — readers: `table_shape_equivalence.rs`
and `harness/oracle/cases/table-shape.ts` ONLY (grepped). No committed DB pair
moves (`host_boot_hardness`'s plants are in-test SQL over `provision_fresh_instance`).

**Risk:** SQLite `ALTER … ADD COLUMN … NOT NULL DEFAULT` on a VIEW plant fails
(`chunk-view`-style plants exist for other tables; none for `doc_mount_points`
today — if one is added, the ensure-form line must name `doc_mount_points`).
`db/mod.rs` is shared by every lane that adds a module.

---

## A2. P4.D248 Tier 3 item 20 — the collapse success INFO's v4 bytes

**Source** (`p4.d248-*.md:369-371`):
> 20. **The success INFO's v4 bytes** (`Collapsed duplicate avatar rolls` +
> camelCase bag vs v5's `… into one image per configuration` + snake_case) —
> pre-existing, not `e5c6bd0c0`'s; for a smalls round.

**v5 today — v5 emits BOTH lines; the item's framing ("vs") is half-wrong.**
- Core `crates/quilltap-core/src/db/avatar_rolls_collapse_heal.rs:982-995`
  ALREADY logs v4's line: `tracing::info!(target: "quilltap::migration",
  context = LOG_CONTEXT /* "migration.collapse-duplicate-avatar-rolls", :94 */,
  configurations, rowsKeyed, victimsDeleted, protectedKept, albumCopiesKept,
  blobsDeleted, chatsChanged, charactersChanged, messagesChanged,
  "Collapsed duplicate avatar rolls")` — on the FULL path only (the
  `victims.is_empty()` early return `:873-890` logs no summary, as v4). It is
  emitted BEFORE `stamp()` (`:660`). **Missing: `durationMs`** (v4's last field).
- Host `crates/quilltap-host/src/host.rs:2031-2069` `log_collapse_ran` logs a
  v5-ONLY second line: `INFO quilltap::boot Collapsed duplicate avatar rolls
  into one image per configuration avatar_rows=… configurations=… rows_keyed=…
  victims_deleted=… protected_kept=… album_copies_kept=… blobs_deleted=…
  chats_changed=… characters_changed=… messages_changed=…` — on EVERY
  `CollapseOutcome::Ran`, INCLUDING the early-return exit v4 never summarizes.
  Called at `:1790` (`Ok`) and `:1823` (`Stamp` arm, first).
- Pinned as v5's own bytes: `host_boot_hardness.rs:1146-1149`
  `COLLAPSED_TWO_ROLLS` (doc says "deferred by name to a smalls round"), used
  in `a_failed_collapse_ledger_write_is_deferred` (`:1099-1125`,
  `assert_lines_in_order`) and the success arm; `assert_silent("Collapsed
  duplicate avatar rolls")` at `:1139` (substring — matches both lines).

**v4 at `07b8f0209`:** `migrations/scripts/collapse-duplicate-avatar-rolls-v1.ts:617-629`:
```
logger.info('Collapsed duplicate avatar rolls', {
  context: 'migration.collapse-duplicate-avatar-rolls',
  configurations: groups.size, rowsKeyed: survivors.length,
  victimsDeleted: victims.length, protectedKept: protectedKept.length,
  albumCopiesKept, blobsDeleted, chatsChanged, charactersChanged,
  messagesChanged, durationMs,
});
```
then `reportCensus` (`:631`). The early return (`:434-446`) logs nothing. The
runner then logs `Migration completed successfully {context:
'migrations.runMigrations', migrationId, itemsAffected, durationMs}`
(`migrations/index.ts:196-201`) — v5 has NO runner and emits that line for NO
re-homed migration (grepped: zero hits in core + host), so the host line is not
its stand-in by any precedent.

**Divergence:** v5 logs one extra v5-only INFO (`quilltap::boot … into one
image per configuration`, snake_case, incl. on the no-victims exit) and its v4
line lacks `durationMs`.

**Predicted hunks:** delete `log_collapse_ran` + its two calls
(`host.rs:1790` → `Ok(_) => {}`; `:1823` drop the call — the core line already
precedes the stamp failure, preserving v4's order); add `durationMs` (wall
clock from the pass start, `i64` ms) as the core line's last field
(`avatar_rolls_collapse_heal.rs:982-995`). Blast radius: `host.rs` only;
`CollapseOutcome::Ran` stays (other consumers: grep shows only host + tests).

**Proof:** `avatar_rolls_collapse_heal_equivalence.rs:~490-505` + oracle
`harness/oracle/cases/avatar-rolls-collapse-heal.test.ts:451-470` already
compare the core line's fields (NOT `durationMs` — nondeterministic); add a
PRESENCE pin (`durationMs` key present and an integer on both sides) to the
shaper. `host_boot_hardness`: rewrite `COLLAPSED_TWO_ROLLS` to
`INFO quilltap::migration Collapsed duplicate avatar rolls context=migration.collapse-duplicate-avatar-rolls configurations=1 rowsKeyed=1 victimsDeleted=1 protectedKept=0 albumCopiesKept=0 blobsDeleted=0 chatsChanged=0 charactersChanged=0 messagesChanged=0 durationMs=<n>`
— `assert_line` is EXACT, so `durationMs` needs a prefix match or a
normalizer (NOT MEASURED how the capture renders it; read `CaptureLayer`'s
format before writing). Add an `assert_silent("into one image per configuration")`.

**Fixtures:** none rebuilt; the collapse family's committed corpus is read by
that family only (oracle regen at the pin needed only if the shaper grows
`durationMs`).

**Risk:** `host_boot_hardness` asserts are exact strings — every arm naming the
success line moves (`:1114`, `:1149`, `:1139`). `quilltap::migration` vs
`quilltap::boot` target: the capture line prefix changes.

---

## A3. P4.D248 Unification — the structural pass's log position

**Source** (`p4.d248-*.md:3`):
> NEW recorded divergence — the structural pass runs after `seed_built_ins`,
> which already holds the reaper and the store backfills, so its ERRORs log
> AFTER those lines where v4's PHASE 3.1 precedes 3.3b/3.4

**v5 today:** `host.rs:563` `seed_built_ins(db)?` (one `write_blocking`
closure, `:1248-1978`) runs, in order: template seed; ~20 migration-twin
ensures/heals (`:1276-1530`); **PHASE 3.6** render reconcile (`:1532-1570`);
`chat_informs` ensures; **PHASE 3.65** FTS reconcile (`:1613-1644`); the
mount-aware block — collapse (`:1725-1835`), placeholder heal, then
`ensure_builtin_mounts_with` (`:1881`, which INSIDE runs the lazy ensures AND
the **3.3b** reaper `sweep_orphaned_store_children`, `builtin_mounts.rs:283-295`),
**3.4c** general scenarios folder (`:1889`), the state.json seed (`:1905-1918`),
the headshoulders scan (`:1951`); finally `create_missing_structural_tables`
(`:1969`). THEN `host.rs:573-581` the pass. Then 3.66 (`:591`), 3.7 (`:597`).

**v4:** `instrumentation.ts` — PHASE 1 migrations (`:404`), 1.25 seed (`:470`),
2, 3, **3.1 verifyStructuralTables (`:569-582`)**, 3.25, 3.2 vault backfill
(`:598`), 3.3, **3.3b reaper (`:692-701`)**, 3.4a/3.4/3.4b/3.4c (`:710-776`),
3.5, **3.6 (`:874`)**, **3.65 (`:900`)**, 3.66, 3.7.

**Divergence:** the pass's ERROR/DEBUG lines (and nothing else — the pass is
read-only) log after 3.3b/3.4c/3.6/3.65 on v5 where v4 logs them before; the
problems recorded are identical.

**What moving it costs:** (1) split `seed_built_ins` into two writer closures
with the pass between — "migrations' twins + lazy ensures + absent-table
creation" then "3.3b onward" — `seed_built_ins` returns `EnsureFailures`, so
the first half returns it and the pass runs on the read pools (it must not run
inside the writer closure); (2) peel the reaper out of
`ensure_builtin_mounts_with` (`builtin_mounts.rs:270-296`) into its own call
after the pass — `builtin_mounts.rs` is a shared core file; (3) move 3.6 and
3.65 (currently main-only, BEFORE the mount block) after the pass, which also
moves them relative to the D184/D175 heals — a second log-order change on its
own; (4) `create_missing_structural_tables` must stay in the first half.
`host_boot_hardness` pins only within-pass order (`:367-371`, `:393-397`), no
cross-phase order — so no existing pin reds either way; a move needs NEW
order pins to be worth anything. Estimate: 3 hunks in `host.rs` + 1 in
`builtin_mounts.rs`, plus order arms. **Recommendation: a human ruling — keep
as a recorded divergence (log order only) unless a dogfood reader needs it.**

---

## A4. P4.D248 Unification — `qtap_schema_embed_guard`'s locator

**Source** (`p4.d248-*.md:3`):
> `qtap_schema_embed_guard` defaults to the live v4 checkout (`QT_V4_ROOT`), not
> `QT_V4_CHECKOUT`, so a dirty checkout reds it in a pinned gate (it did at this
> unification).

**v5 today:** `crates/quilltap-harness/tests/qtap_schema_embed_guard.rs:43-49`
`fn v4_root() -> Option<PathBuf>` reads `QT_V4_ROOT`, else
`$HOME/source/quilltap-server` if it is a dir; SKIP message `:93` "set
QT_V4_ROOT". **The same shape is in a SECOND guard:**
`public_schemas_vendor_guard.rs:68-74` (header `:21`). Census of the harness's
v4 locators: `env::var("QT_V4_CHECKOUT")` ×3 (`zod_version_guard.rs:94-102`,
`builtin_prompt_templates_guard.rs:47-55`, `provider_sdk_version_guard.rs:86-94`
— all `Ok(v) if !v.is_empty()` else `$HOME/source/quilltap-server`, absent dir
→ loud `SKIP:` at the use site); `QT_V4_ROOT` ×2 (the two above). No script or
sweep-results file sets `QT_V4_ROOT` (grepped); `QT_V4_CHECKOUT` is what the
pinned gate exports (`cli_differential`'s run lines).

**v4:** n/a (harness-only).

**Divergence:** two guards ignore the gate's pin variable.

**Predicted hunks:** both files: `v4_root()` reads `QT_V4_CHECKOUT` (non-empty)
FIRST, then `QT_V4_ROOT` as a legacy alias, then the default; header recipe +
SKIP text name `QT_V4_CHECKOUT`. ~10 lines each. Blast radius: the two files.

**Proof:** none needed beyond running both guards with `QT_V4_CHECKOUT` at a
clean pin while the live checkout is dirty (a gate check, recorded). Optionally
a unit test in each for the precedence (env reads race other tests — avoid
`set_var`; a pure `fn locate(checkout: Option<&str>, root: Option<&str>, home)`
is testable).

**Fixtures:** none.

---

## B. P4.D253 — `RefusingTextExtractor`'s stderr wording

**Source** (`work-orders/p4.d253-bug177-pdf-extraction-through-the-converter-seam.md:5`):
> **Still OPEN, by name:** … `RefusingTextExtractor`'s stderr line says the file
> "will be bookkept as extraction-failed", false at this new caller (the
> fallback runs) — a `mount_index` wording fix for the smalls round.

(Also `phase-4.md:7177-7180`.)

**v5 today:** `crates/quilltap-core/src/services/mount_index/converters.rs:31-46`:
```
eprintln!(
    "DocumentTextExtractor unavailable — refusing {file_type} text extraction \
     (the production pdf/docx extractor is deferred by work order P4.6y); \
     the file will be bookkept as extraction-failed, not silently skipped"
);
```
returning `String::new()`. `default_text_extractor()` (`:50-53`) is the
production seam. Callers (production, counted): `api/mount_files.rs` 9 sites
(`:102,195,240,278,392,649,710,794,1040`), `scanner.rs:636`, `reindex.rs:473`,
`reindex_file.rs:92`, `link_groups.rs:78`, plus the files threading a
`SharedTextExtractor` (`api/engine.rs`, `character_archive/service.rs`,
`file_ops.rs`, `refresh.rs`, `store_file.rs`) — all Scriptorium pipelines, where
the sentence is TRUE (v4's "Converter produced no text" / scan `empty` arms).
The ONE caller where it is false: `generators/file_content.rs:331-358`
`extract_pdf_content` → `pdf_text_extractor()` (`:290-295`, falls back to
`default_text_extractor`) → `''` → WARN `pdf-parse found no text, using native
fallback extraction {size}` and `extract_pdf_text_fallback` runs. Its callers:
`generators/wizard.rs:833`, `generators/ai_import.rs:1200`.

**v4:** **NO such line** — the stderr refusal is v5's own notice (v4's
`lib/mount-index/converters/pdf-converter.ts` has a working `pdf-parse` and logs
only `PDF buffer is empty` `:13`, `Failed to extract text from PDF buffer` `:30`,
`Failed to read PDF from disk` `:45` — none of which v5 emits here, consistent
with D19's seam). Nothing to diff.

**Divergence:** a v5-only stderr notice makes a false claim at the
file-content caller.

**Predicted hunks:** the trait method has no caller context (`extract(&self,
bytes, file_type)`), so: **(recommended) caller-neutral wording** in the one
`eprintln!` — e.g. "…deferred by work order P4.6y); returning no text — a
Scriptorium scan bookkeeps the file as extraction-failed, and a document read
falls back to its native PDF scrape" — and move the format into a
`fn refusal_notice(file_type: &str) -> String` so it is unit-pinnable.
Alternative: a second unit struct for the file-content seam (adds a type for a
stderr string — not worth it). Also the module doc `:11-15` + struct doc
`:31-33` say "the call sites bookkeep exactly like v4's
converter-produced-no-text arms" — reword the same way.

**Proof:** a core unit test on `refusal_notice("pdf")` (no "bookkept" claim
unqualified; names both outcomes). stderr is not tracing — no capture rig
applies; no harness family reads the text (grepped: no pin anywhere).

**Fixtures:** none. **Risk:** none; `converters.rs` is in `mount_index` — no
sibling named it.

---

## C. P4.139's OPEN items (the four named in the brief)

**Source** (`work-orders/p4.139-api-key-read-class-census-guard-isactive-routes.md:3`):
> **OPEN by name:** Tier 3 items 15–21 as ordered (…; the `wrapper-no-caller`
> scoped wrappers — dead code the census cannot see callers of); the greeting's
> corrupt-key ERROR as an `initial_greeting_equivalence` arm (the lane's
> orphaned handoff); the wizard caller pin; the two hand-wraps in
> `chat_create.rs` / `chat_enrichment.rs` onto `read_api_key`.

None of these is closed by P4.136 — P4.136 (unified 2026-10-02) built the two
`db::fallback` homes; P4.139 (unified 2026-10-03) is the later conversion whose
leftovers these are. Items 15–20 (pricing cadence, `test_plants`, `MainReads`
move, almanack SQL, help-chat/SB cast reads, `GET /api-keys/[id]`) are not in
this lane's brief and are not surveyed here.

### C1. The greeting's corrupt-key arm

**v5 today:** the greeting is NOT in a `services/greeting*.rs` file — it is
`auto_generate_first_message` in `crates/quilltap-core/src/services/chat_create.rs:2480-2525`
(`services/initial_greeting.rs` is the DB-free `generateGreetingMessage` port).
`:2502-2504` reads the key through `crate::db::fallback::find_api_key_by_id_or_none(api_key_id,
|| api_keys::find_by_id(main, api_key_id))`; `None` → `tracing::warn!(context =
"autoGenerateFirstMessage", "[Chats v1] Connection profile is missing its API key")`
(`:2516-2519`) and `GeneratedGreeting::none()`. The home's line
(`db/fallback.rs:204-218`): `ERROR quilltap::db Error finding API key by ID
collection=connection_profiles keyId=<id> error=<bare>`. Existing pins:
`chat_create.rs` test module `:4479` `a_dangling_api_key_id_is_named_and_ends_the_greeting`
(a MISSING row — no ERROR) and `:~4512` `a_profile_without_an_api_key_id_never_says_it`
(silence). **No arm plants an UNREADABLE key row** (lane record,
`status-log.md` ≈ `:161433-161440`).

**v4:** `app/api/v1/chats/route.ts:698-705`: `const storedKey = await
repos.connections.findApiKeyById(connectionProfile.apiKeyId); if (!storedKey) {
logger.warn('[Chats v1] Connection profile is missing its API key', { context:
'autoGenerateFirstMessage' }); return NO_GREETING; }`.
`findApiKeyById` = `connection-profiles.repository.ts:249-266`, a 4-arg
FALLBACK `safeQuery` (`'Error finding API key by ID', { keyId: id }, null`) —
`collection: 'connection_profiles'` injected by `base.repository.ts:101`. A
corrupt row (Zod `ApiKeySchema.parse` throw) → ERROR `Error finding API key by
ID {collection, keyId, error}` then the WARN. REACHABLE.

**Divergence:** NONE in behaviour (converged by P4.139); the gap is a missing
pin of the ERROR-before-WARN order.

**Predicted hunks:** test-only. The handoff's named family is WRONG:
`initial_greeting_equivalence.rs` is DB-free (header `:1-8`, drives v4's
`generateGreetingMessage` with no repository) and cannot plant a row. Copy
`a_dangling_api_key_id_is_named_and_ends_the_greeting` (`:4479-4510`) with a
venue whose key row is unreadable (P4.139's `api_key_service::test_instance::provisioned`
BLOB-key plant is the idiom), assert `one_at(…, "ERROR", "Error finding API key
by ID")` BEFORE `one_at(…, "WARN", "…missing its API key")`. Lives in
`chat_create.rs`'s test module — **NOT this lane's file** (flag below).

**Proof class:** capture-pinned unit (v4 bytes transcribed; the home's own
line already two-sided via P4.139's route families).

### C2. The wizard caller pin

**v5 today:** `crates/quilltap-core/src/generators/wizard.rs:735`
(`profile_api_key_value_scoped` for the primary) and `:770-800` — `vision_api_key
= primary_api_key.clone()`; replaced only on
`api_key_service::profile_api_key_found_scoped(db, &secondary, user_id) ==
Some(key)` (`:796-800`). Helper-level pin only (P4.139 Unification).
`character_wizard_tier3_equivalence.rs:452-461` `normalize` REMOVES `apiKey`
from every recorded call; the oracle `harness/oracle/cases/character-wizard-tier3.test.ts:185-220`
records `sendMessage(params)` fields but NOT the second argument (the key).

**v4:** `lib/services/character-wizard.service.ts:726-730` (primary),
`:754-771` (`let visionApiKey = primaryApiKey; … if (secondaryProfile.apiKeyId)
{ const apiKey = await repos.connections.findApiKeyByIdAndUserId(…); if (apiKey)
visionApiKey = apiKey.key_value; }`) and the streaming twin `:991-1034`;
`provider.sendMessage({…}, apiKey)` (`:419-428`) — the key is arg 2.

**Divergence:** NONE known; the caller is unpinned.

**Predicted hunks:** oracle side — record `apiKey` (sendMessage's 2nd arg) on
each call; v5 side — stop stripping `apiKey` in `normalize` (or strip only
where v5 legitimately differs; NOT MEASURED whether v5's recorder records the
key — read the Rust family's canned provider). The committed fixture spec
ALREADY carries distinct keys: `harness/oracle/fixtures/character-generators.json`
`apiKeys` `a0000002-…0001` (`OPENAI_COMPATIBLE`, `sk-synthetic-mock-key`) →
profile `c0000002-…0001`, and `a0000002-…0002` (`OPENAI`,
`sk-synthetic-vision-key`) → profile `c0000002-…0002`. So the existing
`stream_gallery_vision_fallback` case (`character-wizard-tier3.json:266-270`,
`visionProfileId c0000002-…0002`) already exercises "secondary REPLACES". The
"secondary KEEPS the primary's" arm (no `apiKeyId` / dangling) needs a third
profile → the committed pair rebuilt, OR an in-case plant (`UPDATE
connection_profiles SET apiKeyId = NULL WHERE id = 'c0000002-…0002'` on both
copies — the P4.139 route-family idiom).

**Fixtures (shared — do NOT rebuild in this lane):**
`crates/quilltap-web/tests/fixtures/character-generators-{main,mount}.db` built
by `harness/oracle/fixtures/build-character-generators-fixture.ts`; readers:
`character_wizard_tier3_equivalence.rs`, `character_optimizer_tier3_equivalence.rs`,
`ai_import_tier3_equivalence.rs`, `crates/quilltap-web/tests/common/mod.rs`,
the three oracle `.test.ts`, `migrate-memories-fixture-columns.ts`. The P4.D253
OPEN Tier 2 item 7 (a PDF `document` case) ALSO wants this pair rebuilt — one
lane, one rebuild; prefer in-case plants here.

### C3. The dead `wrapper-no-caller` wrappers (a human call)

**v5 today:** `crates/quilltap-core/src/services/api_key_service.rs:405-475`
— FIVE scoped wrappers: `get_all_api_keys` (`:414`), `find_api_key_by_id_scoped`
(`:422`), `update_api_key_scoped` (`:434`), `delete_api_key_scoped` (`:449`),
`record_api_key_usage_scoped` (`:463`). Production callers counted (grep over
`crates/**/*.rs`, excluding the definitions): **ZERO for all five** —
`find_api_key_by_id_scoped` is called only by the other three wrappers
(`:441,455,469`) and the file's tests (`:1014-1045`); the three mutators only by
those tests; `get_all_api_keys` only named in the census. The census classifies
TWO as `wrapper-no-caller` (`api_key_read_sites_census.rs:313-314`, counts
`:355-377`) — the three mutators are not reads and so are invisible to it.

**v4:** `UserScopedConnectionsRepository` (`user-scoped.ts`) — the class v5
mirrors; v4's api-keys routes use the UNSCOPED container (P4.136's measured
finding), so v4's own wrappers are largely dead too (NOT MEASURED which v4
callers remain).

**Divergence:** none (dead code). **Hunks if ruled "delete":** remove the five
fns + the file's wrapper tests (`:~1000-1050`); the census's two
`wrapper-no-caller` rows + its class counts (`:355-377`) move (the
"closing-a-divergence-moves-the-censuses" trap). **Ruling needed (human).**

### C4. The two hand-wraps onto `read_api_key`

**v5 today:** `services/chat_create.rs:2502-2504` and
`services/chat_enrichment.rs:461-463` call `crate::db::fallback::find_api_key_by_id_or_none(id,
|| api_keys::find_by_id(conn, id))` directly — deliberately, so each file's
`api_keys` import stayed used (lane record ≈ `status-log.md:161394-161398`).
`read_api_key<R: MainReads>(reads, id)` (`api_key_service.rs:86-90`) is the same
body over `MainReads` (implemented for `Connection`, `:60-67`). The census
classifies both `home`.

**Divergence:** none (identical body); a DRY fold.

**Hunks:** each site → `crate::services::api_key_service::read_api_key(conn,
id)`; drop the now-unused `api_keys` import if nothing else in the file uses it
(grep at order time). Census: both rows reclassify (whatever class `read_api_key`
callers get) — counts move.

**Proof:** the existing pins stay green (`the_chat_enrichments_profile_summary_nulls_a_corrupt_key`
in `api_key_service.rs`, the C1 arm); the census.

**Ownership flag (C1 + C4):** `services/chat_create.rs` and
`services/chat_enrichment.rs` are NOT in this lane's proposed list and are
high-traffic files (P4.148 owns `quilltap_import/**`, not these — NOT MEASURED
whether another 2026-10-05 survey claims them). Each hunk is ≤10 lines in a
named region; order them as "these hunks ONLY".

---

## D. P4.141's OPEN items

**Source** (`work-orders/p4.141-model-layer-timeout-wording-2xx-parse-plugin-error-lines.md:3`
Unification, `:8-13` Status):
> **OPEN by name:** Tier 3 items 14–20 as written; `UNPORTED_PLUGIN_WARN_LINES`
> (the Responses EOF WARN); the withdrawn `google-incomplete-tail` `.wire` row
> (wants `streaming_composer_equivalence::assert_catch_lines` widened first —
> bytes in the lane record); Google's sibling `No parts found in Google
> response candidate` WARN (neither ported nor named until now); the family's
> 2xx rows never assert `transport_kind == None`, and `PosedTransport::execute`
> ignores `policy` (…); a failed 2xx body read now goes round the retry loop
> (…); every known-provider 2xx is parsed twice; `GenaiTail::push` rescans its
> whole buffer.
> NEW handoff: `cheap_llm_exec::is_timeout_failure` does not read the new
> transport kind

### D1. `is_timeout_failure` — **CLOSED (phantom)**

`crates/quilltap-core/src/services/cheap_llm_exec.rs:385-404`: the first arm is
`if error.transport_kind == Some(TransportErrorKind::Timeout) { return true; }`
(doc `:378-384` cites "a §3 finding at the `f6426e196` recorded-divergences
unification"). The P4.141 Unification paragraph's should-fix (1) records it
landed; unit pins at `:1684-1705` and `:2213-2216` construct
`.with_transport(Timeout|Connect, …)`. Nothing to order.

### D2. Google's `No parts found in Google response candidate` WARN

**v5 today:** NOT emitted anywhere (`ggrep` over `crates/` and
`harness/oracle`: zero hits; zero in the 880-row
`harness/oracle/fixtures/text-http-errors/text-http-errors.recorded.ndjson`).
Its sibling IS ported: `model/plugin_catch_log.rs:384-410`
`emit_google_no_candidates` (WARN, target `quilltap::model::completion_provider`,
`context="GoogleProvider.extractTextFromResponse"`, `modelName`, `blockReason`
omitted when absent, message `No candidates found in Google response`), called
ONLY from the non-streaming send (`model/completion_provider.rs:315-318`).
`parse_google` (`model/response_parse.rs:927-958`) reads
`candidates[0].content.parts` and has NO `content.text` fallback.

**v4 at `07b8f0209`:** `plugins/dist/qtap-plugin-google/provider.ts:255-305`
`extractTextFromResponse(response, modelName)` (plugin logger
`createPluginLogger('qtap-plugin-google')`, `:13`):
1. `if (response?.text) return response.text;`
2. no/empty/non-array `candidates` → `logger.warn('No candidates found in
   Google response', { context: 'GoogleProvider.extractTextFromResponse',
   modelName, blockReason: response?.promptFeedback?.blockReason })` → `''`
   (ported).
3. `parts = candidates[0]?.content?.parts`; missing / non-array / empty →
   **`logger.warn('No parts found in Google response candidate', { context:
   'GoogleProvider.extractTextFromResponse', modelName, finishReason:
   firstCandidate?.finishReason })`** (`finishReason` omitted when undefined),
   then `if (content?.text) return content.text;` else `''`.
4. a throw → `logger.warn('Error extracting text from response', {context,
   modelName, error})` → `''` (not ported either; likely unreachable — every
   access is `?.`).

Callers: `sendMessage` `:618` (every non-streaming send) and `streamMessage`
`:847-850` — ONLY `if (isThinking && !totalStreamedContent && lastResponse)`.
**Reachable** on the send path for a 2xx `{"candidates":[{"finishReason":
"MAX_TOKENS"}]}` (or `"SAFETY"` with no content) — a real Gemini shape.

**Divergence:** v5 is silent where v4 WARNs `No parts found…` (send path, and
the thinking-model empty-stream path); the streaming path ALSO lacks the
already-ported `No candidates…` WARN (`model/decoders/google_parts.rs:244-264`
`extract_text` + `build_done` `:~285` log nothing). Possible content divergence
(`content.text` fallback) — **NOT MEASURED**: the genai SDK's response
converter likely drops unknown `content` keys, making `content.text`
unreachable on the wire; the recorder through the REAL genai answers it.

**Predicted hunks:** `model/plugin_catch_log.rs` — generalize to
`emit_google_extract_text_warns(model, body)` (or add
`emit_google_no_parts`) with v4's ordered conditions (SDK `.text` truthy → no
line; reuse `parse_google`'s `.text`-getter equivalence); call it at
`completion_provider.rs:318` in place of the current call; optionally at
`google_parts.rs` `build_done` behind the same `is_thinking_model &&
total_streamed_content.is_empty()` gate (target there is the streaming
provider's — read v5's Google streaming catch target first). If the content
fallback is measured REACHABLE, `parse_google` gains it.

**Proof:** `text_http_errors_equivalence` (tier-1 wire differential over v4's
REAL plugins; `diff_lines` `:1029-…` already diffs `pluginWarnLog` in order
with a silence leg on every row). Add a case to
`harness/oracle/fixtures/text-http-errors/cases.json` (42 cases today; the
`ok_*` five are `ok_choices_empty`, `ok_choice_no_message`, `ok_empty_object`,
`ok_non_json`, `ok_empty_body_json`) — e.g. `ok_google_candidate_no_parts`
(`{"candidates":[{"finishReason":"MAX_TOKENS","content":{"role":"model"}}]}`)
with the per-case `modes`/provider filter so it records GOOGLE only (else every
provider grows rows); regen at the pin via `regenerate-text-errors.sh` (the
P4.141 recipe, order `:300-310`). Unit pin: copy
`plugin_catch_log.rs:674` `google_no_candidates_warn` (thread-scoped
`captured_with`). Stream half: a `google_parts` `.wire` (same shape) — see D4
for the shared `cases.json`/`assert_catch_lines` constraint.

**Fixtures:** `text-http-errors/cases.json` + `.recorded.ndjson` — readers:
`text_http_errors_equivalence.rs` and `record-text-errors.mjs` (grepped
`ok_` hits elsewhere are unrelated names). One regen owns both.

### D3. `transport_kind == None` on 2xx rows; `PosedTransport::execute` ignores `policy`

**v5 today:** `crates/quilltap-harness/tests/text_http_errors_equivalence.rs`:
- `PosedTransport::execute` `:564-582` takes `_policy` and never reads it;
  only `execute_stream` (`:584-610`) uses `policy.timeout` (the headers budget)
  — so the row's `requestTimeoutMs` reaching the NON-streaming transport is
  unproven.
- The error-kind asserts `:1126-1170`: `Posed::Http` asserts the message only
  (no `transport_kind == Some(Http)`); `Connect|Timeout` assert
  `Some(Connect|Timeout)` (`:1156-1166`); `Posed::Ok2xx(_) => {}` (`:1168`) —
  an `Err` on a 2xx row (the seven a2 parse-throw providers) never asserts
  `transport_kind == None`.
- Core: every `TransportError` maps through `.with_transport(error.kind, …)`
  (`model/completion_provider.rs:140`, `model/streaming_provider.rs:354,626`),
  so HTTP rows carry `Some(Http)`; a 2xx parse error is
  `CompletionError::new(…)` → `None` (`model/completion.rs:236`).

**v4:** n/a (harness-side proof gap; v4's `AbortError`/`TimeoutError` name arm
is what the kind models).

**Divergence:** none known; two proof gaps.

**Predicted hunks (harness only):** `PosedTransport` gains
`seen_timeout: Mutex<Option<Duration>>` recorded in `execute`; on `transport_hang`
send rows assert it equals `spec.request_timeout_ms` (and on the others the
default `TransportPolicy` budget — read `transport.rs:117-125`); the `Http` arm
asserts `Some(TransportErrorKind::Http)`; the `Ok2xx` arm asserts `e.transport_kind
== None`. Mutation proofs: drop `with_transport` on the completion path → Http
arm reds; set a kind on the 2xx parse error → Ok2xx arm reds; ignore the
row's timeout in the composer → the policy assert reds.

**Fixtures:** none (reads the committed corpus; no regen).

### D4. The withdrawn `google-incomplete-tail` `.wire` row

**Lane record** (`status-log.md` ≈ `:160397-160420`, "Withdrawn, a §S handoff"):
the row `fixtures/streams/google_parts/google-incomplete-tail.wire` (`data:
{…"Hello"…}\n\n` + an undelimited `{"candidates":…}`) was built and recorded at
the pin; the decoder edit (the tail check) LANDED; the row was not committed
because `streaming_composer_equivalence` reads the same `cases.json` and its
`assert_catch_lines` cannot render Google's line. Recorded row bytes (verbatim
from the record): `{"decoder": "google_parts", "provider": "google", "case":
"google-incomplete-tail", "error": "Incomplete JSON segment at the end",
"chunks": [{"content": "Hello", "done": false}], "thrown": {"message":
"Incomplete JSON segment at the end", "name": "Error"}, "refusal": {"refused":
false}, "trigger": "provider-error", "pluginErrorLog": [{"plugin":
"qtap-plugin-google", "message": "Error streaming from Google Gemini API",
"context": {"context": "GoogleProvider.streamMessage", "model":
"gemini-2.5-flash", "error": "Incomplete JSON segment at the end"}, "error":
null}]}`.

**v5 today:** `crates/quilltap-harness/tests/streaming_composer_equivalence.rs:382-408`
`assert_catch_lines` filters `l.contains("API error in streamMessage")` and
formats `ERROR quilltap::model::streaming_provider {message} context={…}
baseUrl={…} error={…}` with `e["context"]["baseUrl"].as_str().unwrap()` and
`e["error"].as_str().unwrap()` — it would PANIC on the Google entry (no
`baseUrl`; top-level `"error": null`) and its filter cannot see `Error streaming
from Google Gemini API`. Stream cases live at
`harness/oracle/fixtures/streams/google_parts/cases.json` (+ four siblings).

**Divergence:** none in behaviour (decoder fixed); a missing corpus row.

**Predicted hunks:** `assert_catch_lines` rewritten to render each v4 entry's
`context` keys IN ORDER with v4's `error` rule (the `text_http_errors_equivalence`
`render_v4_line` idiom, `:~985-1004`), filter on the recorded messages rather
than a fixed needle; then commit the `.wire`, the `cases.json` entry and the
recorded row (regen `regenerate-stream-fixtures.sh` at the pin — or append the
recorded bytes, which the record says were recorded at `f6426e196`; at
`07b8f0209` re-record to be safe). Families re-run: `stream_decoders_equivalence`,
`streaming_composer_equivalence`.

**Fixtures:** `harness/oracle/fixtures/streams/google_parts/cases.json` + its
recorded ndjson — readers: `stream_decoders_equivalence.rs`,
`streaming_composer_equivalence.rs`, `record-stream-fixtures.mjs` (NOT MEASURED
exhaustively — grep `google_parts/` at order time).

### D5. Not in this brief, listed for completeness (P4.141 OPEN)

`UNPORTED_PLUGIN_WARN_LINES` (`text_http_errors_equivalence.rs:1011-1022`: the
Responses `Stream ended without response.completed event` WARN for OPENAI and
GROK `stream` — lives in `model/decoders/responses_api_sse.rs`); the 2xx
body-read retry loop; the double parse; `GenaiTail::push`'s rescan; Tier 3
14–20 (rulings/deferrals). Not surveyed in depth.

---

## E. P4.140 — `ProductionSpineFactory::new`'s UTC default + a `TimeZone::get` needle

**Source** (`work-orders/p4.140-salon-spine-option-v-file-processing-frame-carina-row.md:3`):
> **OPEN by name:** … `ProductionSpineFactory::new` defaulting `display_zone` to
> UTC (only a source needle guards a future caller); the census hunts ambient
> reads and `TimeZone::UTC` but not a display surface re-deriving the zone via
> `TimeZone::get(server_tz)`; …

**v5 today:**
- `crates/quilltap-host/src/spine.rs:3674-3712`: `ProductionSpineFactory { …,
  display_zone: TimeZone, … }`; `new(base_dir, version, tz)` sets
  `display_zone: TimeZone::UTC` (`:3698`); `with_display_zone(self, zone)`
  (`:3708-3711`).
- Callers of `ProductionSpineFactory::new` (counted): **1 production**
  (`crates/quilltap-web/src/lib.rs:271-274`, chained `.with_display_zone(config.display_zone.clone())`
  — shared by the HTTP binary and the Tauri shell); **9 test** — host
  `tests/host_llm_log_cleanup.rs:125`, `tests/host_headshoulders_backfill.rs:107`;
  web `tests/avatar_rolls_routes.rs:282,344`, `characters_generators_routes.rs:57`,
  `generators_wizard_routes.rs:45`, `image_profile_generate_dispatch_wire.rs:116`,
  `impersonation_voice_preview_wire.rs:81`, `profile_bound_api_key_wire.rs:174`
  (all have a `HostConfig` `c` in scope; none call `with_display_zone`).
- The guard today: `crates/quilltap-harness/tests/host_zone_sites_census.rs`
  `HOST_SITES` row `:150-157` (needle `.with_display_zone(config.display_zone.clone())`
  ×1 in `quilltap-web/src/lib.rs`); host UTC allowance `:506-526` — `spine.rs`
  allowed exactly **2** production `TimeZone::UTC` (`js_local_offset_minutes`
  fallback `:2425` + `new`'s default).
- The census's hunts (`host_zone_sites_census`, `:352-442`): (1) ambient
  `system_display_zone()` reads (`CENSUS`); (2) `VALUE_SITES`/`HOST_SITES`
  needles; (2b) `civil_from_days(` / `div_euclid(86_400_000)` with
  `UTC_ARITHMETIC_ALLOWED`; (2) `TimeZone::UTC` with `UTC_ALLOWED` (`:260-…`).
  **No hunt on `TimeZone::get(`.**
- Production `TimeZone::get(` sites today (outside `#[cfg(test)]`, each checked
  against its file's first `#[cfg(test)]` line) — all NAME-fed calendar /
  story-zone reads, none a display default:
  `enclave/cron.rs:97` (cron), `enclave/step.rs:261` (`last_local_midnight_iso`),
  `day_references.rs:313` (`resolve_day_reference`), `db/llm_logs.rs:1297`
  (retention cutoff), `progressions/engine.rs:749` (`format_instant_en_us`,
  the story zone NAME — P4.140 Tier 3 residue), `services/memory_recap/distill.rs:150`
  (`local_date_stamp`), `chat_timestamp.rs:226` (`zone_offset_seconds`, story
  timezone); host `spine.rs:2425` (`js_local_offset_minutes`). = **7 core files
  ×1, 1 host file ×1.** (`quilltap-web/src/main.rs:405,411` are in its
  `#[cfg(test)]` at `:277`.)

**v4:** n/a (v4 reads the process zone ambiently; the VALUE threading is v5's
ruled shape).

**Divergence:** none today; two guard gaps (a future caller can forget
`with_display_zone`, and a future display surface can re-derive a zone from the
`tz` NAME without any census noticing).

**Predicted hunks:**
1. `spine.rs:3691-3711` — `new(base_dir, version, tz, display_zone: TimeZone)`
   (required), delete `with_display_zone` (or keep for tests — prefer delete:
   one way in). `quilltap-web/src/lib.rs:271-274` passes
   `config.display_zone.clone()`; the 9 test callers pass `c.display_zone.clone()`
   (all have it). A `from_host_config(&HostConfig)` ctor is the alternative that
   also removes the `tz` hand-copy — pick one.
2. `host_zone_sites_census.rs`: the web `HOST_SITES` row's needle moves to the
   new argument (`ProductionSpineFactory::new(base_dir, version, tz,
   config.display_zone.clone())` or the `from_host_config` call); the host
   `spine.rs` UTC allowance `2 → 1` (`:517-520`, comment `:506-514`); NEW hunt
   (2c): `TimeZone::get(` per file with a `NAME_ZONE_ALLOWED: (path, count,
   why)` table — the 7 core rows + a host branch (`spine.rs` 1) — copying the
   `UTC_ALLOWED` loop (`:430-441`) verbatim.

**Proof:** the census itself (source census, red-first: add the hunt with an
empty allow-list → 8 reds, fill to green; mutation: a `TimeZone::get(&self.tz)`
added to a display site reds). The POSIX child (`a_posix_tz_rule_reaches_every_display_entry`,
`:742`) unchanged.

**Fixtures:** none. **Risk:** 7 web test files touched (crate `quilltap-web/tests`
— not P4.152's `apps/web/**`); `api/types.rs` untouched.

---

## §Ownership proposal

**Edit:**
- `crates/quilltap-core/src/db/doc_mount_points_column_repair.rs` (NEW) — or the
  fn in `crates/quilltap-core/src/db/mount_index_case_repair.rs`
- `crates/quilltap-core/src/db/mod.rs` — ONE `pub mod` line (only if the new file)
- `crates/quilltap-core/src/services/builtin_mounts.rs` — the one new
  `lazy_ensure` before `:267` (and the reaper peel ONLY if A3 is ruled "move")
- `crates/quilltap-core/src/db/avatar_rolls_collapse_heal.rs` — `durationMs`
- `crates/quilltap-host/src/host.rs` — `log_collapse_ran` removal (`:1790`,
  `:1823`, `:2031-2069`); the A3 split only if ruled
- `crates/quilltap-host/src/spine.rs` — `ProductionSpineFactory::new` (`:3691-3711`)
- `crates/quilltap-web/src/lib.rs:271-274` (one call)
- `crates/quilltap-core/src/services/mount_index/converters.rs` — wording + `refusal_notice`
- `crates/quilltap-core/src/model/plugin_catch_log.rs`,
  `model/completion_provider.rs:315-318`, `model/decoders/google_parts.rs`
  (`build_done`), `model/response_parse.rs` (only if `content.text` measured reachable)
- `crates/quilltap-core/src/generators/wizard.rs` — none expected (test-side pin)
- C1/C4 hunks in `services/chat_create.rs` (`:2502-2504` + one test in its
  module) and `services/chat_enrichment.rs:461-463` — **"these hunks ONLY"**
- C3 (`services/api_key_service.rs:405-475` + tests) ONLY on a human "delete"

**Harness / oracle / fixtures owned:**
- `table_shape_equivalence.rs` + `harness/oracle/cases/table-shape.ts` +
  `harness/oracle/fixtures/table-shape-spec.json`
- `crates/quilltap-host/tests/host_boot_hardness.rs` (new arm; `COLLAPSED_TWO_ROLLS`)
- `avatar_rolls_collapse_heal_equivalence.rs` + `avatar-rolls-collapse-heal.test.ts`
  (the `durationMs` presence pin)
- `qtap_schema_embed_guard.rs`, `public_schemas_vendor_guard.rs`
- `text_http_errors_equivalence.rs` + `record-text-errors.mjs` +
  `harness/oracle/fixtures/text-http-errors/{cases.json,*.recorded.ndjson}`
- `streaming_composer_equivalence.rs` (`assert_catch_lines`) +
  `stream_decoders_equivalence.rs` + `harness/oracle/fixtures/streams/google_parts/**`
- `character_wizard_tier3_equivalence.rs` + `character-wizard-tier3.test.ts`
  (+ `character-wizard-tier3.json` for an in-case plant) — **never** the
  `character-generators-*` pair
- `host_zone_sites_census.rs`; `api_key_read_sites_census.rs` (C3/C4 count moves)
- the 9 `ProductionSpineFactory::new` test callers (host ×2, web ×7)

**Must NOT touch:** `db/fallback.rs` and the db repository homes (P4.149) — note
C1/C4 CALL `find_api_key_by_id_or_none`/`read_api_key` and edit neither;
`db/projects.rs`, `db/groups.rs`, `services/quilltap_import/**` (P4.148);
`services/backup/**` (P4.147); `db/chat_informs.rs`, `*voice_mode_repair*`
(P4.151); `apps/web/**` (P4.152); `api/types.rs` (frozen);
`crates/quilltap-web/tests/fixtures/character-generators-*.db` (shared; the
P4.D253 PDF item's rebuild).

**Lands in a sibling-ish file:** `services/chat_create.rs` /
`services/chat_enrichment.rs` (C1/C4 — unowned by the brief's list); `db/mod.rs`
(one line); `builtin_mounts.rs` (shared mount boot, one hunk).

## §Open questions

1. A1: port only v4's four per-boot `onTableEnsured` ALTERs (with lines), and
   record `alignDocMountPointsSchema`'s 14-column silent set + the 3
   `doc_mount_file_links` policy columns as NO-PORT (ledger-gated migrations)?
2. A1: the line proof — a `Logger.prototype` spy in `table-shape.ts` (two-sided)
   or transcribed bytes in `host_boot_hardness` only?
3. A2: confirm deleting the host's v5-only summary line (no runner-line stand-in
   anywhere in v5) rather than renaming it; and whether to carry `durationMs`
   (nondeterministic — presence-pinned only).
4. A3: move the structural pass ahead of 3.3b/3.4c/3.6/3.65 (two-closure split +
   reaper peel), or keep it a recorded log-order divergence?
5. C3: delete the five dead scoped wrappers (human call, per item 21)?
6. C1/C4: may this lane edit the named hunks of `chat_create.rs` /
   `chat_enrichment.rs`, or do they go to whichever lane owns those files?
7. D2: is `candidates[0].content.text` reachable through the real genai SDK
   (measure in the recorder before porting the content fallback)?
8. E: `new(…, display_zone)` vs `from_host_config(&HostConfig)`?
