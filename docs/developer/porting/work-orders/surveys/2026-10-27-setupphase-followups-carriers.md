# Survey A: the carriers (backup / restore / .qtap import + export)

Pin: v4 `322a5488e`, read through `git show`. v5 main @ `4e43b714e`. Read-only: nothing was built and no test was run.

Tally: **OPEN 14 · CLOSED 7 · NEEDS-RULING 4 · NO-ACTION 12.** The per-order index is at the end.

---

## A. The four named items

### A1: the rider branch `claude/import-chat-record-columns-f49fb0` (`cb88a3840`)   [OPEN, needs rework before it lands]

- **Source:** P4.D317's OPEN line ("judged SOUND at unification, NOT taken"). P4.D327's lane record also files the same class as a HANDOFF to P4.D324: `status-log.md:191732`, "v5's chat create has no `ChatMetadataSchema` `JsonSchema` twin".
- **What the commit does:**
  - It extends `chat_create_zod_error` in `services/quilltap_import/entities.rs` with nine `z.record` checks.
  - Three columns are checked before the Concierge enums: `pendingOutfitNotifications`, `state` (where `null` is refused) and `compressionCache`.
  - Six are checked after them: `sceneState`, `equippedOutfit`, `characterAvatars`, `compiledIdentityStacks`, `courierCheckpoints` and `commonplaceSceneCache`.
  - It adds the unit test `chat_record_column_tests`.
  - It adds two oracle cases, `execute_chat_record_columns` and `…_preserve`, plus their non-vacuity block in `system_import_state.rs`.
  - It also touches the CHANGELOG, `status-log.md`, and a core version bump in `Cargo.toml` / `Cargo.lock`.
- **Does it still apply to main? Predicted by reading main's files.**
  - **`entities.rs`: applies cleanly.** Main's `chat_create_zod_error` sits at `:995-1021`, with the same doc lines, body and anchors as the rider's base. `fn create_chat` is at `:1029`, `Ok((id, cursors))` at `:1114` and `mod rendered_markdown_strip_tests` at `:1118`. None of these has moved.
  - **`crates/quilltap-harness/tests/system_import_state.rs`: conflicts.** The rider edits count literals that have moved on main:
    - `assert_eq!(ran, 83…)`: main has `89` at `:1845`, so the rider's value becomes 91.
    - `IMPORT_WARN_CASES 69`: main has `74` at `:2581-2583`, so it becomes 76.
    - `WARDROBE_LOG_CASES 69`: main has `74` at `:2599-2601`, so it becomes 76.
    - `MEMORY_LOG_CASES 69`: main has `74` at `:2620-2622`, so it becomes 76.
    - `REPO_LOG_CASES 15`: main still has `15` at `:2637-2639`, so it becomes 17.

    The hunk contexts no longer match. Re-apply this file by hand and re-count.
  - **`harness/oracle/cases/system-import-execute.test.ts`: hunk 1 conflicts.**
    - The rider inserts its two `executeCase(...)` entries just before the `];` that closes the case list.
    - Main has added P4.D317's outfit-history cases in the same place. Main's list now ends with `['DROP TABLE "chat_wardrobe_history"'],\n    ),\n  ];` just before `const outLines` (`:4515`), not with the rider's `true,\n    ),\n  ];`.
    - Hunk 2 (the appended `chatRecordColumnsPayload` at end of file) applies, because main's file still ends with the `afterAll` scratch-dir block.
    - `conciergeBogusPayload(base, chats)` is unchanged on main (`:2375-2400`), so the helper the rider uses still fits.
  - **Docs / `Cargo.toml` / `Cargo.lock`: conflict** (status-log, CHANGELOG, the version bump). Redo them.
  - **Recommendation:** port the rider's code by hand onto main. Do not cherry-pick it. Regenerate the `system-import-execute` oracle from the **`322a5488e`** pin; the rider's oracle came from `25b8fab6a`.
- **Is the nine-column list complete at the pin? No. Twelve `JsonSchema` columns are missing three.**
  - v4 `lib/schemas/chat.types.ts` @322a5488e: `ChatMetadataSchema` starts at `:831` and closes at `:1242`. Its `JsonSchema` columns (`common.types.ts:61`, `z.record(z.string(), z.unknown())`) are below; the Concierge enum `conciergeMode` sits at `:1022`.

    | Column | Line | Status |
    |---|---|---|
    | `sillyTavernMetadata` | `:840` | **missing from the rider; present at `25b8fab6a` too (`:834`)** |
    | `scenarioSource` | `:940` | not `JsonSchema`; already checked by v5 |
    | `pendingOutfitNotifications` | `:977` | in the rider |
    | `state` | `:980` | `.default({})`; in the rider |
    | `compressionCache` | `:983` | in the rider |
    | `sceneState` | `:1037` | in the rider |
    | `equippedOutfit` | `:1040` | in the rider |
    | `equippedInventory` | `:1047` | **NEW since `25b8fab6a`** (v4 `4fbdad186`, the Inventory carriers) |
    | `characterAvatars` | `:1050` | in the rider |
    | `compiledIdentityStacks` | `:1081` | in the rider |
    | `courierCheckpoints` | `:1094` | in the rider |
    | `commonplaceSceneCache` | `:1111` | in the rider |
    | `commonplaceRecallHistory` | `:1122` | **missing from the rider; present at `25b8fab6a` too (`:1109`)** |

  - v4's import does not strip any of these. `import-entities.ts:402` destructures away only `id`, `userId`, `messages`, `createdAt` and `updatedAt`, then `repos.chats.create` runs `_create` → `validate`.
  - v5's `ChatCreate` (`db/chats.rs:346, :491, :509`) takes all three as `Option<Value>`, so v5 imports a non-record value where v4 refuses it.
