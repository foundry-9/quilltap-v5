# Survey — P4.149: the repository-fallback class, round 3

v5 `main` `6c3a3c635`; v4 `~/source/quilltap-server` `main` `07b8f0209`
(the oracle baseline; tree clean). Read-only survey: nothing was built, run or
regenerated. Line numbers are today's.

## Headline findings (read these first)

1. **Item 1's premise needs correcting: v4's `Error deleting entity` is NOT a
   fallback line.** The base `_delete` (`lib/database/repositories/
   base.repository.ts:427-447`) is a **3-argument (RETHROW-mode) `safeQuery`**:
   it logs ERROR `Error deleting entity {collection, id, error}` and
   **rethrows**. So the shape is "log and propagate" (with `strictFailures:
   true` appended inside the strict scope, `safe-query.ts:57-71`), not "log and
   answer `false`". The fallback lives one layer up, in the repository
   methods that wrap it with a 4-argument `safeQuery` (the three
   `chat-informs` bulk deletes answer `0`). It belongs to a family of three
   unported base rethrow lines (`Error creating entity`, `Error updating
   entity`, `Error deleting entity`) plus their success INFO / not-found WARN
   lines. Item 7 hits the `update` / `create` members of the same family.
2. **Item 3 is CLOSED** (phantom on today's `main`). P4.142 converted the
   overlay batch reads and the two error arms. The 2026-10-03 walk ran the
   same plant live: rows F1 (`listChats` 200 with drops) and F2 (`chatGet` 500
   `Failed to fetch chat`) both PASS. Nothing to order.
3. **Item 6's `Error finding links by project ID` is UNREACHABLE in v4.**
   `findByProjectId`'s own fallback `safeQuery` wraps `findByFilter`, which is
   itself a fallback. The reachable line is `Error finding entities by filter
   {collection: project_doc_mount_links, error}`, and that line already has a
   home (`find_by_filter_or_empty`). **No new home or `HOME_MESSAGES` literal
   is needed.**
4. **Item 2 is a per-ROW drop in v4, not a list fallback.** `findByUserId`
   goes through `findByFilter`, which runs `validateSafe` on each row: the
   corrupt row is dropped with ERROR `Data validation failed` + WARN `Safe
   validation failed`, and the list answers without it. v5 fails the whole
   read. The fix sits in `db/connection_profiles.rs`, and **42 production
   call sites** move with it (backup, export and import among them — see the
   ruling question).
5. **`fallback_engine_equivalence.rs` is unrelated.** It is the LLM failover
   engine (`lib/llm/fallback/`), not the repository fallbacks. It is not a
   recipe for this lane.

---

## Item 1 — a `delete` shape for v4's `Error deleting entity`

**1. The item, quoted.** `work-orders/p4.d249-standing-informs-server-schema-ensure-carriers.md:5`
(Unification): "v4's `Error deleting entity` line (the base `_delete`'s
`safeQuery`) has no v5 analog — needs a `delete` shape in `db::fallback`
(pre-existing since P4.D205)". The lane record `status-log.md:164746-164748`
adds that `_delete` "logs it before rethrowing to `deletePendingByBatch`'s
outer catch". `phase-4.md:7337` repeats it.

**2. v5 today.**
- **No v5 repository `delete` logs anything.** 40 `pub fn delete(` in
  `crates/quilltap-core/src/db/*.rs`, every one a bare `DELETE … WHERE id =
  ?1` returning `Result<bool, DbError>`; zero `tracing::` calls in any body
  (measured per fn). The only exception is `chats.rs:1370`, which carries the
  v5 cascade WARN below. `grep "Entity deleted\|Entity not found for
  deletion\|Error deleting entity"` → **0 hits** in `crates/`.
- The three inform bulk deletes in `db/chat_informs.rs` do **not** call
  `self.delete`. They inline `DELETE FROM chat_informs WHERE id = ?1` per row:
  `delete_pending_by_batch` `:606-626`, `delete_pending_for_participant`
  `:632-655`, `delete_by_chat_id` `:661-670`. All propagate with `?`.
- Their outer lines, as v5 writes them today:
  - **batch** — `api/chat_informs.rs:550-563`: `tracing::error!(batch_id =
    %batch, error = %e, "Error deleting pending informs by batch")`. Module
    target (not `quilltap::db`), no `collection`, snake `batch_id`, and the
    `sqlite error:` prefix via `%e`. The answer is `0`, which matches v4. The
    pin `:918-919` asserts only `starts_with("ERROR")`.
  - **participant** — `api/chat_cast.rs:650-671`: on `Err`, WARN `[Chats v1]
    Could not drop pending informs for removed seat`. In v4 this is
    UNREACHABLE on a database failure (below).
  - **chat id** — `db/chats.rs:1410-1417`: WARN `[Chats] Failed to delete chat
    informs for chat`. This arm has **no v4 counterpart**: no v4 production
    code calls `chatInforms.deleteByChatId`, because v4's FK cascades
    (measured: no caller outside the repository and tests).

**3. v4 at `07b8f0209`.**
- `base.repository.ts:427-447` `_delete`, a rethrow `safeQuery`:
  - success → INFO `Entity deleted {collection, id}`;
  - zero rows → WARN `Entity not found for deletion {collection, id}` →
    `false`;
  - throw → ERROR `Error deleting entity {collection, id, error}` (`collection`
    first, injected by `base.repository.ts:101`), then **rethrow**.
- The siblings in the same family: `_create` (`:354-379`, ERROR `Error
  creating entity` via `createErrorMessage()`, INFO `Entity created`) and
  `_update` (`:383-420`, ERROR `Error updating entity {collection, id}`, WARN
  `Entity not found for update`).
- **36 v4 repository files call `this._delete`** (40 call sites). How each
  wraps it:
  - **Pass-through** (`async delete(id) { return this._delete(id) }`): only
    the base line, then the throw reaches the caller. The 14 files:
    `doc-mount-files`, `chat-documents`, `group-character-members`,
    `help-doc-chunks`, `project-doc-mount-links`, `doc-mount-points`,
    `group-doc-mount-links`, `doc-mount-folders`, `help-docs`,
    `conversation-chunks`, `doc-mount-file-links` (plain `delete`),
    `conversation-annotations`, `chat-informs`, `doc-mount-chunks`,
    `doc-mount-documents`.
  - **Wrapped in a RETHROW `safeQuery`**: two ERRORs in order (`Error
    deleting entity`, then the outer), then the throw. The outer messages:

    | Repository | Outer line |
    |---|---|
    | embedding-profiles | `Error deleting embedding profile {profileId}` |
    | background-jobs | `Error deleting background job {jobId}` |
    | llm-logs | `Error deleting LLM log {logId}` |
    | character-plugin-data | `Error deleting character plugin data {entryId}` |
    | chat-settings | `Error deleting chat settings {chatSettingsId}` |
    | connection-profiles | `Error deleting connection profile {profileId}` |
    | tags | `Error deleting tag {tagId}` |
    | store-backed (projects, groups) | `` `Error deleting ${label.toLowerCase()}` `` `{[idLogKey]}` (`store-backed.repository.ts:219-233`) |
    | memories | `Error deleting memory {memoryId}` |
    | provider-models | `Error deleting provider model {modelId}` |
    | plugin-config | `Error deleting plugin config {pluginConfigId}` |
    | folders | `Error deleting folder {folderId}` |
    | users | `Error deleting user {userId}` |
    | prompt-templates | `Error deleting prompt template {templateId}` |
    | files | `Error deleting file {fileId}` |
    | roleplay-templates | `Error deleting roleplay template {templateId}` |
    | characters | `Error deleting character {characterId}` |
    | image-profiles | `Error deleting image profile {profileId}` |
    | chats | `chats.repository.ts:310-` (message not read past `:375`) |

  - **Wrapped in a FALLBACK `safeQuery`** (`…, false`), so the rethrow is
    caught: `terminal-sessions` (`Error deleting terminal session
    {sessionId}`) and `text-replacement-rules` (`Error deleting text
    replacement rule {ruleId}`).
