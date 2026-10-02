# Survey — P4.143: the data/zod smalls (the import mask per entity family, the restore orchestrator's serde arm, `dumpFileFacts`'s tier-3 upgrade, the trail WARN's `errors` array)

**Date:** 2026-10-02 · **v4:** f6426e196 (tree dirty by the three recorded docs paths) · **v5 main:** cb9ecf256 · **Kind:** read-only measurement — source reads both sides, `git show`-free (HEADs as named), and four throwaway `npx tsx` probes under `/private/tmp/claude-503/p4143probe/` (Node 24, run from the v4 checkout, importing ONLY v4's pure schemas — `TagSchema`, `RoleplayTemplateSchema`, the three profile schemas, `ChatMetadataBaseSchema`, `ChatEventSchema` — plus the pure `seedLegacyConnectionProfileFields` / `withConciergeModeFromLegacy`; zod `4.6.5` verified at `node_modules/zod/package.json`). No cargo, no jest, no oracle regen. Every "predicted" byte below is a pure-schema probe result; the lane's regen is the proof.

## The finding in one line

All four items stand, but three of them are bigger or different than recorded: (j) v4's `errors` is NOT "the full `path: message` array" in general — `ChatEventSchema` is a plain `z.union`, so zod 4.6.5's `handleUnionResults` collapses EVERY row whose message branch carries an *aborting* issue (a `null` trigger, a bad `via`, a non-array trail, a bad `role`, a non-object `hostEvent`, a NULL `content`, an unknown `type`) to the single line `": Invalid input"`, and only rows whose issues are all non-aborting *checks* (uuid / datetime format, string `too_big`) log their issues — then ALL of them; (c) v4 logs THREE repository ERRORs on a refused chat create, not two (`Failed to create chat` is missing on v5), and on the import path two of them carry `strictFailures: true`; (b) beyond the six masked ZodError tails, v5's image/embedding **duplicate** arms swallow a malformed item with NO warning, v5 accepts an embedding `provider` outside v4's enum, and four of the five families log no WARN on the serde arm; (i) `dumpFileFacts` is a v5 ORACLE helper, not a v4 function.

---

## §A (b) — the import mask's exception, per entity family

### A1 Where the mask lives (v5 harness)

`crates/quilltap-harness/tests/system_import_state.rs` (2142 lines; the brief's guess `import_*_equivalence.rs`/`qtap_import*` does not exist):

- `QUOTED_FAMILIES` `:600-617` — 16 quoted heads incl. the five non-chat families `:612-616` (`Failed to import tag "`, `… roleplay template "`, `… connection profile "`, `… image profile "`, `… embedding profile "`).
- `mask_warning` `:707-760` — for a quoted family, keeps the head + quoted name and masks the tail to `<ENGINE>` (`:746`); the ONE exception `:743-745`: `*family == "Failed to import chat \"" && is_zod_error_message(tail)` keeps the line verbatim.
- `is_zod_error_message` `:690-705` (a non-empty JSON array of objects each with string `code`, array `path`, string `message`).
- `SERDE_ARM_CASE`/`SERDE_ARM_HEAD` `:648-649` + `classify_serde_arm` `:651-688` — the P4.130 chat serde divergence carve (VANISHED if tails agree; WRONG SHAPE unless v4's tail is a Zod message containing `"scenarioText"` and v5's starts ``invalid type: integer `5`, expected a string``); applied at `:1925` for that one case.
- Count pin `assert_eq!(ran, 43, …)` `:1288`; the chat non-vacuity pin `:1292-1330`.

### A2 v4's five sentences (the oracle)

All five catch arms push `` `Failed to import <entity> "${x.name}": ${error instanceof Error ? error.message : String(error)}` `` then `moduleLogger.warn('Failed to import <entity>', { <entity>Id: x.id, error: <same message> })` (`moduleLogger = logger.child({ module: 'import:quilltap-import-service' })`):

| family | v4 catch | warning head | WARN fields |
|---|---|---|---|
| tag | `lib/import/quilltap-import/import-entities.ts:73-82` | `Failed to import tag "` | `{tagId, error}` |
| roleplay template | `import-entities.ts:171-180` | `Failed to import roleplay template "` | `{templateId, error}` |
| connection profile | `import-profiles.ts:116-125` | `Failed to import connection profile "${rawProfile.name}"` | `{profileId: rawProfile.id, error}` |
| image profile | `import-profiles.ts:181-190` | `Failed to import image profile "` | `{profileId, error}` |
| embedding profile | `import-profiles.ts:246-255` | `Failed to import embedding profile "` | `{profileId, error}` |

The throw is the repository create's Zod `validate`: every create is `safeQuery(async () => this._create(…), 'Error creating <entity>', {userId, name[, provider]})` (rethrow mode — `tags.repository.ts:60-89` `'Error creating tag'`; `roleplay-templates.repository.ts:285-305` `'Error creating roleplay template'`; `connection-profiles.repository.ts:67-89` `'Error creating connection profile'` `{userId, name, provider}`; `image-profiles.repository.ts:55-77` `'Error creating image profile'` `{userId, name, provider}`; `embedding-profiles.repository.ts:73-93` `'Error creating embedding profile'` `{userId, name, provider}`), around `_create` (`base.repository.ts:350-379`, its own rethrow `safeQuery(…, this.createErrorMessage())` = `'Error creating entity'` — none of the five overrides `createErrorMessage`), around `validate` (`:130-141`, ERROR `Data validation failed {collection, error}`). The user-scoped wrapper injects `userId` (`lib/repositories/user-scoped.ts:85-87`). **So v4 logs, per refused item: ERROR `Data validation failed`, ERROR `Error creating entity`, ERROR `Error creating <entity>`, then the importer's WARN — and because `executeImport` runs inside `withStrictRepositoryFailures` (`execute.ts:425-431`), the two `safeQuery`-born ERRORs carry `strictFailures: true` (`safe-query.ts:63-68`).**

**The exact tails** for the corpus's planted rows (probe over the real schemas with the data each create validates — `{...data, userId, id, createdAt, updatedAt}` plus the tag create's `nameLower`/`quickHide` and the connection importer's `seedLegacyConnectionProfileFields` + `apiKeyId: null`):

| case / item | v4 tail (`ZodError.message`, `JSON.stringify(issues, null, 2)`) |
|---|---|
| `execute_named_item_failures` · `Broken Tag` (`visualStyle: 'not-an-object'`) | `[{"expected":"object","code":"invalid_type","path":["visualStyle"],"message":"Invalid input: expected object, received string"}]` |
| · `Broken Connection` (`modelName: 42`) | `[{"expected":"string","code":"invalid_type","path":["modelName"],"message":"Invalid input: expected string, received number"}]` |
| · `Broken Image` (`provider: 42`) | `[{"expected":"string","code":"invalid_type","path":["provider"],"message":"Invalid input: expected string, received number"}]` |
| · `Broken Embedding` (`provider: 42`) | `[{"code":"invalid_value","values":["OPENAI","OLLAMA","OPENROUTER","NANOGPT","BUILTIN"],"path":["provider"],"message":"Invalid option: expected one of \"OPENAI\"\|\"OLLAMA\"\|\"OPENROUTER\"\|\"NANOGPT\"\|\"BUILTIN\""}]` |
| · `Broken Template` (`systemPrompt: 42`) | `[{"expected":"string","code":"invalid_type","path":["systemPrompt"],"message":"Invalid input: expected string, received number"}]` |
| `execute_bug105_seed_abort` · `Bug 105 Connection` (`provider: 42`) | `[{"expected":"string","code":"invalid_type","path":["provider"],"message":"Invalid input: expected string, received number"}]` |

(Compact here for width; the real bytes are the 2-space pretty form.) Note the embedding row: `EmbeddingProfileProviderEnum = z.enum([...5])` (`lib/schemas/common.types.ts:35`) gives `invalid_value`, while `ProviderEnum`/`ImageProviderEnum` are `z.string().min(1, …)` (`:29`, `:32`).

The payloads: `harness/oracle/cases/system-import-execute.test.ts` `malformedItemsPayload` `:1290-1360` (case `:2244`) and `bug105SeedAbortPayload` `:1397-1432` (case `:2252`). **Every one of the five families already has a malformed planted row — no new plant is needed for the mask.**

### A3 v5 today

All serde-arm sentences are `serde_json::Error`'s `Display` (`from_value`, no line/column):

| family | v5 site | v5 tail (predicted from the DTO field types) | WARN on the serde arm |
|---|---|---|---|
| tag | `services/quilltap_import/entities.rs:165-171` (create), `:124-130` (duplicate); `ImportedTag.visual_style: Option<tags::TagVisualStyle>` `:82-91` | `invalid type: string "not-an-object", expected struct TagVisualStyle` | **none** (the `:192-195` WARN fires only on the outer `Err`; field `tag_id` snake_case) |
| roleplay template | `entities.rs:364-372`, `:345-356`; `system_prompt: String` `:213` | ``invalid type: integer `42`, expected a string`` | **none** (`:383` outer only, `template_id`) |
| connection profile | `profiles.rs:218-226`, `:189-197`; `parse_connection_profile` `:257-263` | ``invalid type: integer `42`, expected a string`` (both rows) | ONE WARN `Failed to import connection profile {error}` from `:260` — **no `profileId`** |
| image profile | `profiles.rs:411-417`; duplicate `:401-404` | ``invalid type: integer `42`, expected a string`` | **none** |
| embedding profile | `profiles.rs:527-535`; duplicate `:516-519`; `provider: String` `:469` | ``invalid type: integer `42`, expected a string`` | **none** |

No v5 importer logs any of v4's three repository ERRORs on these arms (the serde decode runs before the repository). v5 has no `strictFailures` notion anywhere (`grep strictFailures crates/` = 0).

### A4 Three further divergences the mask hides or no case reaches

1. **The image/embedding DUPLICATE arm swallows the item.** `profiles.rs:401-404` and `:516-519`: `let Ok(p) = serde_json::from_value::<…>(raw.clone()) else { return Ok(()); };` — no warning, no WARN. v4 (`import-profiles.ts:150-160`, `:215-225`) sets the phantom map, calls `create`, the validate throws, the catch pushes the named warning. (The tag/template/connection duplicate arms DO push.) Reachable: `conflictStrategy: 'duplicate'` + a payload item whose id exists in the destination + one wrong-typed field. **No case reaches it today.**
2. **Embedding `provider` outside the enum is ACCEPTED by v5** (`provider: String`, `profiles.rs:469`; the repository stores TEXT). v4 refuses `invalid_value`. A string `"BOGUS"` imports a row on v5 and is a named refusal on v4 — a STATE divergence, not just wording. **No case reaches it.** (The same class, smaller: `ProviderEnum`/`ImageProviderEnum` are `.min(1)`, so an EMPTY-string provider is refused by v4 `too_small` and accepted by v5's `String`.)
3. **The WARN lines** (A3 last column) and v5's snake_case WARN fields (`tag_id`, `template_id`, `profile_id`, `chat_id` — `entities.rs:194/383/732`, `profiles.rs:246/427/546`) where v4 writes `tagId`/`templateId`/`profileId`/`chatId` (§R.6's camelCase rule). No family captures them (the import oracle has no `Logger.prototype` spy — `grep Logger system-import-execute.test.ts` = 0).

### A5 The per-family carve, and the red-first count

- **Widen the exception** to every quoted family (`is_zod_error_message(tail)` → verbatim), not just the chat head. P4.130 measured exactly what that does with no core change: `execute_named_item_failures` and `execute_bug105_seed_abort` red. **Red-first = 6 warnings in 2 cases** (5 + 1; the lane confirms by `grep -c 'Failed to import \(tag\|roleplay template\|connection profile\|image profile\|embedding profile\) "'` over the fresh NDJSON — no other case is predicted to carry one since P4.70 closed the connection-profile vintage gap).
- **The carve**: generalize `classify_serde_arm` into ONE table `SERDE_ARM_DIVERGENCES: &[(case, warning_head, v4_zod_path, v5_serde_prefix)]` — 7 rows (the existing chat row + the six above), each checked exactly like today's (VANISHED if equal; WRONG SHAPE unless v4's tail `is_zod_error_message` with that `path` key and v5's starts with the predicted serde prefix), each replaced with a stand-in on both sides, plus an **exercised-count assert = 7**. Then `QUOTED_FAMILIES` masking stays only for the genuine engine sentences (SQLite constraint text).
- **Mutation proofs**: drop any one table row → that family's warning reds as a verbatim mismatch; re-narrow the exception to the chat head → the six rows report "WRONG SHAPE"/mask mismatch.
- **Closing (not pinning) the divergence** needs a Zod twin of each create schema's field TYPES in schema key order (TagSchema, RoleplayTemplateSchema, Connection/Image/EmbeddingProfileSchema — the connection schema alone is ~40 keys). serde's `from_value` error carries no path, so the serde error cannot be mapped to an issue. The only maintainable route is the "ship the generator" pattern (a tsx dump of each schema's `shape` → a committed Rust table) — Tier 3.
- **New red-first arms for A4** (plants, by addition to the TS payload builders): (i) a `duplicate` case whose `imageProfiles`/`embeddingProfiles` reuse an id the merged export carries with one wrong-typed field → v4 two named warnings, v5 none → **2 warnings red**; (ii) an embedding profile with `provider: 'BOGUS'` → v4 one warning + no row, v5 a row + no warning → **red on `main.embedding_profiles` and on `warnings`**. Count 43 → 45 (or 44 if both go in one case). The lane measures which image/embedding ids the destination fixture holds first (the `execute_duplicate_all` precedent).

---

## §B (c) — the restore orchestrator's serde arm

### B1 v4

`lib/backup/restore/restore.ts:199-240`: `repos.chats.create(withConciergeModeFromLegacy(stripScenarioSeededSummary(chatData)), { id: chat.id })` (`:211`); the catch pushes `` `Failed to restore chat "${chat.title}": ${error instanceof Error ? error.message : String(error)}` `` (`:238`) and `moduleLogger.warn('Failed to restore chat', { chatId: chat.id, error })` (`:239`, the Error OBJECT). `stripScenarioSeededSummary` (`lib/chat/scenario-seeded-summary.ts`) only reads `scenarioText` via `typeof … !== 'string'`, so a numeric `scenarioText` reaches the create untouched. `chats.create` (`lib/database/repositories/chats.repository.ts:233-281`) is `this.safeQuery(…, 'Failed to create chat', {})` around `_create` → `validate` against `ChatMetadataBaseSchema` (`:67`). **Three ERRORs precede the WARN:** `Data validation failed {collection: 'chats', error}` (`base.repository.ts:130-141`), `Error creating entity {collection: 'chats', error}` (`:350-378`), `Failed to create chat {collection: 'chats', error}` (`:237-281` via `base.repository.ts:95-105` → `safe-query.ts:51-72`). Restore is NOT under `withStrictRepositoryFailures` (only `preview.ts`/`execute.ts` enter it), so no `strictFailures` there; the import's chat path DOES carry it on the second and third lines.

Probe (the real `ChatMetadataBaseSchema` over a clone of `restore-archive.zip`'s `c…0001` minus `userId/createdAt/updatedAt/messages`, `withConciergeModeFromLegacy` applied, `scenarioText: 5`, the create's defaults) — the warning tail v4 will produce:

```
[
  {
    "expected": "string",
    "code": "invalid_type",
    "path": [
      "scenarioText"
    ],
    "message": "Invalid input: expected string, received number"
  }
]
```

### B2 v5

`crates/quilltap-core/src/services/backup/restore/orchestrator.rs:462-523`. The `skip` closure `:485-491` calls `chat_override::log_chat_create_validation_failure(error)` (the two ERRORs), pushes `Failed to restore chat "{title}": {error}` and WARNs `Failed to restore chat {chatId, error}`. It is used for the Concierge refusal (`:492-498`, `concierge_columns_zod_error` — v4's bytes) AND the serde arm (`:500-506`, `skip(&mut w, &e.to_string())` — **serde's sentence, ``invalid type: integer `5`, expected a string``, carried by both ERRORs as if it were a ZodError**). The helper (`services/dangerous_content/chat_override.rs:355-376`) logs exactly TWO lines; its unit test `a_refused_chat_create_logs_v4s_two_repository_errors_in_order` `:445-466` asserts `lines.len() == 2`. **`"Failed to create chat"` occurs nowhere in `crates/`.** The import twin `quilltap_import/entities.rs:772-785` uses the same helper on both arms.

(Also seen, recorded only: the restore's `chats.create` DB-error arm `:518-523` pushes the warning with no WARN; the message-replay serde arm `:531` answers serde text where v4's `addMessage` throws a `ChatEventSchema` ZodError — an `invalid_union` with nested `errors` — Tier 3.)

### B3 The plant

`system_restore_state` restores committed zips from `crates/quilltap-web/tests/fixtures/restore-archives/` (`archive_for` `crates/quilltap-harness/tests/system_restore_state.rs:897-948`; the bogus row `:913-914`); oracle `harness/oracle/cases/system-restore.test.ts` `RESTORE_CASES` `:123-~300` (the bogus row `:262-268`). The P4.130 derive is `harness/oracle/fixtures/derive-restore-archive-concierge-bogus.py` (streams `restore-archive.zip` through `zipfile`, appends clone `c1000000-…-0005` "The Bogus Room" `conciergeMode: 'bogus'`, `messages: []`, `messageCount: 0`, `counts.chats` 2 → 3, every other byte and `date_time` kept). **Readers of `restore-archive-concierge-bogus.zip`: `system_restore_state.rs` and `system-restore.test.ts` only.**

**Recommended shape — PLANT, do not rebuild:** a NEW sibling derive `harness/oracle/fixtures/derive-restore-archive-chat-serde-arm.py` writing a NEW `restore-archive-chat-serde-arm.zip` (the same template: clone `c…0001` → `c1000000-0000-4000-8000-000000000006` "The Serde Room", `scenarioText: 5`, `messages: []`, `messageCount: 0`, Concierge columns left valid so the Concierge check passes and the typed decode is reached; `counts.chats` 2 → 3). The committed bogus zip stays byte-identical (re-running the old derive is unnecessary; if the lane chooses a fifth clone in the SAME script instead, it must md5-check that the bogus zip is unchanged). New `RESTORE_CASES` row `restore_chat_serde_arm_replace` + `archive_for` arm; `seen` 21 → 22 with the arithmetic comment `:1285-1297`.

**Red-first prediction:** with the case added and no carve, `compare_warnings` (`:1596-1614`, VERBATIM) reds on exactly **1 warning** in the new case (v5's serde tail vs v4's ZodError tail); the state diff is a plain equality (the chat absent on both sides). The carve: `classify_restore_serde_arm` beside `assert_bogus_concierge_chat_skipped` (`:1305-1360`) — VANISHED if the tails agree, WRONG SHAPE unless v4's tail parses as a Zod message with `"scenarioText"` and v5's starts ``invalid type: integer `5`, expected a string``; replaced with a stand-in on both sides before `compare_warnings`; a by-name non-vacuity pin (chat `c…0006` absent both sides). Mutation: delete the carve → the verbatim compare reds on that one warning.

**The ERROR-line half:** land the third line in `log_chat_create_validation_failure` (`ERROR quilltap::db Failed to create chat collection=chats error=…`, after `Error creating entity`), its unit test 2 → 3 lines in order — this fixes BOTH the restore and the import (the only two callers, `entities.rs:779/784`, `orchestrator.rs:486`). The serde arm's three lines still carry serde text (a recorded divergence until the chat schema twin exists; pin it in the unit test by name). Making it differential needs a `Logger.prototype.error` spy in `system-restore.test.ts` (and `system-import-execute.test.ts`) recording `{message, collection, error, strictFailures}` on the refusal arms + a capture on the Rust side — Tier 2; the import side then also exposes the `strictFailures: true` field v5 cannot emit (Tier 3 — v5 has no strict-scope concept).

---

## §C (i) — `dumpFileFacts`'s tier-3 upgrade

### C1 What `dumpFileFacts` is

**Not a v4 function** (`grep -rn dumpFileFacts ~/source/quilltap-server/lib ~/source/quilltap-server/__tests__` = 0). It is v5's ORACLE helper, `harness/oracle/cases/files-routes.test.ts:216-247` (P4.104): per `files` row matching an `originalFilename`, `{mimeType, blobFound, sizeMatchesBlob, shaMatchesBlob, shaIsInput}` — encoder-neutral booleans, emitted as `fileFacts` for the three `image:` cases (`chat_upload_real_png` `:594-602`, `chat_upload_lossless_webp` `:608-617`, `file_upload_real_png` `:623-631`; payload built at `:428-430`). Its Rust twin is `dump_file_facts` (`crates/quilltap-harness/tests/files_routes_equivalence.rs:47-~80`), compared at `:1632-1638` and `:1693-1699`.

### C2 What covers the describe today, and the vacuity

- **v4 side of `files_routes`:** P4.130 measured (spy + 4 s settle, a throwaway copy): both chat-upload cases log INFO `auto-describe: vision describe did not produce a description {fileEntryId, type: "unsupported", error: "No image description profile available. Configure one in Settings → Chat Settings → Image Description Profile"}` → `describe-failed`, nothing written; the general upload never fires. The committed oracle has NO settle (it dumps straight after the route returns, racing v4's `void autoDescribe…`).
- **v5 side of `files_routes`:** `chat_media::chat_file_upload` (`crates/quilltap-core/src/api/chat_media.rs:1167-1175`) is the seam-less wrapper (`auto_describe: None`) — a new image row skips with a DEBUG line. **Both sides write nothing, so the family agrees vacuously.**
- `chat_upload_auto_describe.rs` (P4.120): the fire set, the `chatId`-less call, the `.catch` WARN, the unarmed skip — **v5-only, no oracle** (its header says so).
- `photo_tools_equivalence` (`harness/oracle/cases/photo-tools.test.ts:466-509`): the REAL module with `generateImageDescription` doMocked at the §C2 seam (`{type: 'image_description', imageDescription, processingMetadata}` / `unsupported`) — module-level, no route, no profile pick.
- `file_attachment_tier3` / `attach_mount_file_equivalence`: v4's real `generateImageDescription` / `processFileAttachmentFallback` against a planted vision profile with `@/lib/llm` `createLLMProvider` canned by (provider, model, attachment filename), recorded as `{kind:"canned", provider, model, temperature, filename, mimeType, content, finishReason, usage}` rows (`file-attachment-tier3.test.ts:227-300`, `attach-mount-file.test.ts:130-160`), replayed by `quilltap_harness::CannedCompletionProvider` — but never through the upload.

### C3 The upgrade

**Family: extend `files_routes_equivalence` (the route-level family), by ADDITION — one new case**, not `file_attachment_tier3` (that family is the context-builder/fallback module; it has no upload route) and not a mutation of the three existing image cases (their `imageFacts` must stay encoder-neutral).

- **v4 side** (`files-routes.test.ts`): (1) a per-case PLANT on the scratch copy (raw SQL, replayed verbatim by the Rust side — the `plantTrail` precedent of `retry_uncensored_tier3`): one vision-capable connection profile + its key, and the user's `chat_settings.imageDescriptionProfileId` pointing at it (copy the column set from `attach-file-*`'s builder — that family solved the transport gate, bug 91: describe through OPENAI, not `OPENAI_COMPATIBLE`); (2) `jest.doMock('@/lib/llm', …)` with the canned `createLLMProvider` + recording (the file-attachment shape); (3) **a deterministic settle**: `doMock('@/lib/photos/auto-describe-attachment')` that `requireActual`s the module and wraps `autoDescribeChatImageAttachment` to push its promise into a list the case awaits — never a sleep; (4) the dump extended for that case: `files.description` (by `originalFilename`), the five `doc_mount_file_links` columns (`description`, `descriptionUpdatedAt` → `<ts>`, `extractedText`, `extractedTextSha256`, `extractionStatus`) and `count(*)` of `doc_mount_chunks` for the link, plus the `kind:"canned"` rows. The storage manager is already `requireActual` there (`applyMocks` `:126`), so the module's `fileStorageManager.downloadFile` reads real bytes (not jest.setup's "mock file content" stub — verify by the canned row's `mimeType`).
- **Rust side**: `chat_file_upload_with_auto_describe(…, Some(UploadAutoDescribe { bytes, describe: Some(Arc::new(TestDescribeRunner{ db, user_id, completion: CannedCompletionProvider from the oracle's canned rows })), side_effects }))` with the thread-scoped background queue (`arm_background_spawner_for_current_thread`, drained to completion — `chat_upload_auto_describe.rs:65-72` shape); `TestDescribeRunner` is `attach_mount_file_equivalence.rs:131-155`'s restatement of `HostImageDescribeRunner` (`crates/quilltap-host/src/spine.rs:906-949`).
- **Canned reply shape:** `{ content: "<a fixed description>", finishReason: "stop", usage: {promptTokens, completionTokens, totalTokens} }`, keyed `(provider, model, temperature, filename, mimeType)` — **the key must not include bytes**: v4's attachment is real-sharp WebP and v5's the host codec's (D19), so the image payload can differ while the filename/mime (`image/webp` after the transcode) agree. Use the 1×1 PNG (`PNG_1X1_BASE64` `:194`) or `PHOTO_PNG`, and record which.
- **What it proves that nothing proves today:** the COMPOSED production chain v4 runs on every chat image upload — `uploadFileToProject` → the `chatId`-less background describe → the real profile pick (image-description profile → cheap vision → first vision) → the real vision call → `files.description` + the five link columns + the chunk pass — against v4's REAL route, instead of three partial pins (module with the describer mocked; describer with no route; fire set with no oracle) and a vacuous files-family agreement.
- **Red-first:** none expected on v5 production code (P4.120 ported the chain); the arm reds first on the HARNESS (today v5's family passes `None`). A mutation proof that bites: make the spawned call pass `Some(chat_id)` (`chat_media.rs:1177-1232`) or skip the link-column persist in `photos/auto_describe_attachment.rs` → the new case reds on the dump. Risk: v4's chunk pass may enqueue embeddings through the doMocked scheduler (`:143`) — the dump must not read `jobs`.

---

## §D (j) — the trail WARN's `errors`

### D1 v4

`lib/database/repositories/chats-messages.ops.ts:343-356`, inside the fallback `safeQuery(…, 'Failed to get messages for chat', { chatId }, [])` (`:320-358`):

```ts
const result = ChatEventSchema.safeParse(msg);
if (result.success) { validMessages.push(result.data); }
else {
  logger.warn('Skipping corrupted chat message', {
    chatId,
    messageId: msg?.id || 'unknown',
    messageType: msg?.type || 'unknown',
    errors: result.error.issues.map(i => `${i.path.join('.')}: ${i.message}`),
  });
}
```

`logger` is the root `@/lib/logger` (`:18`) — **no `context`/`module` field** (`grep 'db.chats' lib/` = 0). The field is `errors`, an ARRAY.

**The decisive fact the recorded description missed:** `ChatEventSchema = z.union([MessageEventSchema, ContextSummaryEventSchema, SystemEventSchema])` (`lib/schemas/chat.types.ts:637-641`) — a plain union, and zod 4.6.5's `handleUnionResults` (`node_modules/zod/v4/core/schemas.js:1195-1214`) returns the ONE non-aborted option's issues when exactly one option is non-aborted, else ONE `invalid_union` issue (`path: []`, `message: "Invalid input"`). `util.aborted` (`util.js:520-529`) is true when any issue lacks `continue: true` — i.e. every type/enum/literal issue. The other two members always abort on the `type` literal, so: **all issues of the row's member are non-aborting checks → those issues, ALL of them, in schema key order; any aborting issue → `[": Invalid input"]`**. Probe (the real schema, zod 4.6.5):

| row (chats-messages-ops spec read 4, `c00000b0…`) | v4 `errors` | v5 today (`error=` field) |
|---|---|---|
| 01 valid two-row trail | kept | kept |
| 02 `profileId: "not-a-profile"` | `["routeTrail.0.profileId: Invalid UUID"]` | `routeTrail.0.profileId: Invalid UUID` |
| 03 201-char `detail` | `["routeTrail.0.detail: Too big: expected string to have <=200 characters"]` | same text |
| 04 `trigger: null` | `[": Invalid input"]` | `routeTrail.0.trigger: Invalid option` |
| 05 199×`x` + astral | kept | kept |
| 06 bad `via` in the 2nd element | `[": Invalid input"]` | `routeTrail.1.via: Invalid option` |
| 07 `{"not":"an array"}` | `[": Invalid input"]` | `routeTrail: not an array ({…})` |

Other shapes measured with the same probe (the reads 1–3 of the same spec): bad `role` → `[": Invalid input"]`; non-uuid `id` → `["id: Invalid UUID"]`; `hostEvent: 42` / bad `toStatus` → `[": Invalid input"]`; `hostEvent.participantId` non-uuid → `["hostEvent.participantId: Invalid UUID"]`; a bad `introducedCharacterIds[1]` → `["hostEvent.introducedCharacterIds.1: Invalid UUID"]`; a `createdAt` without seconds / `yesterday` on a message, context-summary or system row → `["createdAt: Invalid ISO datetime"]`; non-uuid `participantId` → `["participantId: Invalid UUID"]`; NULL `content` / unknown `type` / non-numeric `detail` → `[": Invalid input"]`; **non-uuid `id` + a 201-char `detail` → BOTH lines** `["id: Invalid UUID","routeTrail.0.detail: Too big: expected string to have <=200 characters"]` (v5 reports only the first).

### D2 v5

The ONE WARN site is `crates/quilltap-core/src/db/chats_messages_read.rs:542-566` (`get_messages_strict`; `get_messages` `:463-477` delegates): `tracing::warn!(target: "quilltap::db", context = LOG_CONTEXT, chatId, messageId, messageType, error = %error, "Skipping corrupted chat message")` — field `error` (a string), plus a v5-invented `context = "db.chats-messages"` (`:392`). The string comes from `RowOutcome::Corrupted{error}` (`:398-406`) — either a cell-marshal error or `zod_shape_failure` (`:323-387`), a FIRST-failure stand-in over five shapes (`id`, `createdAt`, `role`, `participantId`, `routeTrail` via `api::zod_issues::zod_route_attempt_failure` `:680-772`, `hostEvent`). `zod_shape_failure` has a second caller, `db/chats_messages.rs:618` (`updateMessage`'s merged-event check → `DbError::Internal("updateMessage parse: …")`); `zod_route_attempt_failure` a second caller, `api/chat_media.rs:1972` (the `saved_trail` filter, `.is_none()` only). The WARN location in the brief (`services/route_trail.rs` / `api/chat_media.rs`) holds no WARN.

### D3 The home and the fix

- **Renderer exists:** `api/zod_issues.rs:869-878` `zod_issue_lines(issues, separator)` — `issues.map(i => `${path.join('.')}${sep}${message}`)`, numeric indices bare via `join_path` `:880-889`. Call it with `": "`. **The issue SOURCE does not exist:** `zod_route_attempt_failure` returns `Option<String>` (first failure, pre-rendered), and `zod_shape_failure` the same.
- **Shape:** a `zod_chat_event_issues(&Value) -> Vec<ZodIssue>` home (in `api/zod_issues.rs`, beside the route-attempt twin, which becomes `zod_route_attempt_issues(&Value, path_prefix) -> Vec<ZodIssue>`) that collects EVERY issue of the checked shapes in `MessageEventSchema` key order (`id`, `role`, `content`, …, `createdAt`, `participantId`, `routeTrail`, `hostEvent`; within an element `profileId`, `profileName`, `provider`, `modelName`, `via`, `outcome`, `trigger`, `evidence`, `profileKind`, `detail`), marks each aborting (type/enum) or not (uuid/datetime format, `too_big`), and collapses to one `{path: [], message: "Invalid input"}` when any is aborting — the WARN then logs `errorsJson = %serde_json::to_string(&zod_issue_lines(&issues, ": "))` (the `…Json` file-layer convention, `crates/quilltap-web/src/log_file.rs:1112-1160`, rendering `errors: [...]`). A cell-marshal failure and an unknown `type` are always the collapsed line. `zod_shape_failure` keeps its `Option<String>` contract for `chats_messages.rs:618` (derived from the same issues — e.g. the joined lines) so that file need not move; the `saved_trail` filter keeps `…failure(row).is_none()` or switches to `issues.is_empty()`.
- **The pin:** `chats_messages_ops_tier2` drives v4's REAL `getMessages` (`harness/oracle/cases/chats-messages-ops-tier2.ts:139-152`, a `Logger.prototype.error/warn` spy recording only `chatId`/`messageId`/`messageType` `:146-148`): add `'errors'` to the recorded keys; the Rust comparator (`crates/quilltap-harness/tests/chats_messages_ops_tier2_equivalence.rs:728-779`, `assert_reads`, checks `key=value` substrings `:758-762`) compares the captured `errorsJson` parsed as an array. **Red-first: every WARN of the four `getMessages` reads (spec ops 15/30/44/54) — read 4 is exactly 5 lines; reads 1–3 ≈ 1 / 12 / 6 (count from the fresh NDJSON); pre-fix v5 carries no `errors` field at all.** Unit tests in `chats_messages_read.rs` (the existing `a_null_content_row_is_skipped…` `:845-883` and `an_unknown_type_row…` `:885-915` gain the `errorsJson` bytes) plus a planted bad-trail row per class (collapsed vs check-only vs two-check). No other family captures the line (`retry_uncensored_tier3` reaches the skip through v4's real route but records only the 404).
- **Mutation proofs:** (M-a) never collapse → rows 04/06/07 red; (M-b) first-failure only → the two-check row (add one: non-uuid `id` + 201-char `detail`) red; (M-c) drop the element index from the path → 02/03 red.

---

## §E P4.130's other OPEN-by-name items

| item | measurement | recommendation |
|---|---|---|
| `GroupSchema`'s five store-resident columns (`description`, `instructions`, `state`, `color`, `icon`) | They ARE columns in the DDL (`fresh_schema.json` main `groups`: `description TEXT`, `instructions TEXT`, `state TEXT DEFAULT '{}'`, `color TEXT`, `icon TEXT`), but v4 strips them on write (`GroupRowSchema` + `GROUP_STORE_MANAGED_FIELDS`, `lib/schemas/group.types.ts:52-89`), so on any v4-written row they read NULL → `undefined` and pass; only a pre-overlay or hand-planted row could fail `color: HexColorSchema` / `icon.max(50)` / `description.max(2000)` / `instructions.max(10000)`, and which value wins (column vs store overlay) on such a row is unmeasured. | **Defer** (unreachable on v4-written data; needs the overlay-precedence measurement first). |
| Retyping `ToolMetadata.route_trail` | The struct is `services/tool_execution.rs:124-134` (NOT `tools/executor.rs`), and `route_trail` is ALREADY `Vec<RouteAttempt>`; `tools/executor.rs:1927-1933` only clones `out.route_trail`. Nothing to retype. | **Retire** (record the correction in P4.130's header). |
| The v4-only `characters` validation line | v4 `characters.findByIdRaw` → `_findById` → `validate` → ERROR `Data validation failed {collection: characters, error: <CharacterSchema ZodError>}` before `Error finding entity by ID`; v5's mount pool reads `characters_read::find_by_id_raw` (`db/characters_read.rs:291-293`) through `db::fallback::find_by_id_or_none` (`services/scenario_builder/mount_pool.rs:133-135`), so a BLOB `name` is a cell error and no validation line. Pinned both ways as `V4_ONLY_VALIDATION` (`scenario_builder_mount_pool_equivalence.rs:134`). Closing it needs a `CharacterSchema` Zod twin (the largest schema in v4). | **Defer** (needs the schema-twin generator; touches `db/characters_read.rs`, beside P4.142's character-overlay batch reads). |

---

## What the recorded description got wrong

1. **(j) "v4 logs the full `path: message` array"** — only for check-only failures. `ChatEventSchema` is a plain `z.union`; zod 4.6.5 collapses any aborting failure to the single `": Invalid input"`. Three of P4.130's five planted trail rows (04 null trigger, 06 bad `via`, 07 non-array) log `[": Invalid input"]`, and for the other two (02, 03) v5's first-failure stand-in text already EQUALS v4's sole line. The real gaps are the field (`errors` array vs `error` string), the collapse, and multi-check rows. Also "the home can render it now": `zod_issue_lines` renders, but no chat-event ISSUE source exists — the trail twin returns a pre-rendered first-failure string.
2. **(j) the WARN's location** — it is `db/chats_messages_read.rs:554-563` alone; `services/route_trail.rs` and `api/chat_media.rs` log no such WARN. And the v5 line carries a v5-invented `context = "db.chats-messages"` v4 does not log.
3. **(c) "v5's two `quilltap::db` ERRORs"** — v4 logs THREE on a refused chat create (`Failed to create chat` from `chats.repository.ts`'s outer `safeQuery`); v5's helper and its unit test pin two. On the import path the second and third also carry `strictFailures: true` (`execute.ts:430`), which v5 cannot express.
4. **(i) `dumpFileFacts`** is v5's oracle helper (`files-routes.test.ts:216-247`), not a v4 function; and the family's current agreement is vacuous on BOTH sides (v4 `describe-failed`, v5 seam-less).
5. **(b)** the recorded five-family list holds, but the mask also hides nothing about two STATE-level divergences no case reaches (the image/embedding duplicate arms swallow silently; embedding `provider` outside the enum imports on v5), and four of the five serde arms log no WARN (connection logs one without `profileId`).
6. **`ToolMetadata.route_trail`** lives in `services/tool_execution.rs` and is already typed — the OPEN item is empty.

Re-verified and holding: the chat-only exception and its two red cases (`system_import_state.rs:735-745`); `classify_serde_arm`'s recorded v5 sentence; the restore `skip` closure's dual use (`orchestrator.rs:485-506`); `restore-archive-concierge-bogus.zip`'s derive and its sole reader; `zod_issue_lines` as the renderer.

## Proposed tiered deliverables

**Tier 1 — must land**
1. (b) Widen `mask_warning`'s exception to every quoted family with a Zod-message tail; generalize `classify_serde_arm` into a 7-row `SERDE_ARM_DIVERGENCES` table with an exercised-count assert; red-first 6 warnings in `execute_named_item_failures` (5) + `execute_bug105_seed_abort` (1) before the table lands. Family: `system_import_state`.
2. (c) NEW derived `restore-archive-chat-serde-arm.zip` + NEW `derive-restore-archive-chat-serde-arm.py`; `RESTORE_CASES` row + `archive_for` + 21 → 22; `classify_restore_serde_arm` pinned both ways + a non-vacuity pin; red-first 1 warning. Family: `system_restore_state`.
3. (c) The third ERROR `Failed to create chat {collection: chats, error}` in `log_chat_create_validation_failure`, the unit test 2 → 3 in order, the serde-arm text recorded as a divergence in that test.
4. (j) `zod_chat_event_issues` (+ `zod_route_attempt_issues`) in `api/zod_issues.rs` with the union collapse; the WARN logs `errorsJson` via `zod_issue_lines(…, ": ")`; the ops oracle records `errors`; the comparator compares it; red-first on every WARN of the four reads (read 4 = 5); M-a/M-b/M-c; a two-check planted row added by addition. Family: `chats_messages_ops_tier2`.

**Tier 2 — should land**
5. (b) The image/embedding duplicate-arm warnings (`profiles.rs:401-404`, `:516-519`) red-first on a new `duplicate` case (2 warnings).
6. (b) The embedding `provider` enum check (`invalid_value` with v4's five values) red-first on a planted `'BOGUS'` (state + warning); record the `.min(1)` empty-provider class.
7. (b) v4's WARN on every serde arm with camelCase ids (`tagId`, `templateId`, `profileId`, `chatId`), `parse_connection_profile`'s profile-less WARN folded; unit capture pins (no oracle spy exists — add a `Logger.prototype.warn/error` spy to `system-import-execute.test.ts` if the lane takes the ERROR half too).
8. (i) The `files_routes` tier-3 case (§C3): a planted vision profile + `imageDescriptionProfileId`, the canned `@/lib/llm`, the promise-capturing settle, the extended dump; Rust through `chat_file_upload_with_auto_describe` + the queue spawner + `CannedCompletionProvider`.
9. (c) The restore/import `Logger.prototype.error` spy recording the three repository ERRORs on the refusal arms, compared byte-for-byte on the Concierge arm and pinned as a divergence on the serde arm.

**Tier 3 — loud deferrals**
10. Closing the serde-vs-Zod divergence for all six create families (and the chat) via a committed, generated schema-shape table — the "ship the generator" route.
11. `strictFailures: true` on the import's `safeQuery`-born lines (v5 has no strict-scope concept).
12. The `updateMessage` ERROR's `error` as v4's full `ZodError.message` (an `invalid_union` with nested `errors` of all three members) — `db/chats_messages.rs:618`.
13. The restore message replay's serde arm (`orchestrator.rs:531`).
14. The v5-invented `context = "db.chats-*"` field on the `db::chats_messages_read` / `chats_messages` / `chats_search` lines (a house shape pinned by tests at `chats_messages_read.rs:830`, `chats_messages.rs:1382`) — record; a file-family-wide ruling, not this lane.
15. `GroupSchema`'s store-resident columns; the v4-only `characters` validation line (§E). Retire the `ToolMetadata.route_trail` item.

## Files the lane would edit

Core (`crates/quilltap-core/src/`):
- `api/zod_issues.rs` — the chat-event / route-attempt issue sources; `zod_route_attempt_failure` re-derived from them.
- `db/chats_messages_read.rs` — `zod_shape_failure` over the issues (contract kept), `RowOutcome::Corrupted` carrying the lines, the WARN's `errorsJson`, unit tests.
- `services/dangerous_content/chat_override.rs` — `log_chat_create_validation_failure`'s third line + its test.
- `services/quilltap_import/profiles.rs` — Tier 2 items 5/6/7.
- `services/quilltap_import/entities.rs` — Tier 2 item 7 (WARNs, camelCase).
- Conditional, only if a signature moves: `db/chats_messages.rs` (`:618`), `api/chat_media.rs` (`:1972`). `services/backup/restore/orchestrator.rs` needs NO edit for Tier 1 (the carve is harness-side; the third line lands in the shared helper).

Harness / oracle:
- `crates/quilltap-harness/tests/system_import_state.rs` (mask, table, count 43 → 45, new non-vacuity pins).
- `harness/oracle/cases/system-import-execute.test.ts` (Tier 2 duplicate + enum payloads; optional Logger spy).
- `crates/quilltap-harness/tests/system_restore_state.rs` (case, `archive_for`, 21 → 22, carve, pin).
- `harness/oracle/cases/system-restore.test.ts` (`RESTORE_CASES` row; optional Logger spy).
- NEW `harness/oracle/fixtures/derive-restore-archive-chat-serde-arm.py`; NEW `crates/quilltap-web/tests/fixtures/restore-archives/restore-archive-chat-serde-arm.zip` (DERIVED, added — no committed fixture is rebuilt).
- `crates/quilltap-harness/tests/chats_messages_ops_tier2_equivalence.rs` + `harness/oracle/cases/chats-messages-ops-tier2.ts` (+ `harness/oracle/fixtures/chats-messages-ops-tier2.json` grows by ADDITION: the two-check row).
- Tier 2 item 8: `crates/quilltap-harness/tests/files_routes_equivalence.rs` + `harness/oracle/cases/files-routes.test.ts` (+ `harness/oracle/fixtures/files-web.json` by addition for the vision spec). The committed `files-main.db`/`files-mount.db` are NOT rebuilt — the profile is a per-case plant (they have five other readers: `chat_upload_auto_describe`, `embedding_reapply_equivalence`, `embedding_profiles_routes_equivalence`, `memory_dedup_equivalence`, `crates/quilltap-host/tests/host_generated_image_placeholder_heal.rs`).

Families regenerated from the lane's pin: `system_import_state`, `system_restore_state`, `chats_messages_ops_tier2`, (Tier 2) `files_routes`; neutrality: `retry_uncensored_tier3` (reads the strict twin), `repository_zod_messages`, `scenario_builder_mount_pool`, `scenario_builder_routes` (web crate), `system_restore_equivalence`, `restore_vintage_state`; guards: `zod_issues_home_guard` (constructor census 5/1/1 and parsed-type 6/6 must stay — new fns must not be named `invalid_type(`/`parsed_type(`/`received(`), `get_messages_caller_census` (UNMOVED), `concierge_state_writers_census`.

## Files the lane must READ but not edit

`services/route_trail.rs` (`RouteAttempt::from_value`), `llm_fallback/types.rs:51-62`, `services/dangerous_content/refusal.rs` (`RefusalEvidence::from_wire`), `services/backup/restore/orchestrator.rs`, `db/chats_messages.rs` (unless the conditional fires), `photos/auto_describe_attachment.rs`, `services/file_fallback.rs`, `api/chat_media.rs:1158-1240`, `crates/quilltap-host/src/spine.rs:906-949`, `crates/quilltap-harness/tests/{attach_mount_file_equivalence.rs,chat_upload_auto_describe.rs,file_attachment_tier3_equivalence.rs}`, `harness/oracle/cases/{attach-mount-file.test.ts,file-attachment-tier3.test.ts,photo-tools.test.ts}`, `harness/oracle/fixtures/derive-restore-archive-concierge-bogus.py`, `crates/quilltap-web/src/log_file.rs`; v4 `lib/import/quilltap-import/{import-entities.ts,import-profiles.ts,execute.ts}`, `lib/backup/restore/restore.ts`, `lib/database/repositories/{base.repository.ts,safe-query.ts,chats.repository.ts,chats-messages.ops.ts,tags.repository.ts,roleplay-templates.repository.ts,connection-profiles.repository.ts,image-profiles.repository.ts,embedding-profiles.repository.ts}`, `lib/schemas/{chat.types.ts,profile.types.ts,tag.types.ts,template.types.ts,common.types.ts}`, `node_modules/zod/v4/core/{schemas.js:1195-1214,util.js:520-529}`.

## Cross-lane adjacencies / risks

- **P4.140 owns `orchestrator.rs`** — that is `services/orchestrator.rs` (the Salon spine). This lane's restore file is `services/backup/restore/orchestrator.rs` and needs NO Tier 1 edit; the ownership table should spell the full path to avoid a false collision.
- **P4.142** owns `db/fallback.rs` and the doc-mount repos and the character overlay's batch reads. This lane edits neither; the §C3 dump reads `doc_mount_file_links`/`doc_mount_chunks` by raw SQL in the harness only. The deferred `characters` validation line (§E) sits in `db/characters_read.rs`, beside P4.142's work — leave it.
- **P4.139** (`api/settings.rs`, `db/api_keys.rs`, an API-key read census): `quilltap_import/profiles.rs` forces `apiKeyId: null` and reads no key — if P4.139's census scans `services/quilltap_import/**`, the Tier 2 edits there must not add a key read. §C3's planted vision profile needs a key row written by raw SQL in the harness (no core read path changes).
- **P4.141** (model layer): §C3 drives `CompletionProvider` through `CannedCompletionProvider`; a trait change in P4.141 would ripple into `files_routes_equivalence.rs` — land §C3 after P4.141 or keep to the existing trait methods.
- **P4.144** widens `ALIAS_ASSIGN` in `harness/tools/recipe_sweep.py`: the recipe headers of `system_restore_state`, `system_import_state`, `chats_messages_ops_tier2`, `files_routes` must still extract under the widened parser (`--self-test`).
- **Shared fixture readers:** `chats-messages-ops-tier2.json` and the restore archives directory are read only by this lane's families (plus `system_restore_equivalence`/`restore_vintage_state`/`p4_9g6_seam_contract` reading OTHER archives by name). Nothing is rebuilt.
- `log_chat_create_validation_failure` is called from the import AND the restore — the third line moves both; no sibling lane touches `chat_override.rs`.

## Versions / bumps

- `quilltap-core` (0.0.1146 at survey) — Tier 1 items 3–4, Tier 2 items 5–7.
- `quilltap-harness` (0.0.1074) — every harness family edit.
- `quilltap-web` (0.0.207) — tests-only, for the NEW committed archive under `crates/quilltap-web/tests/fixtures/restore-archives/` (the P4.130 §R.8 precedent).
- host / cli / tauri / SPA / sqlite3mc-sys: none.