- **Fix shape:**
  - Re-apply the rider with **twelve** columns in declaration order:
    1. `sillyTavernMetadata` FIRST, before the `scenarioSource` issues (`:840` is ahead of `:940`). This means a new leading check ahead of the `scenarioSource` block in `chat_create_zod_error`.
    2. Then `pendingOutfitNotifications`, `state`, `compressionCache`.
    3. Then the Concierge enums.
    4. Then `sceneState`, `equippedOutfit`, `equippedInventory`, `characterAvatars`, `compiledIdentityStacks`, `courierCheckpoints`, `commonplaceSceneCache`, `commonplaceRecallHistory`.
  - Grow the oracle payload with rows for the three added columns. Include one combined row with `sillyTavernMetadata` and a bad `scenarioSource`, to pin the issue order.
  - Note: `commonplaceRecallHistory`'s documented shape is `string[][]`, but the schema is `z.record`, so v4 refuses an array there. Port what the schema says.
  - **Rider-scope question (minor ruling):** v4 validates at the repository `_create`, which covers every create path, but the rider checks at the import site only. P4.D327 handed the class to P4.D324's `ChatsRepository::create`. Recommend the import site, as the rider did, because the restore and the chat-create routes reach v5 through their own decoders. Record that the repository-level twin is not ported.
- **Differential:** `crates/quilltap-harness/tests/system_import_state.rs` with `harness/oracle/cases/system-import-execute.test.ts` (regenerate from the pin), plus the unit test. No fixture rebuild: the payload is built inside the TS case.
- **Files a lane would own:** `crates/quilltap-core/src/services/quilltap_import/entities.rs`, `system_import_state.rs`, `system-import-execute.test.ts`, docs.
- **Size:** S–M.

### A2: P4.D294 item 19, a `scenarioSource` text cell on export. HUMAN RULING: CONVERGE   [OPEN]