- **v4 deletes that do NOT go through `_delete`:**
  - `embedding-status.repository.ts:320-334`: own `deleteOne`, rethrow
    `Error deleting embedding status {id}`;
  - `tfidf-vocabulary.repository.ts:185-202`: own `deleteOne`, rethrow
    `Error deleting TF-IDF vocabulary {context, id}`, plus INFO;
  - `connection-profiles.repository.ts:370-391` `deleteApiKey`: rethrow
    `Error deleting API key {keyId}` + WARN/INFO;
  - `wardrobe.repository.ts:419-` (vault delete, throws on no mount);
  - `doc-mount-blobs.repository.ts:322-334`: **a non-`safeQuery` FALLBACK**,
    WARN `Failed to delete blob {id, error}` → `false`. v5's
    `doc_mount_blobs.rs:302` propagates.
- **The inform deletes:** `chat-informs.repository.ts:271-291`
  (`deletePendingByBatch`), `:294-313` (`…ForParticipant`) and `:317-331`
  (`deleteByChatId`) are all **4-argument FALLBACK** `safeQuery`s answering
  `0`. Each calls `this.delete(row.id)`, which is a pass-through `_delete`.
  So a failed row delete logs:
  1. ERROR `Error deleting entity {collection: 'chat_informs', id, error}`;
  2. ERROR `Error deleting pending informs by batch {collection, batchId,
     error}` (or `… for participant {collection, chatId, participantId,
     error}`);
  3. the method answers `0`.

  Consequences:
  - v4's route-level catch at `participants.ts:625-638` (`Could not drop
    pending informs for removed seat`) is **unreachable** on a database
    failure, and the DEBUG `Pending informs dropped with removed seat
    droppedInforms: 0` fires instead.
  - On success, v4 logs one INFO `Entity deleted` per row; v5 logs none.
- **Prior stance on the success lines.** `status-log.md:757-763` (the
  prompt-templates lane) recorded v4's generic per-op lines (`Entity
  created`, …) as "not ported, recorded: v5's `db::` layer has no logging
  anywhere by design". The success INFO/WARN half is therefore a **ruling
  question**, not a defect.

**4. Divergence.** A failed v5 entity delete logs nothing at the repository
layer: v4's ERROR `Error deleting entity` (and, for 19 repositories, an outer
`Error deleting <x>`) is absent. The three inform bulk deletes also log the
outer line in the wrong target and fields (batch), log a v4-unreachable WARN
(participant), or log a v5-only WARN (chat id, legitimately v5-only).

**5. Predicted hunks.**
- **`db/fallback.rs`: a RETHROW-shape home**, NOT a fallback.
  - Signature: `pub fn delete_entity_or_rethrow(collection: &'static str, id:
    &str, op: impl FnOnce() -> Result<bool, DbError>) -> Result<bool,
    DbError>`.
  - It logs ERROR `Error deleting entity` `collection, id, error =
    %error_text(..), strictFailures = strict.then_some(true)` and returns the
    `Err`.
  - The same strict tail as the batch homes (`fallback.rs:406-428`); the
    `chat_override.rs:384-420` precedent is a strict-aware rethrow line.
  - Optional siblings for item 7: `update_entity_or_rethrow` / `create_entity_
    or_rethrow`.
  - `HOME_MESSAGES` grows by 1 (or 3). Check `chat_override.rs:397,411`:
    they emit `Error creating entity` literally OUTSIDE the home, so adding
    that literal to `HOME_MESSAGES` reddens the guard on those two sites (a
    fold, `dangerous_content/` — not this lane's file; see Risk).
- **Two new FALLBACK homes for the inform outers**, in v4's bytes:
  - `Error deleting pending informs by batch {collection: chat_informs,
    batchId, error}` → `0`;
  - `Error deleting pending informs for participant {collection, chatId,
    participantId, error}` → `0`.
- **`db/chat_informs.rs` production fns:**
  - the per-row `DELETE` goes through the rethrow home (or a new private
    `delete_one` routed through it);
  - the batch and participant bulk deletes wrap in their fallback homes;
  - `delete_by_chat_id` stays propagating — it has no v4 caller and is the
    v5-only cascade.
- **Callers:**
  - `api/chat_informs.rs:550-563` loses its local line (the home logs it);
  - `api/chat_cast.rs:650-671`'s `Err` arm becomes unreachable (the home
    answers `0`), and the DEBUG then fires with `dropped_informs=0` as v4.
- **Blast radius of adopting the rethrow home repository-wide:** 40 `delete`
  fns. A minimal order adopts it at the inform rows only, where the line is
  reached and pinned, and names the other 39 as a census table.

**6. The proof.**
- `chat_informs_routes_equivalence` MOCKS the repositories, so it cannot see
  repository lines.
- `chat_informs_tier2_equivalence` + `harness/oracle/cases/chat-informs-
  tier2.ts` drive v4's REAL `ChatInformsRepository.deletePendingByBatch`
  (`:130-131`), but capture no log lines today.
- Proposed: a plant on a per-run copy (a `CREATE TRIGGER … BEFORE DELETE ON
  chat_informs BEGIN SELECT RAISE(ABORT,'planted'); END` — reads still
  succeed, deletes fail), plus a `Logger.prototype.error/warn` spy (recipe
  `harness/oracle/cases/chats-messages-ops-tier2.ts:120-155`), compares v4's
  `[Error deleting entity, Error deleting pending informs by batch]` sequence
  and the `0` against v5's.
- v5 side: these lines fire inside `db.write` on the WRITER thread, so
  thread-scoped `captured_with` cannot see them (P4.142 gotcha). Use
  `crate::test_support::global_capture` (as `db/chat_informs.rs:830-832`
  arms it), or pin at the repository directly over a `Connection`, as
  `chat_informs.rs:840-890` does.
- Tier: a capture-pinned log line (tier-2 op + line compare).

**7. Fixtures.** `harness/oracle/fixtures/chat-informs-tier2.json` (spec, read
by `chat_informs_tier2_equivalence` + `chat-informs-tier2.ts` only). A
trigger plant on a per-run copy rebuilds nothing.

**8. Risk / ruling.**
- **RULING:** port the success INFO `Entity deleted` / WARN `Entity not found
  for deletion` lines too, or only the ERROR? The standing record
  (`status-log.md:757-763`) declined the generic success lines.
  Recommendation: ERROR only, plus a recorded non-port for the success pair.
- Writer-thread capture (above).
- `db/chat_informs.rs`'s TEST module is excluded from this lane by the brief.
  Put the new pins in `fallback.rs`'s test module and `api/chat_informs.rs`'s,
  or ask for a carve-out.
- `api/chat_cast.rs` is a one-arm edit in a file probably owned elsewhere.
  Flag it.
- The `Error creating entity` literal at `dangerous_content/chat_override.rs`
  would trip `fallback_home_guard` if the create sibling is added. Add the
  create/update siblings only with an agreed fold of that file.

---

## Item 2 — a BLOB in `connection_profiles.name` fails a background job

**1. The item, quoted.** `dogfood-findings.md:666-672` (Standing notes,
2026-10-03, item 2): "Under the C5 plant the walk's real turn ran a
`TITLE_UPDATE` job that FAILED with `sqlite error: Invalid column type Blob at
index: 2, name: name` — a profile LIST read propagating the corrupt row where
v4's repository read would fall back or skip it. Measure which read (likely
the cheap-profile resolution's list) and what v4 does on the same row before
ordering". Walk evidence: `dogfood-walks/2026-10-03-recorded-divergences-and-
roster-pass.md:78,169-171`.

**2. v5 today.**
- `services/title_update_job.rs:216-223`:
  `db.read_main(|c| connection_profiles::find_by_user_id(c, &uid)).map_err(|e|
  e.to_string())?`. This read is the list. The single-profile read at
  `:200-208` passed in the walk because the job's own profile was not the
  BLOB row.
- `db/connection_profiles.rs:951-966` `find_by_user_id` and `:910-920`
  `find_all` both `push(r?)` per row. One `marshal_cp_row` failure
  (`:713-`, `r.get::<_, String>(2)?` for `name` at `:718`) fails the whole
  list with `InvalidColumnType`.
- **42 production call sites** of `connection_profiles::find_by_user_id` /
  `find_all` outside the repository, measured. They span `api/settings.rs`
  ×6, `services/orchestrator.rs` ×2, `outfit_selections.rs` ×2,
  `chat_admin.rs` ×2, `title_update_job.rs`, `context_summary_job.rs`,
  `memory_extraction_job.rs`, `cheap_llm_fallback.rs`, `fallback_repos.rs`,
  `chat_participants.rs`, `danger_scan.rs`, `dangerous_content/
  {gatekeeper_job,moderation_wire,understudy}.rs`, `story_background_job.rs`,
  `help_chat/summary_check.rs`, `backup/{collect.rs, restore/orchestrator.rs}`,
  `qtap_export/{entities,mod,preview}.rs`, `quilltap_import/profiles.rs`,
  `tools/generate_image.rs`, `pascal/llm_consult.rs`, `enclave/step.rs`, and
  others.

**3. v4 at `07b8f0209`.**
- `lib/background-jobs/handlers/title-update.ts:72`: `const
  availableProfiles = await repos.connections.findByUserId(job.userId)`.
- `findByUserId` = `UserOwnedBaseRepository.findByUserId` →
  `this.findByFilter({ userId })` (`base.repository.ts:556-558`).
- `findByFilter` (`:283-298`) is a fallback `safeQuery`, and maps each row
  through `validateSafe`.
- The backend decodes a BLOB in a non-blob, non-JSON column as `Float32Array`
  (`lib/database/backends/sqlite/backend.ts:455-463`, `logger.trace('Buffer
  in non-blob column, decoding as Float32')`). `ConnectionProfileSchema.name`
  is `z.string()` (`lib/schemas/profile.types.ts:42-45`), so `validate`
  (`base.repository.ts:131-143`) logs:
  - **ERROR `Data validation failed {collection: 'connection_profiles', error:
    <ZodError.message>}`**, then throws;
  - `validateSafe` (`:146-157`) catches and logs **WARN `Safe validation
    failed {collection, error}`** → the row is dropped.
- The list answers WITHOUT the row, and the job proceeds (selection,
  cheap-LLM call, title).
- `_findAll` (`:263-278`) does the same, so `findAll` drops too.
- The ZodError message bytes: `JSON.stringify(issues, null, 2)` of one
  `invalid_type` issue, path `["name"]`, `received Float32Array`. Byte form
  NOT MEASURED here. The renderer exists: `api/zod_issues.rs`
  `zod_float32_array_cell` + `zod_error_message`, proven tier-1 by
  `repository_zod_messages_equivalence` for groups/links/chats.
- **Single-row read (C5's path), for completeness:** v4 `_findById`
  (`:247-258`) → `validate` throws inside the fallback `safeQuery`, so it logs
  ERROR `Data validation failed` FIRST, then `Error finding entity by ID
  {collection, id, error: <ZodError.message>}` → `null`. v5 logged only the
  second line, with `error=Invalid column type Blob at index: 2, name: name`
  (walk row C5). The `error` bytes differ and a line is missing; the same
  class as the API-key note at `fallback.rs:1069-1078`.
- Reachable in v4: yes. Strict scope does NOT affect `validateSafe`, so the
  importer drops the row too.

**4. Divergence.** v5 fails every connection-profile LIST read on one
corrupt row (the job fails, the settings list 500s); v4 drops that row with
two lines and carries on.

**5. Predicted hunks.**
- `db/connection_profiles.rs` `find_by_user_id` + `find_all` (and
  `find_default` if in scope, a `findOneByFilter` → `validate` throw →
  `Error finding entity by filter`): a per-row match on `marshal_cp_row`.
  - `InvalidColumnType` (and `FromSqlConversionFailure`) → skip the row,
    with v4's two lines rendered through `zod_issues`.
  - Every other `Err` still propagates.
- The 42 callers need no edit; they move by behaviour.
- **Scope trap:** v5's `marshal_cp_row` does not validate enums, uuids or
  JSON shapes, so v5 ACCEPTS rows v4's Zod drops (a bogus `provider`, a
  non-uuid `userId`). Full `ConnectionProfileSchema` validation is a far
  larger order. Recommend scoping to the BLOB-in-TEXT arm (the measured
  dogfood shape) and recording the rest.

**6. The proof.**
- `title_update_tier3_equivalence` + `harness/oracle/cases/title-update-
  tier3.test.ts` already run v4's REAL `handleTitleUpdate` over a per-case
  fresh copy of the `cost-background` pair. They also already carry a
  corrupt-cell plant (`corrupt_key_logs_and_refuses`, `:622-673`,
  `corrupt_bound_key` `:1611-1622`).
- New case: a SECOND profile for the job's user with `name = x'4a554e4b'`.
  - v4: the job completes and titles the chat; the lines are `Data validation
    failed` + `Safe validation failed`.
  - v5 today: the job fails. RED-first on the comparand.
- Plus a unit capture pin in `connection_profiles.rs`, and a
  `repository_zod_messages_equivalence` row for `ConnectionProfileSchema`'s
  `name` BLOB if the message bytes are rendered by a new issue function.

**7. Fixtures.** `crates/quilltap-web/tests/fixtures/cost-background-
{main,mount}.db` (committed; readers `cost_background_routes_equivalence.rs`,
`title_update_tier3_equivalence.rs`, oracle `cost-background-routes.test.ts`,
`chat-cast-routes.test.ts`, `retry-uncensored-tier3.test.ts`,
`title-update-tier3.test.ts`). The plant is per-run, so the committed pair is
NOT rebuilt.

**8. Risk / ruling.**
- **RULING:** the backup collect (`backup/collect.rs`) and the `.qtap` export
  read these lists. Under the 2026-08-03 "fix, don't match" ruling, should a
  backup silently DROP a corrupt profile (v4) or FAIL? Today v5 fails, and
  the lists sit in P4.147 / P4.148's files.
  - Recommendation: the repository drops (v4); the backup/export keep a
    strict sibling only if the human rules so.
- `db/connection_profiles.rs` ownership: confirm with the round table. It is
  not on the brief's excluded list.

---

## Item 3 — `chatGet` / `listChats` 500 `no such column: l.relativePath`

**1. The item, quoted.** `dogfood-findings.md:678-684` (2026-10-02 note):
"`chatGet` and `listChats` answer 500 `sqlite error: no such column:
l.relativePath` — a direct `doc_mount_*` repository read outside the fallback
homes … Measure v4's chat GET / list under the same plant".

**2. v5 today.** CLOSED by P4.142 (unit 2's batch twins, unit 4's arms):
- `api/salon.rs:121-163` `list_chats`: the overlay batch reads fall back
  (`db/vault_read_overlay.rs:136,146,155`), vaulted characters drop, 200.
- `api/salon.rs:185-238` `chat_get`: the overlay's single read surfaces
  `Unavailable` → `chat_get_failed` (`:89-92`) → 500 `Failed to fetch chat`
  + ERROR `[Chats v1] Error fetching chat chatId=…`.
- P4.142 §R.4 item 5 had already measured that neither failure is "a direct
  `doc_mount_*` read" — both are the vault overlay.

**3. v4.** `handleList` → 200 with drops; `handleGetChat` → its own catch
(`app/api/v1/chats/[id]/handlers/get.ts:416-419`) → 500 `Failed to fetch
chat` (P4.142 survey §C).

**4. Divergence.** NONE — converged. Proven live on the Friday copy on
2026-10-03: walk rows F1 / F2 PASS (`dogfood-walks/2026-10-03-…:104-105`).
One recorded residue, pinned both ways in `salon_reads_equivalence`: the
participant fan-out (v5 logs one participant's 11 batch lines where v4 logs
every participant's).

**5–8.** No hunks. The order should strike the note as closed by P4.142.

---

## Item 4 — the strict scope honoured by the other homes the importer reaches

**1. The item, quoted.** P4.142 Unification (`work-orders/p4.142-…md:3`):
"v5's strict scope honoured ONLY by the two batch homes — the
importer-reachable converted twins (e.g. `file_storage.rs:1158`'s link lookup
via `write_user_upload_to_mount_store`) now skip silently where v4's strict
import rethrows". Also P4.143 item 11 (P4.142 × P4.143).

**2. v5 today.**
- **Readers of `strict_repository_failures_active()` (3):**
  - `db/fallback.rs:412` `documents_by_mount_point_ids_and_path_or_empty`;
  - `:445` `documents_by_mount_point_ids_in_folder_or_empty`;
  - `services/dangerous_content/chat_override.rs:391` (a rethrow line's tail,
    not a fallback).
- **Homes that do NOT honour it (21):** `find_by_id_or_none`,
  `find_all_or_empty`, `find_by_filter_or_empty`,
  `find_one_by_filter_or_none`, `joined_file_links_or_empty`,
  `document_by_mount_point_and_path_or_none`, `delete_with_gc_or_false`,
  `find_api_key_by_id_or_none`, `find_api_key_by_id_and_user_id_or_none`,
  `ensure_table_or_log`, `ensure_collection_or_log`,
  `seed_built_in_templates_or_log`, `sweep_orphaned_store_children_or_default`,
  `count_embedded_chunks_or_empty`, `search_chunk_content_or_empty`,
  `search_file_links_by_name_or_path_or_empty`,
  `files_by_mount_point_id_or_empty`, `file_by_mount_point_and_path_or_none`,
  `clear_embeddings_by_link_id_or_zero`, `find_api_keys_by_user_id_or_empty`,
  `can_character_participate_or_false`.
  - Every one except the batch pair returns a bare `Option` / `Vec` / `bool`,
    so honouring the scope means a `Result` signature (the batch homes'
    precedent: `Result<Vec<T>, DbError>` with `Ok([])` as the fallback).
- **Scope entrants (8):**
  - `quilltap_import/mod.rs:977` (execute) and `preview.rs:85`;
  - `qtap_export/mod.rs:260,374` and `preview.rs:34`;
  - `backup/collect.rs:489`;
  - `services/cascade_delete.rs:196`;
  - `db/doc_mount_documents.rs:768` (a test).
- **Measured importer-reachable converted twin (one):**
  `services/file_storage.rs:1163-1167`, inside `store_mount_blob`
  (`:1113-`): `links.find_by_mount_point_and_path_or_none(…)` → the links
  twin → `joined_file_links_or_empty` (`fallback.rs:128-142`). It is reached
  from `quilltap_import/files.rs:292` (`write_project_file_to_mount_store`)
  and `:309` (`write_user_upload_to_mount_store`), both in
  `services/file_storage.rs:1238,1328`. Today a failed read answers `None`
  silently, skips the chunk delete and upserts.
- **Checked and NOT twins (they propagate, so they are already correct under
  strict):** `resolve_unique_relative_path` (`file_storage.rs:1060-1090`, the
  propagating `find_by_mount_point_and_path(…)?`); `ensure_folder_path`
  (`doc_mount_file_links.rs:1563-`, raw SQL `?`); `create_character_with_
  options` → `ensure_character_metadata_file` and `document_store_overlay::
  read_properties` (held-pending-ruling, propagate); `characters_read::find_by_
  id_raw` (propagating).
- **The backup / export reach:**
  - transitively, the overlay batch homes (honoured) and propagating list
    reads;
  - no direct twin call in `services/backup/**` or `services/qtap_export/**`
    (measured: zero `_or_none(`/`_or_empty(`/`fallback::` hits outside the
    scope wraps and `error_text`);
  - `qtap_export/records.rs` `stream_one_store` is strict-by-ruling.
  - NOT MEASURED: a full transitive call graph (no tool). The order should
    have the lane run a scripted reachability pass before declaring the list
    complete.

**3. v4.**
- `withRawDb` (`dedicated-db.repository.ts:235-264`) and every fallback
  `safeQuery` read `strictRepositoryFailuresActive()` (`safe-query.ts:57-71`).
  Inside the import, EVERY fallback read logs with `strictFailures: true` and
  RETHROWS, including `queryJoined` under `store-file.ts:276-279`.
- **One exception that does not honour the scope:** `withRawDb`'s
  `acquireDb()` arm (`:242-251`) answers the fallback QUIETLY (DEBUG
  `Dedicated database unavailable; answering with the fallback`) even inside
  strict. A missing mount-index DB is a silent `[]` in v4's import too.

**4. Divergence.** Under the import's strict scope, v5's
`file_storage.rs:1164` link read answers `None` silently where v4 logs
`Error querying joined file links … strictFailures: true` and fails the
import.

**5. Predicted hunks.**
- `db/fallback.rs`: `joined_file_links_or_empty` → a strict-aware variant
  (e.g. `joined_file_links_scoped(where, read) -> Result<Vec<T>, DbError>`,
  the batch-home shape).
- `db/doc_mount_file_links.rs`: a `find_by_mount_point_and_path_or_none_
  scoped` → `Result<Option<_>>` beside the existing twin.
- One call-site edit at `services/file_storage.rs:1164` (`?`).
- Blast radius if the existing twin's signature were changed instead: 7
  `joined_file_links_or_empty` call sites and **32**
  `.find_by_mount_point_and_path_or_none(` call sites. Do NOT change the
  shared twin; add the scoped sibling.
- **Hunk outside the proposed ownership:** `services/file_storage.rs` (one
  line). It is not the importer's file, but confirm nobody else holds it.

**6. The proof.**
- A unit in `fallback.rs` (strict → `Err` + `strictFailures=true`;
  non-strict → `Ok([])`), the `each_p4142_document_store_shape_logs_v4s_
  exact_line` / `the_strict_scope_appends_strict_failures_and_propagates`
  precedent.
- A caller pin: `store_mount_blob` under `with_strict_repository_failures`
  over a provisioned store with the links `originalMimeType` rename plant
  (the P4.142 isolating plant) → `Err`.
- A two-sided import plant would need `system_import_*` (P4.148's family).
  Leave that leg to P4.148, or pin v5-side only and record it.

**7. Fixtures.** None committed; per-run provisioned store.

**8. Risk.** The importer files belong to P4.148 and the backup files to
P4.147. This lane edits `db/fallback.rs`, `db/doc_mount_file_links.rs` and one
line of `services/file_storage.rs`. State a §S handoff if P4.148's own sweep
needs to see the new arm.

---

## Item 5 — P4.142's OPEN items: the scenario checks, `list_chats`' first read, the sync RULING, the 23 held sites

### 5a. The scenario create/rename conflict checks

**1. Quoted.** P4.142 Unification: "the scenario create/rename conflict checks
converted in unit 10 (a failed read reads 'no conflict' — the class unit 11
held elsewhere; low risk, same writer)".

**2. v5.** `api/scenarios.rs:243-254` (`create_op`) and `:366-381`
(`rename_op`), both on `find_by_mount_point_and_path_or_none` (the documents
twin). Pinned by `api::scenarios::fallback_read_tests` (`:612-650`).

**3. v4.** `scenarios/route.ts:89-95` (and the project/group twins), plus
`scenario-item-route-factory.ts:165-169,239-251`. All are fallback
`withRawDb(null)`: `null` → no conflict → write / move.

**4. Divergence.** NONE — converged. Low hazard:
- rename reads `existing` first on the same connection and SQL, so a broken
  documents read answers 404 before the conflict read can matter;
- create's next step (`write_database_document`) writes through the same
  table and fails on the same breakage (unit 10's capture unit measured this:
  "the line FIRST and never the conflict 400 — the write is what fails
  next").

**5–8.** No hunk. Recommend the order record it as CLOSED (v4-faithful, safe
by measurement), not as held.

### 5b. `list_chats`' first read

**1. Quoted.** P4.142 Unification: "`list_chats`' first read answers 500 where
v4's `findByUserId` falls back to 200 `[]`".

**2. v5.** `api/salon.rs:129-132`: `db.read_main(|conn| chats_read::find_by_
user_id(conn, …))` → on `Err`, `list_chats_failed(e)` → 500 `Failed to fetch
chats` + ERROR `[Chats v1] Error listing chats`. `chats_read::find_by_user_id`
is `db/chats_read.rs:405-407` (`run(conn, "WHERE userId = ?1", …)`).

**3. v4.**
- `chats.repository.ts:158-160`: `findByUserId` = `this.findByFilter({ userId
  })`, a fallback (`base.repository.ts:283-298`). A failed read logs ERROR
  `Error finding entities by filter {collection: 'chats', error}` → `[]`.
- `handleList` (`app/api/v1/chats/route.ts:1048-1084`) → 200 `{chats: []}`.
  Its catch is not reached.

**4. Divergence.** A failed chats read answers v5 500 `Failed to fetch chats`
where v4 answers 200 `{"chats":[]}` with the filter line.

**5. Hunk.** `api/salon.rs:129-132` → `let all = crate::db::fallback::find_by_
filter_or_empty("chats", || db.read_main(…));` (the `chat_get` `:197`
precedent wraps the whole `read_main`, so a pool checkout logs the line too).
One site; `db/chats_read.rs` is untouched.

**6. Proof.**
- `salon_reads_equivalence` + `salon-reads.test.ts` already pose a per-case
  plant (`renameMountColumn` `:96-101,282-288`) with a `Logger.prototype`
  spy. Add a `renameMainColumn` key (MAIN copy) and a `list_main_plant` case
  renaming `chats.userId` → `userId_x`.
- Prediction: v4 200 `[]` + one `Error finding entities by filter
  collection=chats`; v5 500 RED-first on status.
- ⚠ Prediction NOT MEASURED. v4's `ensureCollection` runs `generateDDL` (`CREATE
  … IF NOT EXISTS`, `backend.ts:763-791`), which does not re-add a renamed
  column. Per P4.142's gotcha, a rename fails v4 only when the WHERE names
  the column; `userId` is named.

**7. Fixtures.** `crates/quilltap-web/tests/fixtures/salon-{main,mount}.db`,
COPIED per case. Not rebuilt. 24 readers (harness salon_* families,
`transcript_route`, web `messages_swipe_sse_route` + `common/mod.rs`, five
Playwright specs + `global-setup.ts`).

**8. Risk.** `api/salon.rs` is probably held by another lane this round
(P4.142 used a §S error-arms-only carve-out). This is a one-line hunk; ask for
the same carve-out.

### 5c. The sync applier RULING (P4.142 Tier 3 item 16)

**v5 today.**
- `services/mount_index/sync/apply_store.rs:125` (`assert_unchanged`),
  `:261` (`write_store_file`), `:273` (`read_store_bytes`), `:347`
  (`apply_store_action`'s path read) and `:381` (`delete_with_gc`) all
  propagate.
- `walk_store.rs:48-` (hand SQL, `:56`, `:94`) propagates.
- The census keeps the five as `fallback-in-v4`
  (`doc_mount_fallback_sites_census.rs:719-723`, `COUNTS` last field `5`).

**The written-up ruling** (`status-log.md` "P4.142 — … LANE COMPLETE", Tier 3
item 16), summarised for the human:
- **Hazard.** v4's `walkStore` (`walk-store.ts:47,60`) reads through
  fallbacks, so a failed store read is an EMPTY store. The planner then emits
  `removal('disk', …, 'deleted in the store since last sync')` for every
  unchanged base entry (`planner.ts:206-214`; `propagateDeletes` defaults
  `true`, `types.ts:37`). **A failed store read in v4 plans deleting the
  operator's whole disk side.** No degraded-index guard exists.
- **Option A — follow v4.** The walker + five reads take twins: fidelity, and
  the hazard reproduced.
- **Option B — strict sync (RECOMMENDED).** The walker, the five reads and a
  strict sibling for the rmdir all propagate, and the run fails before
  planning.
  - It is recorded as a deliberate divergence of bug 79's kind (v4's own
    strict-scope precedent).
  - Pinned both ways in `sync_engine_equivalence` with a rename-plant case
    (v4 plans deletes; v5 refuses).
  - The walker hazard is filed upstream as a v4 bug.
  - **Not filed yet** (measured: no `walkStore`/`propagateDeletes` hit in v4
    `docs/developer/bugs/`).
- Under B, NO core change to the applier's five reads (they already
  propagate). The work is the rmdir's strict sibling (today
  `delete_database_folder` falls back, P4.131), the census class rename
  (`fallback-in-v4` → `strict-by-ruling`, 5 rows), the both-ways pin and the
  v4 filing. `services/mount_index/sync/**` was a MUST-NOT in P4.142; confirm
  ownership this round.

### 5d. The 23 `held-pending-ruling` sites

Measured on `main` (`doc_mount_fallback_sites_census.rs:270-421` rows +
reasons; `:603-716` EXPECTED). Each hazard is the census comment, verbatim in
substance:

| # | Site (file → fn → method) | Named hazard if converted to v4's fallback |
|---|---|---|
| 1 | `db/character_vault.rs` → `ensure_character_metadata_file` → path read | `metadata.json` overwritten with the empty seed; v5 reaches it from every FK `ensure_character_vault` + the importer's adopt arm (v4: boot backfill only) |
| 2 | `db/document_store_overlay.rs` → `read_properties` | project/group settings bag reseeded from defaults; importer-reachable (`reconcile.rs:493`) |
| 3 | `db/vault_character_update.rs` → `read_current_properties` | all six vault properties reset (bug 8 / dogfood #47) |
| 4 | `photos/avatar_rolls_service.rs` → `delete_avatar_roll` → `delete_with_gc` | `files` row deleted, `deleted: true`, link + blob survive invisibly |
| 5 | `photos/character_gallery_service.rs` → `remove_from_character_gallery` → `delete_with_gc` | pointers cleared, 200 `deleted: true`, photo stays |
| 6 | `photos/save_image_to_album.rs` → `find_existing_photos_link_by_sha` → list | a DUPLICATE album photo saved |
| 7 | `photos/user_gallery_service.rs` → `remove_from_user_gallery` → `delete_with_gc` | DIFFERS besides: v4 answers `fileId !== null` (→ 404); the bool twin cannot carry it |
| 8 | `character_archive/service.rs` → `prune_vault` → list (survivors) | `undead = 0` → every non-Wardrobe folder row deleted, success reported |
| 9 | same → `prune_vault` → `delete_with_gc` | surviving chunks' `embedding_status` rows deleted |
| 10 | same → `prune_empty_folders` → list | prunes nothing and says nothing (paired with #8) |
| 11 | `services/file_storage.rs` → `delete_mount_blob_conn` → `delete_with_gc` | `files` row dropped, link + blob orphaned, success |
| 12 | `mount_index/file_ops.rs` → `source_exists_or_throw` → path read | disk rename happens, source link never deleted (stale until rescan) |
| 13 | same → `dest_exists` → path read | DEST_EXISTS guard skipped → **silent overwrite** |
| 14–15 | same → `delete_at_source` → `delete_with_gc` ×2 | a move reports success while the source link survives (duplicate) |
| 16 | same → `delete_at_dest` → path read | force overwrite keeps the old link; with #17, a false `deleted: true` |
| 17 | same → `delete_at_dest` → `delete_with_gc` | the delete half of #16 |
| 18 | same → `move_file` → `delete_with_gc` | a move reports success, the source link survives |
| 19 | `mount_index/folder_ops.rs` → `move_folder` → list | links keep the old paths; the next scan drops their chunks / embeddings / descriptions |
| 20 | `mount_index/general_state.rs` → `ensure_general_state_file` → path read | `state.json` reset to `{}` at boot |
| 21 | `mount_index/scanner.rs` → `remove_mount_file` → `delete_with_gc` | over-counted in `files_deleted` (the stale link heals next scan) |
| 22 | `mount_index/store_file.rs` → `store_mount_file` → `find_content_and_mtime_…` | optimistic-concurrency guard bypassed → **concurrent edit silently overwritten**; doubled log line |
| 23 | (the 23rd) `prune_vault` has TWO list rows (`:664`, `:667`), counted separately | as #8 |

(`file_ops` contributes 7 rows (#12–18); the census total is G2 6 + G3 17 =
23.)

**Candidates for conversion WITHOUT a new ruling.** Each still needs the
human's "convert safe" classification re-applied, because the lane had judged
all 23 risky:
- **#10 `prune_empty_folders`' own read:** a failed read prunes NOTHING. That
  is safe on its own; the destructive arm is #8's read, not this one.
- **#21 `remove_mount_file`'s delete:** reporting-only (over-count), self-heals
  on the next scan.
- **#12 `source_exists_or_throw`:** a stale link until rescan. Self-healing
  but user-visible; borderline.

Everything else either destroys or misreports data (1–9, 11, 13–20, 22) or
cannot be expressed by the existing twin (#7).

**Ruling to put to the human (one paragraph).** The 23 sites are places where
v4's fallback answer feeds a WRITE or a success report, so matching v4 imports
v4's data loss (P4.142's closing gotcha). The options:
- **(i)** keep all 23 propagating as a recorded divergence family under the
  2026-08-03 "fix, don't match" ruling — one census class
  `strict-by-ruling(write-path)`, file the clearest (#13, #22, #8) upstream as
  v4 bugs;
- **(ii)** convert them to v4;
- **(iii)** convert the three low-hazard candidates above and hold the rest
  under (i).

Recommendation (iii). The lane's work is then census re-classing + three
capture units + v4 bug filings. The held 20 need no core change.

---

## Item 6 — P4.D245's OPEN items

**1. Quoted.** `work-orders/p4.d245-…md:3`: "Tier 2 item 14's second half (the
`Error finding links by project ID` home in `tools/wardrobe_shared.rs`); …
item 18 (`resolve_project_mount_point_ids_for_chat`'s silent chat-read
`Err`); the enumeration's `.unwrap_or_default()` over the collector (latent —
only an `operator_override` caller could hit its fallible arm; a
`debug_assert!` or a matched `Err` would keep it honest)". Items 14/18 text:
`:289-308`.

### 6a. `resolve_project_mount_point_ids` — the project-links read

- **v5** `tools/wardrobe_shared.rs:47-58`:
  `ProjectDocMountLinksRepository::new(mount).find_by_project_id(project_id)
  .unwrap_or_default()`. A failed read is SILENT `[]`.
  `db/project_doc_mount_links.rs:158-166` propagates.
- **v4**
  - `lib/mount-index/tiered-mount-pool.ts:169-181`: `try { findByProjectId }
    catch → WARN 'Project mount lookup failed' }`.
  - `project-doc-mount-links.repository.ts:62-74` `findByProjectId`: a
    4-argument fallback `safeQuery(…'Error finding links by project ID',
    {projectId}, [])` around `this.findByFilter`, itself a fallback
    (`base.repository.ts:283-298`), with `getCollection()` (incl.
    `ensureTable`, `dedicated-db.repository.ts:194-212`) inside it.
  - **Reachable line:** ERROR `Error finding entities by filter {collection:
    'project_doc_mount_links', error}` → `[]`.
  - **UNREACHABLE:** `Error finding links by project ID` and `Project mount
    lookup failed`. On a lazy-ensure failure v4 first logs `Failed to ensure
    project_doc_mount_links table in mount index database {error}` (v5 runs
    ensures at boot — the recorded cadence divergence).
- **Divergence.** v5 logs nothing; v4 logs the filter line. **No new home**:
  `find_by_filter_or_empty("project_doc_mount_links", || …)` at
  `wardrobe_shared.rs:55-57`.
- **Callers (all unchanged in behaviour, `[]` either way):**
  `wardrobe_tiers.rs:184`, `services/chat_merge.rs:249`,
  `services/chat_create.rs:1522`, and via 6b's fn: `generate_image.rs:1684`,
  `story_background_job.rs:1353`, `chat_participants.rs:1369`,
  `chat_create.rs:2174`, `character_avatar_job.rs:288`,
  `aurora_notifications.rs:344`.

### 6b. `resolve_project_mount_point_ids_for_chat` — the chat read (item 18)

- **v5** `tools/wardrobe_shared.rs:63-73`: `match chats_read::find_by_id(main,
  chat_id) { Ok(Some(c)) => c, _ => return Vec::new() }`. An `Err` is
  silent.
- **v4** `tiered-mount-pool.ts:240-252`: `repos.chats.findById` is a fallback
  `_findById`. It logs ERROR `Error finding entity by ID {collection: 'chats',
  id, error}` → `null` → `resolveProjectMountPointIds(null)` → `[]`. The
  `Project mount lookup for chat failed` WARN is unreachable (item 17).
- **Hunk:** `crate::db::fallback::find_by_id_or_none("chats", chat_id, ||
  chats_read::find_by_id(main, chat_id))`.

### 6c. The enumeration's `.unwrap_or_default()`

- **v5** `tools/doc_edit/shared.rs:716-748` `get_accessible_mount_points`
  calls `doc_edit/path_resolver.rs:417-494` `collect_accessible_mount_point_
  ids(…).unwrap_or_default()` (`:746`). The collector's only `?` is the
  operator arm `:422-423` (`find_enabled_for_docedit()?`).
- **v4:** the operator arm (`lib/doc-edit/path-resolver.ts:329-337`) reads
  `repos.docMountPoints.findEnabled()`, a fallback `safeQuery(…'Error finding
  enabled mount points', {}, [])` over `findByFilter`
  (`doc-mount-points.repository.ts:99-111`). It CANNOT throw: the reachable
  line is `Error finding entities by filter {collection: 'doc_mount_points',
  error}` → `[]`.
- **So the collector is INFALLIBLE in v4**, and v5's `?` at `:423` is itself
  a divergence on the operator path. The resolver at `path_resolver.rs:602`
  propagates it as a path-resolution error where v4 resolves against an empty
  set.
- **Hunk:**
  1. `:423` → `find_by_filter_or_empty("doc_mount_points", || …)`.
  2. Change the collector's return to `Vec<String>`. This removes the
     `.unwrap_or_default()` at `shared.rs:746`, the `?` at
     `path_resolver.rs:602`, and the five test `.unwrap()`s
     (`path_resolver.rs:1488,1509,1537,1558,1579`).
  3. Better than a `debug_assert!`: the unreachable arm stops existing.
- **Adjacent, same class, NOT in the item:**
  - `enabled_accessible_mount_points` (`shared.rs:752-`): `if let
    Ok(Some(row)) = repo.find_by_id_for_docedit(&id)`, a silent skip where
    v4's per-id `findById` logs `Error finding entity by ID {collection:
    doc_mount_points, id}`;
  - seven other `find_enabled_for_docedit()` callers
    (`pascal/{roster:564,workbench:429}`, `tools/search.rs:642`,
    `documents/mod.rs:664`, `doc_edit/uri_producers.rs:156`,
    `photos/user_gallery_service.rs:132`,
    `services/embedding_reindex_job.rs:552`);
  - name them for a census, do not convert blind.

**Proof (6a–6c).**
- `tiered_mount_pool_equivalence` + `harness/oracle/cases/tiered-mount-pool.ts`
  already drive v4's REAL helpers through `helperArms` over `helperPlants`
  (header `:16-24`, the P4.D231 recipe). Add arms for
  `resolveProjectMountPointIds` / `…ForChat` with a
  `project_doc_mount_links.projectId` rename plant and a `chats.id` plant.
- Compare `[]` + the captured line. Check whether that oracle spies the
  logger; NOT MEASURED. Add the spy if not.
- 6c: a `doc_edit_path_resolver` / `project_roster_access` operator-arm plant
  (`doc_mount_points.enabled` renamed) or a unit capture pin.
- Fixtures: the tiered-mount-pool fixture is built per run (`build-tiered-
  mount-pool-fixture.ts`), so nothing is committed.

**Risk.** `doc_edit/path_resolver.rs` and `tools/doc_edit/shared.rs` are not
on the proposed list; 6c's signature change lands there. `project_roster_
access.rs` does NOT hold `resolve_project_mount_point_ids_for_chat` (it lives
in `tools/wardrobe_shared.rs`; the roster file only names it in a comment,
`:29`).

---

## Item 7 — P4.144's OPEN items

**1. Quoted.** `work-orders/p4.144-…md:3`: "item 11 RE-RECORDED — the
`updateForCharacter` / `create` repository ERRORs are REACHED (… `Error updating
entity` → `Error updating memory` → `Error updating memory for character` and
`Error creating entity` / `Error creating memory`), UNPORTED, and hidden on both
sides by the comparand's `[FoldEpisodePass]` filter; a divergence the new `?`
opens — a failed `character_id_of` read in `update_for_character`
(`db/memories.rs:329`) now WARNs and stops that character where v4's fallback
`findById` logs, answers null, logs `Memory not found for update` and keeps
linking …; a pool-checkout `Err` on the fragment / chat read answers silently
where v4 logs the fallback line." Items 9–10 (`:435-457`): the MigrationRunner
ledger-skip pin and an MPJ L3 pin. Both are harness-only deferrals, NOT
repository-class, and out of this lane.

**2. v5 today.**
- `db/memories.rs:323-333` `update_for_character`: `if self.character_id_of(
  memory_id)?.as_deref() != Some(character_id) { return Ok(false) }` then
  `self.update(..)`. No lines. Not-found and wrong-owner collapse to a silent
  `Ok(false)`. `character_id_of` (`:505-517`) propagates.
- `delete_for_character` `:336-345`: the same shape.
- `update` `:217` / `create` `:163`: no lines.
- 10 production callers of `update_for_character`: `db/memories.rs:571,667`,
  `api/memories.rs:620`, `services/fold_episode_pass.rs:454,478`,
  `services/search_replace.rs:331`, `services/memory_gate.rs:634,780,836,883`.
- **Fold pass silent reads** (`services/fold_episode_pass.rs`):
  - `:159-163`: `let Ok(Some(chat)) = db.read_main(|c|
    Ok(chats_read::find_by_id_or_none(c, …))) else { return result }`. The
    home sits INSIDE the closure, so a `read_main` checkout `Err` is silent.
  - `:404-409`: the fragment read — the same shape with
    `.unwrap_or_default()`.
  - `:432-437`: `memories_read::find_by_id(..).ok().flatten()`. Silent where
    v4's `findById` logs `Error finding entity by ID {collection: memories}`.

**3. v4** (`lib/database/repositories/memories.repository.ts`).
- `updateForCharacter` `:454-478`: a RETHROW `safeQuery` ('Error updating
  memory for character', `{characterId, memoryId}`).
  - Inner `this.findById` (`:54-56` = `_findById`, a fallback): a failure
    logs `Error finding entity by ID {collection: memories, id}` → `null` →
    WARN `Memory not found for update {memoryId, characterId}` → `null`. No
    throw; the fold keeps linking and counts the back-link.
  - Owner mismatch → WARN `Memory does not belong to character {characterId,
    memoryId}`.
  - An update failure:
    1. `_update` → ERROR `Error updating entity {collection, id}` (rethrow,
       `base.repository.ts:383-420`);
    2. ERROR `Error updating memory {collection, memoryId}` (`:437-447`,
       rethrow);
    3. ERROR `Error updating memory for character {collection, characterId,
       memoryId}`;
    4. then the fold's WARN `[FoldEpisodePass] Failed to write episode for
       character`.
- `create` `:418-430`: ERROR `Error creating entity` (base, rethrow) then
  `Error creating memory {collection, characterId}` (rethrow).
- `deleteForCharacter` `:503-522`: WARN `Memory not found for deletion` /
  `Memory does not belong to character`, ERROR `Error deleting memory for
  character`. `delete` `:486-497` is the `Error deleting entity` → `Error
  deleting memory` pair (item 1's family).
- All reachable (P4.144's `episode_link_fail` / `episode_write_fail` runs
  reached them).

**4. Divergence.**
- A failed owner read stops that character's linking in v5 (WARN `Failed to
  write episode`); in v4 it logs two lines and continues.
- Every repository-level create/update/delete-for-character line is absent.
- Three fold reads are silent on a checkout failure.

**5. Hunks.**
- `db/memories.rs`:
  - `update_for_character` / `delete_for_character`: the owner read through
    `find_by_id_or_none("memories", …)` + v4's two WARNs + the rethrow homes
    from item 1's family. A pool-level `Err` still cannot occur here (it
    takes a `Connection`).
  - `create` / `update`: wrap in `create_entity_or_rethrow` /
    `update_entity_or_rethrow` + their outer `Error creating memory` / `Error
    updating memory` lines.
  - Blast radius: 10 `update_for_character` callers and the `delete_for_
    character` callers. Behaviour moves on a failed owner read only (`Err` →
    `Ok(false)`). The memory gate's four sites then continue where they
    stopped, so measure `memory_gate` / `memories_tier2` neutrality.
- `services/fold_episode_pass.rs`: hoist the two homes outside `read_main`
  (`find_by_id_or_none("chats", id, || db.read_main(..))`, the
  `api/salon.rs:197` precedent) and wrap `:432` in `find_by_id_or_none(
  "memories", …)`.
  - ⚠ This file is NOT on the proposed ownership list (P4.144 owned it last
    round). It is unavoidable for the silent-read half.
- **Families:**
  - `fold_episode_tier3_equivalence` + `fold-episode-tier3.test.ts`: widen the
    comparand from the `[FoldEpisodePass]` filter to include `quilltap::db`
    lines on the existing `episode_link_fail` / `episode_write_fail` cases,
    and add an owner-read plant case (`character_id_of` fails → v4 continues).
  - `memories_tier2_equivalence` / `memories_routes_equivalence` /
    `memory_pipeline_jobs_tier3_equivalence` as neutrality.
- ⚠ **Writer-thread capture again:** `update_for_character` runs inside
  `db.write`. Check how `fold_episode_tier3` captures (the family already
  sees `[FoldEpisodePass]` lines, which fire on the caller thread).

**8. Risk.**
- `db/memories.rs` is the shared memory repository; confirm no sibling lane
  holds it.
- The success INFO lines (`Entity created` / `Entity deleted`) are the same
  ruling as item 1.

---

## Item 8 — the harness: how a new home shape gets pinned

- **`fallback_home_guard.rs`** (136 lines):
  - `HOME_MESSAGES` (`:26-71`) holds **24** literals today (26 `pub fn`s in `fallback.rs`; the strict-scope pair carry no line);
    `every_home_line_is_emitted_only_by_the_home` (`:75-115`) requires each
    literal exactly once in `db/fallback.rs`'s production zone, and nowhere
    else.
  - A new home = one literal here + the home fn.
  - Red-first recipe (P4.142 item 1): add the literal FIRST and record the
    offenders, then fold.
- **`doc_mount_fallback_sites_census.rs`** (962 lines):
  - `METHODS` (12, `:84-99`), `TWIN_SUFFIXES` `_or_none/_or_empty/_or_false/
    _or_zero` (`:112`), `OVERRIDES` (`:~150-430`, `held-pending-ruling` rows
    `:270-421`), `EXPECTED` (`:~430-725`).
  - `COUNTS = (80, 13, 19, 0, 13, 1, 1, 23, 5)` (`:854-865`, arithmetic
    `:793-853`).
  - `HANDED` (`:867-886`). Note it still names `handed(P4.144)` for the fold
    pass; item 7's hunk there keeps the file, so it stays green, but the
    documentation row should be updated.
  - **A new twin whose name ends in `_scoped` (item 4) is NOT one of
    `TWIN_SUFFIXES`**, so it classifies as a propagation (`fallback-in-v4`).
    Either suffix it `…_or_none_strict_aware` or add the suffix.
- **`api_key_read_sites_census.rs`** (538 lines): the API-key reads only.
  Unaffected unless item 2 touches `api_keys`. It asserts "no importer path
  reads `api_keys`".
- **`get_messages_caller_census.rs`** (545 lines): `get_messages` vs
  `get_messages_strict` per site. Unaffected.
- **`fallback_engine_equivalence.rs`: NOT a repository-fallback family** (the
  LLM failover engine, `lib/llm/fallback/`). Ignore it for this lane.
- **The two-sided pin pattern** (P4.142, `status-log.md` P4.142 lane
  record):
  1. a per-run COPY of a committed (or per-run-built) pair;
  2. a column RENAME that only the WHERE of the read under test names (or a
     `BEFORE DELETE` trigger for writes);
  3. v4's REAL route / repository driven by a jest/tsx oracle with a
     `Logger.prototype.error/warn` spy (`harness/oracle/cases/chats-messages-
     ops-tier2.ts:120-155`; in `salon-reads.test.ts:96-101,282-288`);
  4. the lines compared as a multiset by level + message + fields, `error=`
     tails skipped;
  5. on v5, a `captured_with` (caller thread) or `global_capture` (writer
     thread) unit pin with the exact line and a silence leg.

  Pairs/fixtures the plants use:
  - `crates/quilltap-web/tests/fixtures/salon-{main,mount}.db` (salon_reads);
  - `mounts-{main,mount}.db` + `mounts-fs-tree` (mount_read);
  - `cost-background-{main,mount}.db` (title_update_tier3's corrupt-cell
    plant);
  - the per-run mail fixture (`/tmp/qt-mail-*`) and vault-read-overlay
    fixture;
  - the per-run tiered-mount-pool fixture.

  **None is rebuilt by a plant.**
- **Rethrow-shape homes (item 1/7) are new to the guard's model.** Their
  literals (`Error deleting entity`, …) are also v4's lines, so they belong
  in `HOME_MESSAGES` the same way. The guard header (`:1-15`) says "fallback
  lines". Reword it to "repository-layer lines", or give the rethrow homes
  their own sibling list in the same test.

---

## §Ownership proposal

**Edit:**
- `crates/quilltap-core/src/db/fallback.rs`: the rethrow delete home
  (+ update/create if ruled); two inform fallback outer homes; a strict-aware
  joined-links variant; the item-2 per-row drop helper if it lives here.
- `crates/quilltap-core/src/db/chat_informs.rs` (production fns ONLY — the
  per-row delete routing and the two bulk fallback wraps).
- `crates/quilltap-core/src/db/connection_profiles.rs` (`find_by_user_id`,
  `find_all`; `find_default` optional).
- `crates/quilltap-core/src/db/memories.rs` (`update_for_character`,
  `delete_for_character`, `create`/`update` wraps).
- `crates/quilltap-core/src/db/doc_mount_file_links.rs` (the scoped path-read
  sibling).
- `crates/quilltap-core/src/tools/wardrobe_shared.rs` (6a, 6b).
- `crates/quilltap-core/src/doc_edit/path_resolver.rs` +
  `crates/quilltap-core/src/tools/doc_edit/shared.rs` (6c — the collector
  made infallible).
- One-line / one-arm hunks needing a carve-out:
  - `api/salon.rs:129-132` (5b);
  - `api/chat_informs.rs:550-563` (item 1 caller);
  - `api/chat_cast.rs:650-671` (item 1 caller);
  - `services/file_storage.rs:1164` (item 4);
  - `services/fold_episode_pass.rs:159-163,404-409,432-437` (item 7).
- Census reclass only, if the human rules (5c/5d): the census file below.

**Harness:**
- `fallback_home_guard.rs`;
- `doc_mount_fallback_sites_census.rs` (5c/5d reclass, item-4 suffix, the
  `HANDED` row);
- `title_update_tier3_equivalence.rs` + `harness/oracle/cases/title-update-
  tier3.test.ts` (item 2 case);
- `chat_informs_tier2_equivalence.rs` + `harness/oracle/cases/chat-informs-
  tier2.ts` (item 1 trigger plant + spy);
- `salon_reads_equivalence.rs` + `harness/oracle/cases/salon-reads.test.ts`
  (5b `renameMainColumn`);
- `tiered_mount_pool_equivalence.rs` + `harness/oracle/cases/tiered-mount-
  pool.ts` (6a/6b arms);
- `fold_episode_tier3_equivalence.rs` + `harness/oracle/cases/fold-episode-
  tier3.test.ts` (item 7 comparand widening);
- `sync_engine_equivalence.rs` (5c, only under Option B);
- `repository_zod_messages_equivalence.rs` + its case (item 2's message
  bytes, if a new issue fn is added).

**Must NOT touch:**
- `db/projects.rs`, `db/groups.rs`;
- `db/chat_informs.rs`'s test module;
- `services/quilltap_import/**` (P4.148), `services/backup/**` (P4.147),
  `services/qtap_export/**`;
- `services/dangerous_content/chat_override.rs` (the `Error creating entity`
  literal — fold only by agreement);
- `services/mount_index/sync/**` (unless Option B is ruled, then only the
  rmdir sibling);
- `api/types.rs`;
- every committed fixture pair.

**Items landing unavoidably in a sibling's likely file:**
- item 7 → `services/fold_episode_pass.rs`;
- 5b → `api/salon.rs`;
- item 1 → `api/chat_cast.rs`;
- item 2 → behaviour (not text) in backup/export/import list reads.

## §Open questions

1. **Item 1/7 ruling:** port the base repository's success INFO / not-found
   WARN lines (`Entity deleted`, `Entity not found for deletion`, `Entity
   created`, `Entity not found for update`) along with the ERROR rethrow
   lines, or keep the 2026-era "`db::` logs no success lines" stance and port
   ERROR only?
2. **Item 1 scope:** adopt the rethrow delete home at the chat-informs rows
   only (reached and pinnable), or across all 40 `delete` fns plus the 19
   outer per-repository lines (a census, much larger)?
3. **Item 2 ruling:** should the backup collect / `.qtap` export / import
   DROP a corrupt-name profile as v4 does (validateSafe ignores the strict
   scope), or fail under the 2026-08-03 "fix, don't match" ruling? And is
   the BLOB-in-TEXT arm enough, given v5 does not validate the profile
   schema's enums/uuids at all?
4. **5c:** the sync RULING (Option B recommended). Includes filing the
   walker hazard as a v4 bug (not yet filed).
5. **5d:** (i) / (ii) / (iii) for the 23 held sites. Recommendation (iii):
   convert #10, #21 (and possibly #12); hold 20 as `strict-by-ruling`; file
   #13 / #22 / #8 upstream.
6. **Carve-outs:** `api/salon.rs`, `api/chat_informs.rs`, `api/chat_cast.rs`,
   `services/file_storage.rs`, `services/fold_episode_pass.rs`,
   `doc_edit/path_resolver.rs`, `tools/doc_edit/shared.rs` — who owns each
   this round?
7. **Item 4:** confirm a scripted transitive-reachability pass from
   `execute_import` / `preview_import` is wanted before the strict list is
   declared complete. This survey measured one site by grep, not a full call
   graph.
8. **Item 6c adjacents:** the seven other `find_enabled_for_docedit()`
   callers and `enabled_accessible_mount_points`' silent per-id skip — a
   census in this lane, or named for the next round?