- **Source:** P4.D294 status (Tier 2 #19, "MEASURED DIFFERENT"), lane record `status-log.md:182573-182579`.
- **v4 @322a5488e:**
  - `lib/export/ndjson-writer.ts:352-353`: `const chat = await repos.chats.findById(id); if (!chat) continue;`
  - `chats.repository.ts:150-151` forwards to `base.repository.ts:247-258`, `_findById`, which wraps `this.validate(result)` in `safeQuery(…, 'Error finding entity by ID', { id }, null)`.
  - `validate` (`base.repository.ts:130-141`) logs ERROR `Data validation failed {collection: 'chats', error}` and rethrows.
  - `safeQuery` (`safe-query.ts:~54-71`) logs ERROR `Error finding entity by ID {id, error}`. It returns `null` because v4's export is NOT in a strict scope: `withStrictRepositoryFailures` is used only by `import/quilltap-import/execute.ts:435` and `preview.ts:31`.
  - **Result:** the chat is skipped, two ERROR lines are logged, and no export-level line fires.
  - The same shape holds for the export preview, `lib/export/quilltap-export-service.ts:154`.
- **v5 main:**
  - `crates/quilltap-core/src/services/qtap_export/records.rs:227` reads `let Some(chat) = chats_read::find_by_id(main, id)? else { continue };`. The call is a plain `run`, at `db/chats_read.rs:485-487`.
  - `scenario_source_cell` (`chats_read.rs:210-230`) turns the Zod refusal into `FromSqlConversionFailure(ZodRowRefusal)`. The `?` propagates it, so the whole export fails.
  - The same applies to the preview at `qtap_export/preview.rs:95`.
  - The strict scope (`qtap_export/mod.rs:364-388`) is not what fails it: `find_by_id` has no fallback at all.
- **Existing both-ways pin that must flip:**
  - `crates/quilltap-harness/tests/system_export_equivalence.rs:260-283`, the arm `Err(e) if name == "stream_chats_selected_one_scenario_text"`, which asserts that v4 skipped and v5 failed.
  - The plant is at `:87-92` (`SCENARIO_TEXT_CELL_SQL`) and its copy at `:187`. The oracle case is `harness/oracle/cases/system-export.test.ts:458`, with the plant at `:670` / `:822`.
- **Fix shape:**
  - Add a chats-read sibling, e.g. `chats_read::find_by_id_dropping_invalid`. On a `ZodRowRefusal` (or any per-row conversion refusal, the same class `run_dropping_invalid_rows` drops), it logs v4's two lines (`Data validation failed`, then `Error finding entity by ID {collection?, id, error}`) and answers `Ok(None)`.
  - A whole-query failure still propagates, so the strict-export ruling stays intact for the broken-DB case.
  - Use it at `records.rs:227` and `preview.rs:95`.
  - Delete the special `Err` arm so the case diffs as ordinary lines.
  - Add a capture check that both ERROR lines fire, and a preview row over the same plant.
  - Also check `log_find_by_id_failure`'s key set (`fallback.rs:76-85`: `collection, id, error`) against v4's `{id, error}` context. v4's `safeQuery` context for `_findById` is `{ id }` only, so v5's extra `collection` key is a known v5 field. Mirror whatever the existing `find_by_id_or_none` callers do.
- **Differential:** `system_export_equivalence.rs` with `system-export.test.ts`. Regenerating the oracle from the pin should be enough; no fixture rebuild (the plant is SQL on a per-run copy).
- **Files a lane would own:** `db/chats_read.rs`, `services/qtap_export/records.rs`, `services/qtap_export/preview.rs`, `system_export_equivalence.rs`, `system-export.test.ts` (only if a preview case is added).
- **Size:** S.

### A3: P4.D273 carried-file metadata. HUMAN RULING: CONVERGE on v4   [OPEN]

- **Source:** P4.D273 OPEN; `status-log.md:176530`.
- **v4 @322a5488e** (`lib/backup/restore/restore.ts`; the order's `:762-775` has moved):
  - `:720-725` builds the resolver with `makeCarriedStoreRowsResolver(parsed/remapped docMountBlobs, parsed/remapped docMountPoints)`. That function lives in `lib/backup/restore/carried-store-rows.ts` and is a pure id-pairing: no presence check, no disk read.
  - `:733-735` builds `carriedBlobSha256ById` from the remapped `data.docMountBlobs`.
  - **The carried arm is `:791-810`:**

    ```ts
    const fileData = stripLegacyFileRowFields(file);
    … await repos.files.create({ ...fileData, ...(carriedSha256 ? { sha256: carriedSha256 } : {}), storageKey: carriedStorageKey }, { id: file.id });
    ```

    So `mimeType` and `size` come from the archive row; only `sha256` and `storageKey` come from the blob.
  - The replay arm, `replayFile` at `:744-776`, takes the bridge's `storedMimeType`, `sizeBytes` and `sha256`. That is correct, and v5 matches it.
- **v5 main** (`crates/quilltap-core/src/services/backup/restore/orchestrator.rs`):
  - `carried_store_rows` (`:3240-3305`) runs `SELECT storedMimeType, sizeBytes, sha256 FROM doc_mount_blobs` (`:3271-3284`) and carries the result on `CarriedBlob` (`:3182-3192`).
  - `restore_one_file`'s carried arm (`:3364-3369`) writes `mime_type: c.stored_mime_type` and `size: c.size_bytes` (`:3378-3379`).
  - The doc comment at `:3311-3317` asserts the v5 reading ("describe what is actually in the store") and must change.
- **Fix shape:**
  - In the `(None, Some(c))` arm, take `mimeType` and `size` from the archive row: `s(file, "mimeType")` and `file.size`. Keep `sha256 = c.sha256.unwrap_or(archive files.sha256)` and the rebuilt `storage_key`.
  - Drop `stored_mime_type` and `size_bytes` from `CarriedBlob` and the SELECT. Keep the presence read; it sits inside the 2026-07-26 ruled slot.
  - Rewrite the doc comment.
  - Decide how an absent `size` on the archive row decodes. `FileCreate.size` is an `f64`, while v4 would let Zod refuse it. Measure before choosing.
- **Making it visible (needs a plant):**
  - `harness/oracle/fixtures/derive-restore-archive-store-binding.py:85-101` deliberately makes the carried row describe its blob exactly. Line `:86` says "The row describes its blob exactly", setting `mimeType = blob.storedMimeType` and `size = blob.sizeBytes` (`:92-93`).
  - To make the difference visible, write a deliberately different `mimeType` (e.g. `application/octet-stream`) and `size` (e.g. blob size + 7) on the carried row. Either change that row, or add a second carried row in this archive or a new `derive-restore-archive-carried-meta.py`.
  - Re-derive the committed zip; the derive is deterministic.
  - Regenerate `system-restore.test.ts` from the pin.
  - Assert the two cells by name before normalization. Precedent: `system_restore_state.rs:3339-3355` (the bug-117 sha pre-normalization compare), because the table diff normalizes composite cells.
  - Red-first on current main.
- **Related finding, recorded (not covered by the ruling; ask whether it converges too):** v5's carried arm still reads the bytes from disk.
  - `orchestrator.rs:1675-1696`: `replay` calls `get_file_from_extracted_backup` first. If the bytes are missing, it pushes `File not found in backup: <name>` and writes NO row, even when `carried` is `Some`.
  - v4's carried arm never reads the disk (`:796-809`).
  - This is invisible on every committed archive, because the derive writes the bytes (`derive-…-store-binding.py:155`).
  - Converging means: when `carried` is `Some`, skip the disk read.
- **Differential:** `system_restore_state.rs` (+ `system_restore_equivalence.rs` re-run) with `harness/oracle/cases/system-restore.test.ts`. **Fixture: one derived archive re-derived (or a new one). No builder rebuild.**
- **Files a lane would own:** `services/backup/restore/orchestrator.rs` (files phase + helpers), the derive script and its zip, `system_restore_state.rs`, `system-restore.test.ts` (only if a case is added).
- **Size:** S–M.

### A4a: P4.D273's numeric store pointer and SameValueZero   [NEEDS-RULING; recommend NO-ACTION, recorded unreachable]

- **v5:** `services/backup/restore/store_claims.rs`:
  - `make_store_claim_map` (`:136-149`) skips any store row whose `id` is not a string.
  - `claim_value` (`:201-215`) answers `not-carried` for a truthy non-string pointer.
  - The unit test `claim_value_reads_raw_pointers` (`:378-392`) pins `7` → `NotCarried`.
- **v4** (`lib/backup/restore/store-claims.ts`, `makeStoreClaimMap`): `new Map(carriedStores.map(s => [s.id, s]))`, then `carried.get(pointer)`. A pointer `7` binds only when an archive store row has numeric `id: 7`. A string `"7"` row does not match (SameValueZero).
- **Reachability:** the archive's `doc_mount_points.id` is a TEXT UUID column. A numeric id needs a hand-edited archive, and that mount row would then fail restore 22a (`UUIDSchema`). The entity would bind to a store that never lands.
- **Fix shape, if converging:** key `carried` / `holders` by a scalar key (String | Number normalized, so -0 matches 0), and pass the raw `Value` through `claim_value`. Add tier-1 rows: numeric pointer with a numeric store, numeric pointer with a string store, and `-0` with `0`.
- **Differential:** `crates/quilltap-harness/tests/store_claims_equivalence.rs` with `harness/oracle/cases/store-claims.ts`.
- **Files:** `store_claims.rs`.
- **Size:** S.

### A4b: P4.D273's `carried_store_rows` runs twice per non-deferred file (hoist)   [OPEN]

- **v5:** `orchestrator.rs`:
  - The first pass calls it at `:1704-1705` (the `deferrable` test).
  - `replay` calls it again at `:1681` for the same index.
  - Deferred files call it at `:1681` only.
- **Fix shape:**
  - Before the loops, build `let carried: Vec<Option<CarriedBlob>> = data.files.iter().enumerate().map(|(i, f)| carried_store_rows(mount, data, &extracted.data, extracted.data.files.get(i).unwrap_or(f))).collect();`.
  - `deferrable` then tests `carried[i].is_none()`, and `replay` takes `carried[i].as_ref()`.
  - This preserves behaviour. The bridges mint fresh blob ids, so no replay between the two calls can create the archive's blob id. Write that reasoning into the comment.
  - It pairs naturally with A3, which edits the same helper.
- **Differential:** unit-only (pure refactor). Re-run `system_restore_state`.
- **Size:** S.

### A4c: P4.D273's `js_truthy` copies across `quilltap-core` (DRY)   [OPEN; the count is far past nine]

There are **30 JSON-`Value` JS-truthiness functions in `quilltap-core`**: 26 private copies, 2 one-line delegators and 2 `pub` homes already in use. The `cli` crate has 2 more. Every copy answers identically for any `serde_json::Value`. A JSON `Number` is never NaN, so the `!f.is_nan()` and `unwrap_or(true/false)` variants agree.

| Kind | Sites (file:line) |
|---|---|
| `pub` homes | `inventory/types.rs:187` `pub fn js_truthy(Option<&Value>)`; `api/system_qtap.rs:380` `pub fn js_truthy`; `model/provider_error.rs:200` `pub(crate) fn truthy(&Value)`; `services/turn_transcript.rs:567` `pub(crate) fn is_js_truthy` |
| Delegators | `tools/inventory_read.rs:214` → inventory; `services/quilltap_import/message_id_remap.rs:126` → system_qtap |
| Private, `Option<&Value>` | `chat_timestamp.rs:750`; `default_system_prompt.rs:45`; `chat_activity.rs:133`; `admission/decide.rs:97`; `admission/fold.rs:235`; `api/help_chats.rs:89`; `services/home.rs:174`; `services/answer_confirmation.rs:262`; `services/memory_maintenance_job.rs:111`; `services/regenerate_swipe.rs:272`; `services/memory_maintenance/run.rs:752`; `services/help_chat/orchestrator.rs:242`; `services/backup/restore/legacy_migrations.rs:51`; `services/backup/restore/store_claims.rs:111`; `services/backup/uuid_remap.rs:59`; `services/backup/restore/orchestrator.rs:4655` |
| Private, `&Value` | `memory_extraction/parse.rs:173`; `model/tool_wire/text_parsers.rs:115`; `model/request_builder/chat_completions.rs:1146`; `api/roleplay_templates.rs:48`; `api/salon.rs:686`; `api/settings.rs:1191`; `services/queue_service.rs:631`; `services/sillytavern.rs:491` |
| Not the same function, leave alone | `system_prompt.rs:168`, `project_roster_access.rs:49`, `services/data_dir.rs:177`, `services/prospero_notifications.rs:1038`, `services/scenario_selection.rs:182`, `services/admission/apply.rs:68` (`Option<&str>` / `Option<String>` → `Option<&str>` filters); `services/llm_errors.rs:92` (`Option<i64>`); `turn_transcript.rs:562/:583` (column-specific predicates) |
| CLI | `quilltap-cli/src/nodefmt.rs:246` (`pub`), `db_characters.rs:842` |

- **Fix shape:**
  - Create one home, e.g. `crate::jsvalue::{js_truthy(Option<&Value>), js_truthy_value(&Value)}`, next to the existing `jsnum.rs` / `jsstr.rs`. Alternatively, promote `inventory::types::js_truthy`.
  - Repoint all 30, deleting the bodies and keeping the call sites.
  - Add an equivalence unit test over every JSON shape (null, bools, 0, -0, 1, `""`, `"0"`, `[]`, `{}`).
  - This is a pure refactor touching about 28 files across many slices. Make it its own lane, or place it last in the round to avoid ownership collisions.
  - The `quilltap-cli` copies can use the core home if it is `pub`.
- **Differential:** unit-only (behaviour-identical refactor). Run the workspace gate.
- **Size:** M (wide but mechanical).

---

## B. Per-order items

### P4.D272

#### D272-RE: a non-string preserved memory id (`id: 5`) under `preserveIds`   [OPEN]
- **Source:** P4.D272 Tier 3 #18 / R-E. Measured in `status-log.md:176455`: v4 refuses (imported 8 / skipped 8), v5 mints (9 / 7). No committed pin exists.
- **v5:** `services/quilltap_import/memories.rs`:
  - `MemoryIdMap::new_id` (`:179-188`), under `preserve_ids`, calls `super::mint_or_preserve(options, &super::id_of(memory))`.
  - `id_of` reads `""` for a number, and `mint_or_preserve` (`mod.rs:399-406`) then mints.
  - The divergence is documented in the module header at `:58-62`.
- **v4 @pin:** `lib/import/quilltap-import/import-entities.ts`:
  - `:483`: `memoryIdMap.set(memory.id, options.preserveIds ? memory.id : randomUUID())`.
  - `:561-563`: `newMemoryId = memoryIdMap.get(memory.id) ?? memory.id`, `createData = { ...memoryData, id: memory.id }`, `createOptions = { id: newMemoryId }`.
  - The numeric 5 reaches `MemorySchema` and is refused with `Failed to import memory memoryId=5 … expected string, received number`.
- **Fix shape:** two options.
  - (a) Converge. Under `preserve_ids`, a present non-string, non-null id passes through as a raw `Value` into the create, which then fails the memory's validation with v4's ZodError. Concretely: a pre-validation in `import_memories` that refuses a non-string `id` with v4's `invalid_type` issue at `["id"]` before the create.
  - (b) Record with the committed both-ways pin the status line asks for. This needs only the oracle row.
  - The status line wants at least (b). Recommend (a): it is small.
- **Differential:** `system_import_state.rs` with `system-import-execute.test.ts`. Add one row with `{…, id: 5}` in the preserve case.
- **Files:** `quilltap_import/memories.rs`, `system_import_state.rs`, `system-import-execute.test.ts`.
- **Size:** S.

#### D272-16/17: the backup / restore carriage and the reconcile call   [CLOSED]
Both landed in siblings: P4.D273 and P4.D274, unified 2026-10-12. The `RECONCILE_PENDING_UNION` carve was lifted.

#### D272-19 (R-G): no heal of dangling seed `relatedMemoryIds`   [NO-ACTION]
v4 ships no heal either. This is documented at `memories.rs:53-56`.

#### D272-20: a serde-vs-Zod row for a non-string chat watermark   [CLOSED, obsolete]
The column `otherExtractionWatermarkMessageId` was DROPPED by the `83334b9a5` memory refactor (P4.D288). It has 0 occurrences in `fresh_schema.json`.

### P4.D273

#### D273-17: `restore_dup_store_id_new_account` (Tier 2 #17, DEFERRED)   [OPEN]
- **v5:** only the `replace` case exists:
  - `system_restore_state.rs:1371` (archive map), `:4528` (case list), `:4681` (the `DUP_STORE_ID_FIRST_ROW` holder table).
  - `harness/oracle/cases/system-restore.test.ts:624`.
- **Fix shape:**
  - Add `{ name: 'restore_dup_store_id_new_account', archive: 'restore-archive-dup-store-id.zip', mode new-account }` to the TS list.
  - Add the Rust archive map row, the case-list row and the `DUP_STORE_ID_FIRST_ROW` holder row. Measure whether v4 binds the first or the last row in `new-account` (remapped ids).
  - Regenerate from the pin.
- **Differential:** `system_restore_state.rs` with `system-restore.test.ts`. No fixture change: the archive already exists.
- **Size:** S.

#### D273-21 (R-L): `ensure_official_store`'s missing log lines   [OPEN]
- **v5:** `db/ensure_official_store.rs:86-173` logs nothing.
- **v4** (`lib/mount-index/ensure-official-store.ts` @pin):
  - `:84` WARN `ensure<Label>OfficialStore: <label> not found {<idKey>}` (v5 returns `Ok(None)` silently at `:96-98`).
  - `:96` INFO `<Label> officialMountPointId points to a missing mount point; will heal {<idKey>, staleMountPointId}` (v5 falls through silently at `:114`).
  - `:157` INFO `Created <label>-official document store {<idKey>, mountPointId, storeName}` (v5 at `:167-172`, silent).
  - `:118` INFO `Adopted existing linked store…` (only with D273-22).
- **Two further divergences in the same function, found by this survey:**
  - **The name.** v4 `:127` uses `(entityName || 'Untitled').trim()`. A whitespace-only name is truthy, so it trims to `""` and the desired name is the bare prefix. v5 `:122-129` uses Rust `str::trim()` and maps an empty result to `"Untitled"`, which diverges. It should use `js_trim` and apply `'Untitled'` only to a falsy (empty) input.
  - **The truncation.** v4 uses `.slice(0, 200)` (UTF-16 units). v5 uses `truncate_chars(…, 200)`. Confirm whether `truncate_chars` counts code points or UTF-16 units.
- **Fix shape:** add the three lines, using `StoreEntity`'s label / idKey (a trait const). Fix the name expression. About 13 call sites (`grep -c` = 13 non-comment references) inherit the lines; census any captured-log families that now see new INFO lines.
- **Differential:** no direct family covers `ensure_official_store`. The closest are `mount_case_repair_equivalence.rs` / `store_names_equivalence.rs` (which reference it) and `system_restore_state` (22a-i re-provision). The lines would be capture-pinned through `system_restore_state`'s census, which needs both lists, `RESTORE_TS_MESSAGES` TS + Rust (memory note: the restore census is a whitelist), plus a small new tier-2 over v4's real `ensureProjectOfficialStore` for the name edge.
- **Files:** `db/ensure_official_store.rs`, restore census lists, a new or extended case.
- **Size:** M.

#### D273-22: step 2 of `ensureOfficialStore` (adopt a hand-linked store) deferred   [NEEDS-RULING]
- **v5:** `ensure_official_store.rs:117-119` is a comment only; the module header is at `:16, :27`.
- **v4 @pin:** `ensure-official-store.ts:101-123` finds links, filters to eligible `database` / `documents` stores, picks own-name first or else the first eligible, and does a raw FK write plus INFO.
- **Reachability:** v5 says the corpora never have a pre-existing link at 22a-i (links restore at 22h). Other callers, such as the boot backfill and entity create after a manual link, could reach it.
- **Ruling needed:** port it (M, needs a tier-2 with a planted link) or keep deferred.
- **Size:** M.

#### D273-23: restore on a DEGRADED mount index logs no summary INFO; v4's `findAll` at 22a would throw   [OPEN, measurement only]
- Not measured. This overlaps dogfood #156 (`with_both_conns` on a degraded mount index), which is another slice. Recommend folding it into the #156 lane as one degraded-clone `system_restore_state` row.
- **Size:** S (measure) / M (if divergent).

#### D273-24: `next_unique_mount_point_name` `js_trim`   [CLOSED]
Converged at P4.D274: `ensure_official_store.rs:57-74` uses `js_trim` and is pinned by `store_names_equivalence`.

#### D273 HANDOFF 23a reconcile call   [CLOSED]
Landed at union on 2026-10-12.

#### A3, A4a/b/c
Covered in section A above.

### P4.D274

#### D274-a: `reconcile_vault_names_after_concurrent_deletes`' read-failure arm re-scans   [OPEN]
- **v5:** `db/store_name_reconcile.rs:257-331`; the failure arm is `:274-290`.
  - On `Err`, it calls `read_store_name_rows(mount_conn)` AGAIN just to learn which read failed (`:279`). That re-emits every row-drop line.
  - Each subsequent id then runs the whole reconcile.
  - This is state-safe and unreachable in the oracle.
- **Fix shape:** split the match into `(Err(e), _) => "doc_mount_points"` and `(Ok(_), Err(e)) => "characters"`. Drop the re-read.
- **Differential:** unit-only. The arm is unreachable through v4's oracle; pin the line count with a planted failing read in an in-crate test.
- **Files:** `store_name_reconcile.rs`.
- **Size:** S.

#### D274-b: `BUILTIN_REPROVISION_FREE_NAME` uses the raw `find_all_names()`   [NO-ACTION; add a why-comment]
- **v5:** `services/builtin_mounts.rs:378-397`, a v5-only path.
- **Why keep it:** the UNIQUE NOCASE index constrains RAW rows. A row the Zod read drops still holds its name, so the raw set is the correct one for this insert. Switching it to the Zod-dropping read would re-introduce the `UNIQUE constraint failed` boot FATAL.
- **Fix:** a comment.
- **Size:** S.

#### D274-c: `doc_mount_point_issues` treats `lastScanError` / `conversionError` as non-nullable   [OPEN, comment or flag]
- **v5:** `store_name_reconcile.rs:540-575`; `string_issues(bag, "lastScanError", false, false, …)` at `:559` and `conversionError` at `:566`.
- This holds only because `read_bags` (`:386-416`) omits NULL cells via `hydrate`. v4 `mount-index.types.ts`: `lastScanError: z.string().nullable().optional()`, `conversionError` likewise.
- **Fix shape:** give `string_issues` a `nullable` flag like `uuid_issues`'s (`:436`), pass `true` for both, and add a unit test with an explicit `Value::Null` bag.
- **Size:** S.

#### D274-d: the drafted CLAUDE.md "document-store names" standing rule   [NEEDS-RULING, setupphase / human]
The text is at `status-log.md:175655`. It is not in `CLAUDE.md` (grep found nothing). This is docs only.

#### D274 Tier 3   [NO-ACTION]
- The job-child NO-PORT stands.
- v4's stale `mount-index-case-repair.ts:12-15` header is a v4 filing candidate.
- #156 is cross-slice.

### P4.D284 (and the P4.D264 items it carries)

#### D284-banked: a restore-side tier-2 row for 22f-bis's `folder`   [OPEN]
- **v5:** restore 22f-bis's constructor uses `restore::archive::carried_wardrobe_folder`, which is unit-pinned. There is no `system_restore_state` row.
- **v4 @pin** writes `wardrobeItems: []`, so only a crafted archive reaches it.
- **Fix shape:**
  - Add a new `derive-restore-archive-wardrobe-folders.py` (legacy `wardrobeItems` rows: filed, `../x`, `""`, `5`, `["Formal"]`).
  - Add the case to `system-restore.test.ts` and to `system_restore_state.rs`'s per-case tables (`:359-:496`, `:1252` per the lane record; re-locate on main).
  - Regenerate from the pin.
- **Fixture:** a new derived archive.
- **Size:** M.

#### D284-edge: cycle versus non-string `folder` precedence (recorded, not ruled)   [NO-ACTION; recorded edge]
v4 resolves the mount and runs `assertNoCycles` before `safeFolder`. v5 classifies the folder first.

#### D264 #3, a second `replace` restore leaking `chat_settings.userId` UNIQUE   [NO-ACTION; v4-shared filing candidate]

#### D264 #8, `archiveFileId` / `archivedAvatarFileId` not remapped on new-account   [NO-ACTION; v4-shared]
Still true at the pin: neither name appears in v4 `lib/backup/` or v5 `uuid_remap.rs`. A filing candidate.

#### D264 #4, `mapping_object`'s integer-key order; item 12's two non-behaviours; 22f-ter silent without a mount index   [NO-ACTION; comparand-only / by construction]

### P4.D293

#### D293-21: `memoryIndex.reloadAll()` named no-op   [CLOSED]
The comment is in place at `orchestrator.rs:1027`.

#### D293-22 (R-AE): new-account restore does not remap `perCharacterCapOverrides`   [NO-ACTION; v4 filing candidate]
Still v4-faithful at the pin: no `perCharacterCapOverrides` hit in v4 `lib/backup` / `lib/import` or v5 `services/backup`. Not filed. `gh issue list` shows one unrelated issue only. Draft the issue for the human.

### P4.D294

#### D294-19
See A2. OPEN, ruled CONVERGE.

#### D294-20: the forced refit after import   [CLOSED]
Landed at unification.

#### D294-22: v4's lazy cursor-table creation on export   [NO-ACTION]
v5 provisions `memory_extraction_cursors` (it is in `fresh_schema.json`).

#### D294-23: the SillyTavern import's extraction enqueue   [NO-ACTION for this slice; cross-slice]
v4 `lib/import/sillytavern-import-service.ts:610-645` (`queueMemoryExtractionJobs`, INFO `Queueing per-turn memory extraction jobs`). v5's `services/sillytavern.rs` is the card import/export only, and no v5 site enqueues on a SillyTavern chat import. Flag it to the survey that owns imports and jobs.

### P4.D317

#### D317-RK (Tier 3 #28): the three R-9 filing candidates   [NO-ACTION; human files]
The three candidates are listed at `status-log.md:188967-188980`: (a) phantom character mint, (b) a non-array slot throw on new-account, (c) the never-merge hole. None is filed on GitHub.

#### D317-rider
See A1.

#### D317-29 (R-2) and P4.D327 Tier 3 #26 (R-3): V8 `JSON.parse` acceptance classes   [OPEN, low; recommend keep recorded]
- **v5:** `chat/character_keyed_fields.rs:33-35, :90` uses `serde_json::from_str`. Three classes are refused where V8 accepts: `1e400`, lone surrogates, and nesting > 128.
- Only `jsstr::v8_json_parse_message` (`jsstr.rs:202`, the error TEXT) exists. No value-level V8 parser exists, so neither class closes.
- **Fix:** a value-level V8 `JSON.parse` (M–L), or keep both recorded.
- **Recommendation:** NO-ACTION this round.

#### D317-30: the `.qtap` NDJSON reader has NONE of v4's six log lines   [OPEN]
- **v5:** `services/quilltap_import/ndjson.rs` has no `tracing` call at all. The unknown-kind arm `_ => {}` is at `:629-630`, and its comment ("v4 warns and skips") is untrue of v5.
- **v4 @pin** (`lib/import/quilltap-import-stream.ts`):
  - `:175` WARN `Skipping non-object NDJSON record`
  - `:184` WARN `Second envelope line ignored — only the first is used`
  - `:248` WARN `wardrobe_item record with unknown parent character — skipping`
  - `:262` WARN `character_plugin_data with unknown parent character — skipping`
  - `:284` WARN `chat_message with unknown parent chat — skipping`
  - `:457` WARN `Unknown NDJSON record kind — skipping {kind: String(kind)}`
  - `:489-492` INFO `Some NDJSON records were skipped due to unknown kind {count}`
- **Fix shape:** port the lines with v4's keys. Read v4's `:240-290` for the exact contexts. Count unknown kinds and emit the INFO after the loop.
- **Differential:** `qtap_import_equivalence.rs`. A hand-built NDJSON pair, following the precedent of `qtap-import-wardrobe-folders.qtap`, with a bad record of each kind, comparing v4's and v5's captured lines. Alternatively, a `system_import_state` capture block.
- **Files:** `ndjson.rs`, `qtap_import_equivalence.rs`, the oracle `qtap-import.ts`, a new committed `.qtap` fixture (hand-built, no rebuild).
- **Size:** S–M.

#### D317-31: v4's `Raw query failed` ERROR and the wipe's `Skipping table truncate` DEBUG   [NO-ACTION; recorded]
- v5 `services/delete_all.rs:291, :296` uses `let _ = …execute(DELETE…)`.
- v4 `delete-service.ts:68` DEBUG and `backends/sqlite/backend.ts:839` ERROR.
- Unreachable on a booted instance. The DEBUG alone would be an S port if wanted.

#### D317-32: R-10's import-side `unreadable` WARN   [CLOSED]
The lane measured R-10 unreachable on both sides (status line), so there is no divergence.

#### D317-33: the never-merge hole's second arm   [NO-ACTION]
Covered by the repository per-row WARN family. The first arm is pinned (`execute_outfit_history_never_merge_hole`).

### P4.D327

#### D327: "OPEN: none of its own"
The note that v4 `cc5f2347c` carries the Log in a logical backup is drift for the next catch-up, not this round.

Tier 3:
- **#26:** the V8 JSON class; see D317-29.
- **#27:** a non-boolean `canManageInventory`. NO-ACTION (the class of character rows with no Zod twin).
- **#28:** R-13, the import leaving an object's `imageFileId` unrepointed. NO-ACTION; a v4 filing candidate.
- **#29:** a non-string `relativePath` link. NO-ACTION; recorded.
- **HANDOFF to P4.D324** (the chat-create `JsonSchema` twin): absorbed into A1.

---

## C. Harness families and fixtures by item

| Item | Rust family | Oracle case | Fixture |
|---|---|---|---|
| A1 rider (12 columns) | `system_import_state.rs` (+ unit test) | `system-import-execute.test.ts` | none (payload in TS); regenerate from `322a5488e` |
| A2 `scenarioSource` text on export | `system_export_equivalence.rs` | `system-export.test.ts` | none (SQL plant) |
| A3 carried metadata (+ disk-read note) | `system_restore_state.rs`, `system_restore_equivalence.rs` | `system-restore.test.ts` | **re-derive** `derive-restore-archive-store-binding.py` → its zip (or a new derive) |
| A4a SameValueZero | `store_claims_equivalence.rs` | `store-claims.ts` | none |
| A4b hoist | unit / re-run `system_restore_state` | none | none |
| A4c `js_truthy` | unit + workspace | none | none |
| D272-RE | `system_import_state.rs` | `system-import-execute.test.ts` | none |
| D273-17 | `system_restore_state.rs` | `system-restore.test.ts` | none (archive exists) |
| D273-21 R-L lines | `system_restore_state.rs` census (+ both `RESTORE_TS_MESSAGES` lists) + a new small tier-2 for the name edge | `system-restore.test.ts` / new case | none |
| D273-22 adopt | new tier-2 (planted link) | new case | maybe a planted fixture copy |
| D273-23 degraded restore | `system_restore_state.rs` (degraded clone) | `system-restore.test.ts` | a planted copy |
| D274-a/c | unit (in crate) | none | none |
| D284-banked | `system_restore_state.rs` | `system-restore.test.ts` | **new derived archive** |
| D317-30 NDJSON lines | `qtap_import_equivalence.rs` | `qtap-import.ts` | new hand-built `.qtap` (committed) |

No builder rebuild (`build-*.ts`) is needed by any item. Two items need derived-archive work (A3, D284-banked) and one needs a new hand-built `.qtap` (D317-30).

## D. Status index

| # | Item | Status |
|---|---|---|
| 1 | A1 rider | OPEN |
| 2 | A2 D294-19 | OPEN (ruled) |
| 3 | A3 carried metadata | OPEN (ruled) |
| 4 | A3-note disk read on the carried arm | NEEDS-RULING |
| 5 | A4a SameValueZero | NEEDS-RULING |
| 6 | A4b hoist | OPEN |
| 7 | A4c `js_truthy` | OPEN |
| 8 | D272-RE | OPEN |
| 9 | D272-16/17 | CLOSED |
| 10 | D272-19 | NO-ACTION |
| 11 | D272-20 | CLOSED |
| 12 | D273-17 | OPEN |
| 13 | D273-21 | OPEN |
| 14 | D273-22 | NEEDS-RULING |
| 15 | D273-23 | OPEN |
| 16 | D273-24 | CLOSED |
| 17 | D273-HANDOFF | CLOSED |
| 18 | D274-a | OPEN |
| 19 | D274-b | NO-ACTION |
| 20 | D274-c | OPEN |
| 21 | D274-d | NEEDS-RULING |
| 22 | D274 Tier 3 | NO-ACTION |
| 23 | D284-banked | OPEN |
| 24 | D284-edge | NO-ACTION |
| 25 | D264 #3 / #8 / #4 etc. | NO-ACTION |
| 26 | D293-21 | CLOSED |
| 27 | D293-22 | NO-ACTION |
| 28 | D294-22 | NO-ACTION |
| 29 | D294-23 | NO-ACTION (cross-slice flag) |
| 30 | D317-RK | NO-ACTION |
| 31 | D317-29 / D327-26 | OPEN (low; recommend keep recorded) |
| 32 | D317-30 | OPEN |
| 33 | D317-31 | NO-ACTION |
| 34 | D317-32 | CLOSED |
| 35 | D317-33 | NO-ACTION |
| 36 | D327 Tier 3 #27–29 | NO-ACTION |

Counts by item, with D294-20 counted as CLOSED: OPEN 14 (the ruled ones included), CLOSED 7, NEEDS-RULING 4, NO-ACTION 12. Small v4-faithful items are grouped in the index.
