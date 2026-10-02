# Survey — P4.142: the repository fallbacks — `doc_mount_chunks`, the overlay's batch reads (`send_mail`), `chatGet`/`listChats` under a renamed mount-index column, the 130-site census, the sync applier ruling

**Date:** 2026-10-02 · **v4:** f6426e196 (tree dirty by the three recorded docs paths) · **v5 main:** cb9ecf256 · **Kind:** read-only measurement — `sed`/`grep` over both trees at those commits; nothing built, run, or regenerated. Every v4 path is relative to `~/source/quilltap-server`, every v5 path to this repo.

## The finding in one line

All three named divergences are ONE missing layer: v4's **batch** document reads (`findManyByMountPointsAndPath` / `findManyByMountPointsInFolder`, fallback `withRawDb`s) and its **chunk** reads fall back where v5 propagates — and the 130-site census cannot see either, because it follows four methods only. Converting the two batch reads closes `send_mail` (v4 drops every vaulted character → `No soul by that name…`) **and** `listChats` (v4 answers **200** with the vaulted characters and projects dropped). `chatGet` is **not** a status divergence: v4 answers **500** too (`handleGetChat`'s own catch, `Failed to fetch chat`). What differs there is the body and the log lines. v4's `walkStore` reads the store through the same fallbacks, so a failed store read in v4 plans the deletion of the whole disk side. The sync ruling therefore covers the walker as well as the applier's five reads.

---

## §A (e1) `doc_mount_chunks` — v4's reads, v5's reads

### A1 v4 (`lib/database/repositories/doc-mount-chunks.repository.ts`)

`DocMountChunksRepository extends AbstractDedicatedDbRepository` (`:45-55`), collection `doc_mount_chunks`, `dbTarget: 'mountIndex'`. `safeQuery` injects `collection` first (`base.repository.ts:98-104`); the standalone logs `logger.error(errorMessage, {...context, error: extractErrorMessage(error)})` at **ERROR** and answers the fallback when one was given (`safe-query.ts:57-71`).

| v4 method | lines | shape | the line that actually fires on a read failure | fallback |
|---|---|---|---|---|
| `findByLinkId` | `:97-110` | `safeQuery(findByFilter(…, {sort}), 'Error finding chunks by link ID', {linkId}, [])` | **`Error finding entities by filter {collection: 'doc_mount_chunks', error}`**: the inner `findByFilter` is itself a fallback (`base.repository.ts:283-297`, context `{}`), so the outer line is **UNREACHABLE** | `[]` |
| `findByMountPointId` | `:115-122` | `safeQuery(findByFilter({mountPointId}), 'Error finding chunks by mount point ID', {mountPointId}, [])` | **the same `Error finding entities by filter {collection, error}`**: the outer `Error finding chunks by mount point ID` is UNREACHABLE | `[]` |
| `countEmbeddedByMountPointIds` | `:130-153` | `withRawDb(result, fn, 'Error counting embedded chunks by mount point IDs', {mountPointIdCount})` (default mode = fallback, `dedicated-db.repository.ts:198-226`) | `Error counting embedded chunks by mount point IDs {collection, mountPointIdCount, error}` | empty `Map` |
| `searchContent` | `:172-215` | `withRawDb([], …, 'Error searching chunk content', {mountPointIdCount, queryLength})` | `Error searching chunk content {collection, mountPointIdCount, queryLength, error}` | `[]` |
| `findAllWithEmbeddingsByMountPointIds` | `:220-245` | outer fallback around a loop of `findByFilter` | one `Error finding entities by filter` per mount point (the outer line is unreachable) | `[]` |
| `findById` (inherited) | `base.repository.ts:204-206` → `_findById` `:247-257` | `'Error finding entity by ID', {id}, null` | `Error finding entity by ID {collection: 'doc_mount_chunks', id, error}` | `null` |
| `clearEmbeddingsByLinkId` (a WRITE) | `:256-276` | `withRawDb(0, …, 'Error clearing embeddings by link ID', {linkId})`, fallback mode | `Error clearing embeddings by link ID {collection, linkId, error}` | `0` |
| `deleteByLinkId`, `deleteByMountPointId`, `updateEmbedding`, `bulkInsert` | `:281-317,322-341,364-390` | **3-arg (rethrow)** | log, then rethrow | — |

`withRawDb` answers its fallback **quietly**, with a DEBUG line `Dedicated database unavailable; answering with the fallback`, when `acquireDb()` throws (a degraded or absent mount index, `dedicated-db.repository.ts:205-216`). Only a failure **inside** the operation logs the ERROR line.

v4 callers of the chunk reads, each mapped to its v5 twin:

| v4 site | v4 read | v5 twin (today) |
|---|---|---|
| `lib/mount-index/embedding-scheduler.ts:60` | `findByMountPointId` | `services/mount_index/embedding_scheduler.rs:166` `find_rows_by_mount_point_id(..)?` |
| `lib/mount-index/reindex.ts:219` | `findByMountPointId` | `services/mount_index/reindex.rs:297-298` `…?` |
| `lib/background-jobs/handlers/embedding-reindex.ts:295` (inside a try, `:290-305`, whose catch logs `[EmbeddingReindexAll] Failed to process document mount chunks`, unreachable on a chunk-read failure) | `findByMountPointId` | `services/embedding_reindex_job.rs:559-562` `…)?` |
| `lib/background-jobs/handlers/embedding-generate.ts:570` (`null` → WARN `[EmbeddingGenerate] Mount chunk not found` + `markAsFailed`) | `findById` | `services/embedding_generate_job.rs:917-921` `.map_err(db_str)?`: a read failure fails the JOB (a retry) where v4 marks the entity FAILED |
| `lib/characters/archive-service.ts:844` | `findByLinkId` | `services/character_archive/service.rs:1375` `find_ids_by_link_id(..)?` |
| `app/api/v1/mount-points/route.ts:60` | `countEmbeddedByMountPointIds` | `api/mount_points.rs:116` `count_embedded_by_mount_point_ids(conn,&ids)?` |
| `app/api/v1/mount-points/[id]/route.ts:69` | `findByMountPointId` | `api/mount_points.rs:148` `count_nonempty_embeddings_by_mount_point_id(..)?` |
| `lib/database/repositories/doc-mount-points.repository.ts:188` (`refreshStats`, a 3-arg rethrow `safeQuery` `'Error refreshing mount point stats' {id}`, `:182-201`). A failed inner chunk read **answers `[]` and WRITES `chunkCount: 0`** | `findByMountPointId` | `db/doc_mount_points.rs:359-379` raw `SELECT COUNT(*) FROM doc_mount_chunks`, propagates |
| `lib/mount-index/document-text-search.ts:150` | `searchContent` | `db/doc_mount_chunks.rs:342-366`, already a fallback but on the **wrong line** (§A3) |
| `lib/mount-index/mount-chunk-cache.ts:45` | `findAllWithEmbeddingsByMountPointIds` | none: v5 has no in-memory chunk cache (`photos/mount_embedding_effects.rs:10-14`) |

### A2 v5 today (`crates/quilltap-core/src/db/doc_mount_chunks.rs`)

These PROPAGATE (`Result<_, DbError>`, no line): `find_rows_by_mount_point_id` `:306-317`, `find_row_by_id` `:432-445`, `find_ids_by_link_id` `:479-488`, `clear_embeddings_by_link_id` `:491-498`, and the free fns `count_embedded_by_mount_point_ids` `:530-556` and `count_nonempty_embeddings_by_mount_point_id` `:561-573`. `search_content` `:342-366` is the only one that falls back.

### A3 v5's one existing chunk fallback logs the wrong line

`search_content` `:352-363` emits `tracing::error!(target: "quilltap::doc_mount_chunks", collection, mount_point_id_count, query_length, error = %e, "Error searching chunk content")`. Measured against v4 `:212-213`, it is wrong three ways:

- The target is the module, not `quilltap::db`.
- The fields are snake_case. v4 logs `mountPointIdCount`/`queryLength`, and the camelCase rule is §R.6 of every round.
- `error = %e` renders `DbError::Sqlite`'s `sqlite error: ` prefix. That is the exact defect the `97b25fc53` smalls unification fixed in the home (`fallback.rs:24-39`).

Its sibling `db/doc_mount_file_links.rs:1367-1374` (`Error searching file links by name or path`, v4 `doc-mount-file-links.repository.ts:626-627` `{mountPointIdCount, queryLength}`) carries the same three defects. **`fallback_home_guard` cannot see either of them**, because it scans only `HOME_MESSAGES` (`fallback_home_guard.rs:26-44`). The only pin, `doc_mount_chunks.rs:822-830` `a_broken_table_answers_empty_not_an_error`, asserts the empty result and no line.

### A4 The home shape

- `findByMountPointId` / `findByLinkId` use the existing `fallback::find_by_filter_or_empty("doc_mount_chunks", …)` (`fallback.rs:84-97`).
- `findById` uses the existing `find_by_id_or_none("doc_mount_chunks", id, …)` (`:43-58`).
- **NEW home fns** (this lane owns `fallback.rs`):
  - `count_embedded_chunks_or_empty(mount_point_id_count, read)` → `Error counting embedded chunks by mount point IDs {collection: "doc_mount_chunks", mountPointIdCount, error}`;
  - `search_chunk_content_or_empty(mount_point_id_count, query_length, read)` → `Error searching chunk content {collection, mountPointIdCount, queryLength, error}`, which folds `:352-363`;
  - `search_file_links_by_name_or_path_or_empty(…)` → `Error searching file links by name or path {collection: "doc_mount_file_links", mountPointIdCount, queryLength, error}`, which folds `doc_mount_file_links.rs:1367-1374`;
  - (Tier 2) `clear_embeddings_by_link_id_or_zero(link_id, read)` → `Error clearing embeddings by link ID {collection, linkId, error}`.
- `queryLength` is `query.length`, i.e. UTF-16 units; v5's `jsstr::utf16_len` (`:358`) is already right.

### A5 The `mount_embedding_effects` re-poison: what it asserts and how it moves

`photos/mount_embedding_effects.rs:283-327` `a_failed_enqueue_warns_with_v4s_field_name`:

1. It hands one enqueue to an armed spawner.
2. It runs `DROP TABLE doc_mount_chunks` on the mount index (`:299-309`).
3. It runs the task under `captured_with` and asserts EXACTLY one `WARN … failed to enqueue embedding jobs for mount` line, carrying `mountPointId=mp-1` and no `mount_point_id=` (`:318-326`).

The failure path is `enqueue_embedding_jobs_for_mount_point` → `chunks_repo.find_rows_by_mount_point_id(..)?` (`embedding_scheduler.rs:166`) → `Err` → the `db.write` result `Err` → the warn (`mount_embedding_effects.rs:67-76`).

**Once `:166` takes the twin:** the read logs `ERROR quilltap::db Error finding entities by filter collection=doc_mount_chunks error=no such table: doc_mount_chunks` and answers `[]`. Then `unembedded.is_empty()` → `Ok(0)` (`:171-173`), and the warn never fires. **The test goes red at `assert_eq!(warns.len(), 1)` with 0.** That is the predicted red-first.

**No v4-reachable poison remains for that warn.** In v4 every read in `enqueueEmbeddingJobsForMountPoint` is a fallback: links `:33` (queryJoined), chunks `:60`, `embeddingProfiles.findAll()` `:77` and `users.findAll()` `:89`. Both of the last two reach `_findAll`'s fallback (`base.repository.ts:263-277`; `users.repository.ts:77-83`'s outer rethrow-mode `'Error finding all users'` is therefore unreachable). The per-chunk enqueue is try/caught (`:104-121`). So v4's `.catch` WARN (`save-image-to-album.ts:319-324`) cannot fire on a database failure at all.

The honest re-aim is the **v5-only** arm the trait already has: a `Db` with no mount-index partition (`ws.mount_index()` → `None` → `DbError::Internal("embedding enqueue requires the mount-index database")`, `:56-63`). Label it v5-only in the doc comment: v4's `requireMountIndexDb` would throw inside `safeQuery` and fall back. The field-name pin survives on that arm.

### A6 Further v4 lines the embedding scheduler drops (v5 owns the file)

`embedding_scheduler.rs` prints with `eprintln!` where v4 logs:

- `:159` (v4's `Failed to clear embeddings for embed:false link` warn, `:47-53`, is UNREACHABLE: `clearEmbeddingsByLinkId` is a fallback; v4's line there is `Error clearing embeddings by link ID`);
- `:176-180` (v4 WARN `No default embedding profile configured, skipping mount chunk embedding` `{mountPointId, unembeddedCount}`, `:80-85`);
- `:183` (v4 WARN `No user found, skipping mount chunk embedding` `{mountPointId}`, `:92-94`);
- `:193-196` (v4 WARN `Failed to enqueue embedding job for mount chunk` `{chunkId, error}`, `:116-119`).

v5 also has no `Enqueuing embedding jobs for mount chunks` / `Embedding job enqueueing complete` INFO pair (`:97-102`, `:123-127`). This is a Tier 2 rider in an owned file.

---

## §B (e2) The character overlay's batch reads and the `send_mail` divergence

### B1 v4

- **Recipient resolve:** `lib/tools/handlers/send-mail-handler.ts:60-63` calls `resolveCharacterByNameOrId(context.userId, parsed.character)`, and `null` gives `fail('No soul by that name keeps a postbox here.')`. The resolver (`lib/services/character-resolver.ts:44-61`) calls `repos.characters.findByUserId(userId)`. That is `applyDocumentStoreOverlay(await super.findByUserId(userId))` (`characters.repository.ts:124-127`), where the slim read is `findByFilter`, a fallback.
- **The overlay** (`lib/database/repositories/vault-overlay/read-overlay.ts`): `applyDocumentStoreOverlay` `:322-365` → `loadVaultFileMaps(mountPointIds)` `:72-122`. That makes nine `repos.docMountDocuments.findManyByMountPointsAndPath(ids, path)` calls (one per `SINGLE_FILE_OVERLAY_PATHS` entry, `schema.ts:122-132`, under `Promise.all` `:82-86`) plus two `findManyByMountPointsInFolder(ids, 'Prompts'|'Scenarios', '.md')` calls (`:95-106`).
- **Both batch reads are fallback `withRawDb([])`s** (`doc-mount-documents.repository.ts:142-168`, `:179-220`). Their lines, at ERROR:
  - `Error finding documents by mount point IDs and path` `{collection: 'doc_mount_documents', mountPointIdCount, relativePath, error}` (`:165-166`);
  - `Error finding documents by mount point IDs and folder` `{collection, mountPointIdCount, folder, extension, recursive, error}` (`:217-218`). `recursive` is a boolean, and `false` on the overlay path.
- With every map empty, `hydrateOne` throws `CharacterVaultUnavailableError(id, mountId, 'properties.json missing')` (`:158-162`). The batch caller catches it per character and logs ERROR `Dropping character from list — vault unavailable` `{characterId, characterDocumentMountPointId, detail}` (`:345-354`), then WARN `applyDocumentStoreOverlay dropped characters with unavailable vaults` `{dropped, total}` (`:358-363`).
- Every vaulted character is dropped, the resolver finds nothing, and the tool answers `No soul…` before any write.
- **The single overlay** (`applyDocumentStoreOverlayOne`, `:373-388`) propagates `CharacterVaultUnavailableError`. The route middleware maps it to **503** `{error: 'Character vault unavailable', characterId}` (`lib/api/middleware/context.ts:196-205`) unless a handler catches first (§C).
- ⚠ The v4 doc comment `read-overlay.ts:66-70` ("Does NOT swallow read failures — a store-read exception propagates") is **false against v4's own code**: the two repository methods it calls are fallbacks. v5 ported the comment and wrote the code to match the comment (B2).

v4's other callers of the two batch reads, all fallbacks:

- `lib/database/document-store-overlay.ts:136` (projects/groups);
- `vault-overlay/vault-readers.ts:257,293,339`;
- `vault-overlay/vault-projection.ts:39`;
- `lib/mount-index/scenarios-common.ts:199,318`;
- `lib/services/aurora-notifications/core-whisper.ts:203,302`.

### B2 v5

- **Throw path:** `tools/send_mail.rs:141` → `db/character_resolver.rs:59` `characters_read::find_by_user_id(main, mount, user_id)?` → `characters_read.rs:323-330` `overlay_many` (`:252-255`) → `vault_read_overlay.rs:364-415` `apply_document_store_overlay` → `load_vault_file_maps` `:117-149`.
- That function makes **`?`** calls on `repo.find_many_by_mount_points_and_path(..)` (`:124`) and on `find_many_by_mount_points_in_folder(..)` (`:133`, `:140`). Under the `relativePath` rename it errors with `no such column: l.relativePath`, the `?` propagates through the resolver, and `send_mail`'s catch answers `The Post Office stumbled…`.
- The repository fns propagate: `db/doc_mount_documents.rs:371-403` (`find_many_by_mount_points_and_path`) and `:424-…` (`_in_folder` / `_in_folder_opts`).
- `vault_read_overlay.rs:113-116` repeats v4's false comment ("Read failures propagate (no swallow — there is no DB fallback post-cutover)").
- The v5 call sites (14 in production; 11 outside the repo's own delegation):
  - `vault_read_overlay.rs:124,133,140` (the overlay) and `:482,541` (the wardrobe readers);
  - `document_store_overlay.rs:279` (the projects/groups `load_store_files`);
  - `vault_wardrobe_write.rs:56`;
  - `scenarios.rs:295,390`;
  - `services/core_whisper.rs:175,283`.
- **These batch reads are NOT in the 130-site census:** its `METHODS` are `find_by_mount_point_and_path`, `find_by_mount_point_id`, `find_content_and_mtime_by_mount_point_and_path` and `delete_with_gc` only (`doc_mount_fallback_sites_census.rs:68-73`).
- The single overlay: `characters_read::find_by_id` (`:263-289`) maps `OverlayOneError::Db` → `Err` (a 500 at the route), while `Unavailable` → `DbError::StoreUnavailable` (v4's 503). Once the batch reads fall back, a read failure arrives as `Unavailable`, which matches v4.
- The projects/groups list overlay drops **silently**: `document_store_overlay.rs:425-432` (`Err(e) if e.is_unavailable() => continue`). v4 logs ERROR `` `Dropping ${label} from list — document store unavailable` `` `{[idLogKey]: row.id, officialMountPointId: row.officialMountPointId ?? null, reason}` and WARN `` `apply${Label}StoreOverlay dropped ${label}s with unavailable stores` `` `{dropped, of}` (`document-store-overlay.ts:210-229`). This is the same divergence P4.D172 closed for the character drop (`vault_read_overlay.rs:394-413`). Under the rename plant both appear wherever a project or group is listed (§C).

### B3 The pin that exists, and what retires

`crates/quilltap-harness/tests/mail_carina_tools_equivalence.rs`:

- `assert_send_divergence` `:1294-1317` asserts four things:
  - v4's text `== "No soul by that name keeps a postbox here."`;
  - `assert_ne!(v5_text, v4_text, "VANISHED: …")`;
  - v5's text starts `The Post Office stumbled and the letter went unsent — `;
  - exactly one `send_mail handler threw unexpectedly` catch line.
- It is called only for `plant == "links" && tool == "send"` (`:1259-1261`), and `send_divergence_seen` must be set (`:1270-1273`).
- `PLANT_EXCLUDED_MESSAGES` `:956-985` names the four overlay lines as excluded *because* v5's overlay propagates: the two batch-read errors, `Dropping character from list — vault unavailable`, and the summary WARN.

**After conversion:** the `assert_ne!` fires **VANISHED**, which is the red-first proof. Then:

1. Delete `assert_send_divergence` and `send_divergence_seen`, and let the send row fall through the generic `text == want.text` + no-catch compare (`:1262-1266`).
2. Move the two batch-read messages from `PLANT_EXCLUDED_MESSAGES` into the compared set. They are `quilltap::db` repository lines with field compare on `collection`, `mountPointIdCount`, `relativePath`, `folder`, `extension`, `recursive`.
3. Compare the two overlay-module lines (`Dropping character…` ERROR, the summary WARN) by message, level and fields under v5's module target. They are not repository-layer lines; the comparator's `ERROR quilltap::db {m} ` prefix filter (`:1233-1248`) does not fit them and needs a second, target-aware leg.
4. `Data validation failed` stays excluded. Its reason text should be reworded: it is the `characters` collection's `findByFilter`/`validateSafe` on the **`characters`** plant, not the overlay.

The oracle (`harness/oracle/cases/mail-tools.test.ts`) already records v4's lines with their fields in the `-plants.ndjson` file, so no oracle-case edit is needed. A pin-fresh regen is still required per protocol.

### B4 The other overlay differential

`vault_read_overlay_equivalence.rs` drives v4's REAL `applyDocumentStoreOverlay` over an oracle-built fixture (header `:1-27`; builder `harness/oracle/fixtures/build-vault-read-overlay-fixture.ts`, per-run, **not committed**). A rename-plant case there is the most direct red-first for the overlay:

- v4: every vaulted character dropped, with 9 + 2 batch-read lines + N drop lines + 1 WARN;
- v5 today: `Err`;
- the single overlay: v4 `CharacterVaultUnavailableError` vs v5 `OverlayOneError::Db`.

---

## §C (e3) `chatGet` / `listChats` under `doc_mount_file_links.relativePath` renamed

### C1 Where v5 throws

`no such column: l.relativePath` is the overlay batch read, `doc_mount_documents.rs:388-390` (`AND LOWER(l.relativePath) = LOWER(?…)`):

- **`listChats`:** `api/salon.rs:100-147` → `services/chat_enrichment.rs:796-…` `enrich_chats_for_list` → `characters_read::find_by_ids(main, mount, &character_ids)?` (overlay_many → B2) → `Err` → `internal(e)` (`salon.rs:134-139`, `:69-71`) → **500** with the raw message. The same function's `ProjectsRepository::find_by_ids` (the projects overlay, `document_store_overlay.rs:279`) would throw next.
- **`chatGet`:** `salon.rs:164-224` → `assemble_chat_get` → `chat_enrichment::enrich_participant_detail` (`:497-544`) → `get_character_detail` (`:355-363`) → `characters_read::find_by_id` → `apply_document_store_overlay_one` → `load_vault_file_maps` `Err` → `OverlayOneError::Db` → `?` → `internal(e)` → **500** with the raw message.
- Neither function logs a line.

No v5 function "joins `doc_mount_file_links` into the chat read" for its own sake. The join is the **character (and project) vault overlay** hydrating each participant.

### C2 v4: list falls back, get does not

- **`listChats`** (`app/api/v1/chats/route.ts:1048-1084` `handleList`) → `enrichChatsForList` (`lib/services/chat-enrichment.service.ts:631-…`):
  - `repos.characters.findByIds(...)` (`characters.repository.ts:154-157` = `applyDocumentStoreOverlay(await super.findByIds(ids))`, base `findByIds` = `findByFilter`, `base.repository.ts:563-569`) runs the overlay through **fallback** batch reads. Every vaulted character is DROPPED (B1's lines).
  - `repos.projects.findByIds` likewise **drops** each project (`document-store-overlay.ts:204-229`).
  - `findByIdsWithContent` is skipped because the avatar-id set is now empty. When reached, it is a fallback over `queryJoined` whose line is `Error querying joined file links {whereClause: 'WHERE l.id IN (?,…)'}`; its outer `Error finding file links by ids` (`doc-mount-file-links.repository.ts:567-579`) is unreachable.
  - **v4 answers 200** `{chats}` with those participants unresolved. **This is a real status divergence** (v5 500).
  - The handler's own catch, ERROR `[Chats v1] Error listing chats` `{}` + the Error as the third argument, then `serverError('Failed to fetch chats')` (`:1081-1084`), is not reached on this plant.
- **`chatGet`** (`app/api/v1/chats/[id]/handlers/get.ts:243-419` `handleGetChat`):
  - `repos.chats.findById` (fallback; `null` → `notFound('Chat')`);
  - then the cold-chunk re-warm, the terminal reconcile, `surfaceOperatorMailForChat` (whose mail listing logs the queryJoined line);
  - then `Promise.all(participants.map(enrichParticipantDetail))` (`:281-283`) → `getCharacterDetail` → `repos.characters.findById` = `applyDocumentStoreOverlayOne(await this._findById(id))` (`characters.repository.ts:71-74`). Its batch reads fall back, and `hydrateOne` throws `CharacterVaultUnavailableError`.
  - **The handler's own `try` catches it first** (`:416-419`): ERROR `[Chats v1] Error fetching chat` `{chatId}` + the Error, then **`serverError('Failed to fetch chat')`**, i.e. **500** `{error: 'Failed to fetch chat'}`. The middleware's 503 arm (`context.ts:196-205`) is never reached.
  - **So v4 also answers 500. The STATUS is not a divergence**, which is exactly the outcome the standing note asked to measure for. What differs:
    1. the body: v4 `Failed to fetch chat` against v5's raw `sqlite error: no such column: l.relativePath`, or, once the overlay converts, `applyDocumentStoreOverlayOne: vault unavailable for character …`;
    2. v4's 9 + 2 batch-read lines per vaulted participant (participants enrich concurrently under `Promise.all`; the order across participants is v4's resolution order) and the `[Chats v1] Error fetching chat` line, neither of which v5 logs.
- **Precondition:** this holds for a chat with at least one vaulted CHARACTER participant (every post-4.6 character has a vault). Friday's chats qualify.

### C3 What the port is

1. The B-section conversion closes `listChats` (200, drops).
2. `chat_get`'s and `list_chats`' failure arms become v4's handler catches: ERROR `[Chats v1] Error fetching chat` `chatId=…` `error=…` with body `Failed to fetch chat`, and ERROR `[Chats v1] Error listing chats` `error=…` with body `Failed to fetch chats`. v5's convention for v4's third-argument Error is `error = %e`, hoisted by the file layer (`files/llm_image_budget.rs:72-81`).
3. Port the projects/groups drop lines (B2) so the list compare sees them.

Tier 2 adds `chat_get`'s first read as v4's fallback: `chats_read::find_by_id` (`salon.rs:172-177`, a propagating read) where v4 is `findById` (fallback) → `notFound('Chat')`. Use the existing `chats_read::find_by_id_or_none` (`chats_read.rs:395-396` per P4.131's survey).

### C4 The differential

`crates/quilltap-harness/tests/salon_reads_equivalence.rs` + `harness/oracle/cases/salon-reads.test.ts` already drive v4's **real** `handleList` and `handleGet` route modules (`salon-reads.test.ts:271-287`) over a fresh copy of the committed `salon` fixture pair. The fixture has three chats, characters Aria/Elm/Vex with vaults (Vex's unavailable), and a project.

Add a case key `renameMountColumn: {table: 'doc_mount_file_links', from: 'relativePath', to: 'relativePath_x'}`, applied as a raw `ALTER TABLE` on the **mount-index copy** before the import of the route. This is the shape the mail plants use (`mail_carina_tools_equivalence.rs:1131-1150`); the case currently has `rawQuery` for main only, so it needs a mount-index raw exec beside it. Two cases, `list_all_mount_plant` and `get_solo_mount_plant`, and both sides mirror the plant.

- Today: list RED on status (200 vs 500); get green on status, RED on body.
- The lines need a Logger spy in the oracle (recipe: `harness/oracle/cases/chats-messages-ops-tier2.ts:120-155`), compared through an explicit shared/excluded table as in the mail family, `error=` tails skipped. v4's `error` on a rename plant is the `ensureTable` failure for links-repo reads (P4.131 finding 3); for the documents-repo batch reads it is the query's own message.
- `get_solo` exercises Aria. A `get_third` (Vex) case does not exist today. It would pin the pre-existing `Failed to fetch chat` arm without any plant, and is a cheap Tier 2 addition.
- **No committed fixture is rebuilt:** the plant is per-run.
- Mutation proof: revert the batch-read twins → `list_all_mount_plant` red at status; revert the get catch body → `get_solo_mount_plant` red at body.

---

## §D (f) The 130-site census and its conversion

### D1 What the census is

`crates/quilltap-harness/tests/doc_mount_fallback_sites_census.rs` (611 lines) scans the production code of four crates for calls to the four `METHODS` (`:68-73`) and their `_or_*` twins. It records `(file, enclosing fn, method, class)` in source order, with `EXPECTED` at `:192-338`. The classifier is at `:208-249`:

- a twin → `converted`;
- a call inside the three repos → `internal`;
- a receiver text containing `blobs` → `other-repo`;
- a call under `quilltap_import/` → strict;
- a call followed by `?` → `fallback-in-v4`;
- a swallowing adapter → `swallowed-by-other-means`.

`COUNTS = (23, 7, 18, 0, 13, 1, 68)` (`:404`), arithmetic at `:370-403`.

**Three limits that matter for this round:**

1. **`fallback-in-v4` is assigned from v5's `?` alone.** It is true at the repository level (all five v4 methods behind the four v5 names fall back), but the census never checks a per-site v4 twin or the v4 downstream arm.
2. **One row is misclassified.** `doc_mount_blobs.rs` `create_with_ids` (`:374`) calls `self.find_by_mount_point_and_path(...)` on `DocMountBlobsRepository` itself. The receiver text is `self`, not `blobs`, so it lands in `fallback-in-v4`. It is `other-repo`, which makes the true counts **67 `fallback-in-v4` / 19 `other-repo`**.
3. **The census covers four method names, not "the document-store reads".** It does not see:
   - the overlay batch reads (B, 11 sites);
   - the chunk reads (A, 8 sites);
   - `find_by_file_id` (`doc_mount_file_links.rs:1917`; v4 `:537-544` fallback over queryJoined, `WHERE l.fileId = ?`);
   - `find_by_ids_with_content` (`:2001`; v4 `:567-579`, dynamic `WHERE l.id IN (…)`);
   - `refresh_stats`' raw SQL;
   - the sync walker's raw SQL (§E).

**A home-shape consequence.** `joined_file_links_or_empty(where_clause: &'static str, …)` (`fallback.rs:124-138`) cannot carry v4's dynamic `WHERE l.id IN (?,?,…)`. Converting `find_by_ids_with_content` needs the parameter widened to `&str`. `tracing` needs static field NAMES, not static values.

### D2 The 67 `fallback-in-v4` rows, grouped by v4 line

Five v4 lines cover all of them:

- links `find_by_mount_point_id` / `find_by_mount_point_and_path` → **`Error querying joined file links`**, with `whereClause` `WHERE l.mountPointId = ?` or `WHERE l.mountPointId = ? AND LOWER(l.relativePath) = LOWER(?)`; the existing twins `find_by_mount_point_id_or_empty` / `find_by_mount_point_and_path_or_none`;
- documents `find_by_mount_point_and_path` / `find_content_and_mtime_…` → **`Error finding document by mount point and path`**;
- folders `find_by_mount_point_id` → **`Error finding entities by filter`** (collection `doc_mount_folders`);
- links `delete_with_gc` → **`Error deleting file link with GC`** → `false`.

| group | rows (file:fn × n) | repo → line | covering family (a plant arm can be posed?) |
|---|---|---|---|
| **G1 read-only routes/listings** | `api/characters.rs` `character_stats` ×1 (links id); `api/projects.rs` `project_file_list` ×1 (links id); `services/mount_index/list.rs` `mount_files_list` ×1 (folders); `read_file.rs` `read_mount_file_bytes_conn` ×2 + `read_mount_file` ×1 (links path, documents path); `quilltap-web/src/files_routes.rs` `mount_file_get` ×2 + `mount_blob_get` ×2 (links + documents path); `quilltap-web/src/qtap_target_route.rs` `qtap_target_get` ×2 (links + documents path); `photos/character_gallery_service.rs` `list_character_gallery` ×1; `photos/user_gallery_service.rs` `list_user_gallery` ×1 — **14** | as noted per row | `characters_reads_equivalence`, `projects_routes_equivalence`, `mount_read_equivalence` (list + read_file), `files_routes_equivalence`, `character_photo_upload_tier2` / `almanack_tier2` (galleries); `qtap_target_route` has NO family (→ capture-pin unit) |
| **G2 dedup / lookup-before-write** | `photos/save_image_to_album.rs` `find_existing_photos_link_by_sha` ×1; `tools/photo.rs` `find_existing_photos_link_by_sha` ×1; `services/file_storage.rs` `store_mount_blob` ×1; `services/image_job_storage.rs` `write_main_avatar_to_vault` ×1 (links path) + ×1 `delete_with_gc`; `services/mount_index/store_file.rs` `store_mount_file` ×4 (links path ×3, documents mtime ×1); `scanner.rs` `process_mount_file` ×1; `reindex_file.rs` `reindex_inner` ×2 (documents mtime, links path); `link_groups.rs` `reindex_inner` ×1; `general_state.rs` `ensure_general_state_file` ×1 (documents); `db/character_vault.rs` `ensure_character_metadata_file` ×1 (documents); `db/vault_character_update.rs` `read_current_properties` ×1 (documents); `db/document_store_overlay.rs` `read_properties` ×1 (documents); `api/scenarios.rs` `create_op`/`update_op`/`rename_op` ×4 (documents); `documents/mod.rs` `classify_resolved_target` ×1 (links); `services/knowledge_injector.rs` ×1 (documents); `services/librarian_notifications.rs` ×1 (links); `api/chat_media.rs` `chat_attach_mount_file` ×1 (links) — **25** | as noted | `photo_tools`, `chat_gallery`, `files_routes`, `character_avatar_write_tier2`, `mount_write`, `mount_link_groups`, `state_*`, `characters_*_tier2`, `scenarios_routes`, `documents_routes`, `knowledge_injector`, `post_office_librarian`, `attach_mount_file`, all neutrality-only today |
| **G3 deletes / prunes** | `photos/avatar_rolls_service.rs` `delete_avatar_roll` ×1; `character_gallery_service.rs` `remove_from_character_gallery` ×1; `user_gallery_service.rs` `remove_from_user_gallery` ×1; `services/file_storage.rs` `delete_mount_blob_conn` ×1; `services/character_archive/service.rs` `prune_vault` ×3 + `prune_empty_folders` ×1 (folders); `file_ops.rs` ×8 (`source_exists_or_throw`, `dest_exists`, `compute_dest_sha256`, `delete_at_source` ×2, `delete_at_dest` ×2, `move_file`); `folder_ops.rs` `move_folder` ×1; `scanner.rs` `remove_mount_file` ×2 + `rescan_database_mount_point` ×1; `reindex.rs` ×2 — **22** | as noted | `avatar_rolls_tier2`, `character_archive_tier2`, `doc_fm`, `files_routes`, `tool_execution_tier2`, `doc_mount_write_metadata` |
| **G4 RULING (§E)** | `sync/apply_store.rs` ×5 | links path ×4, GC ×1 | `sync_engine_equivalence` |
| **G5 RULING-adjacent (export)** | `services/qtap_export/records.rs` `stream_one_store` ×1 (folders) | folders | `system_export`. ⚠ v4 `lib/export/ndjson-writer.ts:625,642` falls back, so a failed read **exports the store EMPTY, silently**. The standing backup/restore/import/export ruling (memory `backup-restore-fix-dont-match`, 2026-08-03: in this family "v5 FIXES v4 bugs rather than reproducing them") puts this row OUT of the conversion: keep it strict and record the divergence both ways |

Totals: 14 + 25 + 22 + 5 + 1 = **67**, which matches the corrected census count (D1 item 2). The groups are a work plan; the lane recounts from `QT_CENSUS_PRINT`.

**Per-site v4 read is still owed for G2/G3.** A fallback there turns an error into "proceed as if absent". Examples:

- `store_mount_file`: a failed existing-link read → create, then the write throws anyway;
- `delete_at_source`: GC failure → `false` → v4 goes on;
- `prune_vault`: a failed links read → nothing pruned.

The v5 downstream arm must mirror v4's for each site. That is a reading task per site, and some sites may turn out `swallowed`/`strict` on reading.

### D3 Proofs available

No jest oracle plants a failure on these paths (P4.131's measurement, still true). The reusable rigs are:

- (a) the mail family's per-run RENAME plant, recorded from v4's real stack;
- (b) `salon_reads` with the new plant key (§C4);
- (c) `vault_read_overlay` with a plant case (§B4).

For G1, a RENAME-plant case in each listed family is feasible (the family already drives v4's real route). For G2/G3, a capture-pin unit per site on a planted rename in the v5 crate is the realistic proof: v4's bytes come from the twin's home line, and its silence leg comes from the existing neutrality run. **Mutation proof:** flip one group's site back to `?` → its plant arm or unit goes red; flip a census class → `every_document_store_read_site_is_classified` goes red (P4.131's M4 shape).

---

## §E The sync applier's fallback RULING (for the human)

**No ruling exists.** `phase-4.md:7492` lists it as left out; no "RULED" text for the applier appears in `status-log.md` or `phase-4.md`. P4.131 ESCALATED it (order Tier 3, `:294-297`).

**v4 (measured, not run):**

- `lib/mount-index/sync/apply-store.ts`: `assertUnchanged` `:58-67`, `writeStoreFile`'s post-read `:133-134`, `readStoreBytes` `:143`, `applyStoreAction`'s describe `:192-193`, delete `:207` (`deleteWithGC`), rmdir `:212-214` (`deleteDatabaseFolder`). All go through the fallback repository reads; none runs under `withStrictRepositoryFailures`, which only the importer enters (`lib/import/quilltap-import/execute.ts:430`, `preview.ts:31`).
- **More important than the applier is the walker.** `lib/mount-index/sync/walk-store.ts:41-98` reads `docMountFolders.findByMountPointId` (`:47`) and `docMountFileLinks.findByMountPointId` (`:60`), both fallbacks. On a store read failure the walk returns an EMPTY store.
- For every base entry unchanged on disk, the planner then emits `removal('disk', d, 'deleted in the store since last sync')` (`planner.ts:206-214`), because `propagateDeletes` defaults `true` (`types.ts:37`). Deletions run after creations (`planner.ts:295-307`).
- There is no degraded-index guard in `sync/index.ts` (only the character keystone refusal, `planner.ts:62`).
- **In v4, a broken links or folders table (or a degraded mount index: `getCollection` throws inside the fallback) makes `quilltap sync` delete the disk side.** This is a candidate v4 bug filing.

**v5 today:**

- `services/mount_index/sync/walk_store.rs:60-140` is hand-SQL and **propagates**. It is not in the census.
- `apply_store.rs:125,261,273,347,381` propagate (census rows).
- `delete_database_folder` already took v4's fallback via the store layer (P4.131; reached by the applier's rmdir).

**Option A (v4 fidelity).** The applier's five reads, and for consistency the walker's two, take the twins.
- Effects on the applier: `assertUnchanged` on a failed read → `found = None` → a race error when a sha was expected (safe), or proceeds when the planner saw nothing.
- The post-write read returns `''` → the manifest records an empty sha → the next run sees drift.
- `readStoreBytes` → `None` → `The store has no content at …` (`sync/mod.rs:493-502`, v4 `index.ts:271-274`).
- **The walker conversion reproduces the disk-deletion hazard.**

**Option B (strict sync, recorded divergence; recommended).** The whole sync path stays strict: the walker, the applier's five reads, and a strict sibling for the applier's `rmdir` (`delete_database_folder` with a propagating contents read). A store read failure fails the run before planning.
- Record it as a deliberate divergence of bug 79's kind (v4's own strict-scope precedent) and of the 2026-08-03 backup/export ruling's kind, pinned both ways in `sync_engine_equivalence` with a rename-plant case: v4 plans deletes, v5 refuses.
- File the walker hazard upstream.

---

## What the recorded description got wrong

1. **"the fix lives in the overlay … on the census's `fallback-in-v4` list"** (P4.131 lane record, finding 2; census comment `:395-403`). The throw site is the overlay's **batch** reads (`vault_read_overlay.rs:124,133,140`), and **no census row covers them**: `METHODS` excludes `find_many_*`. The three overlay-adjacent census rows (`document_store_overlay::read_properties`, `vault_character_update::read_current_properties`, `character_vault::ensure_character_metadata_file`) are not on the `send_mail` path.
2. **The census's 68 is 67.** `doc_mount_blobs.rs:374` is the blobs repository calling its own method (`other-repo`; 18 → 19).
3. **The census is not "the document-store reads".** It is four method names. The chunk reads, the overlay batch reads, `find_by_file_id`, `find_by_ids_with_content`, `refresh_stats`' SQL and the sync walker's SQL all sit outside it.
4. **`fallback-in-v4` is never checked per site against v4.** It is read off v5's `?`. G5 shows a row (`qtap_export`) where the standing ruling says v5 should NOT take v4's fallback.
5. **The dogfood note's "`chatGet` and `listChats` answer 500 … a v4 fallback would make it a divergence".** It is half right. `listChats` IS a divergence (v4 200 with drops). `chatGet` is NOT a status divergence (v4 500 `Failed to fetch chat` from `handleGetChat`'s own catch, before the middleware's 503); only the body and the lines differ. Neither failure is "a direct `doc_mount_*` repository read outside the fallback homes" in the chat code; both are the character vault overlay.
6. **P4.131's "`DROP TABLE` is not a failure in v4 … every read answers empty".** This holds only on a repository instance's FIRST access. `tableEnsured` (`dedicated-db.repository.ts:134-152`) and `collectionInitialized` (`base.repository.ts:113-121`) make the ensure once-only per instance, so a DROP after first access in a long-lived v4 process IS a failure. The jest oracles see fresh instances (`jest.resetModules()` per case). The move to RENAME plants stays right (a rename fails either way); the stated reason is narrower than recorded.
7. **v4's own `read-overlay.ts:66-70` comment** ("Does NOT swallow read failures") is false against its code, and v5 ported the comment and the propagation together (`vault_read_overlay.rs:113-116`).
8. **The embedding scheduler's `.catch` warn** (pinned by `mount_embedding_effects.rs:283`) is unreachable in v4 on any repository failure once the reads fall back (A5). The P4.131 record implies a v4-faithful re-poison exists; it does not, and the arm must be labelled v5-only.
9. Re-verified and held: `assert_send_divergence`'s shape and its VANISHED leg; the census arithmetic (bar item 2); `joined_file_links_or_empty`'s outer-line-unreachable rule; `error_text`'s bare message.

## Proposed tiered deliverables

**Tier 1 — must land**

1. **The home grows** (`db/fallback.rs`, target `quilltap::db`, v4's bytes, capture + silence unit pins each, `HOME_MESSAGES` +5 in `fallback_home_guard.rs`):
   - `documents_by_mount_point_ids_and_path_or_empty` → `Error finding documents by mount point IDs and path {collection: doc_mount_documents, mountPointIdCount, relativePath, error}`;
   - `documents_by_mount_point_ids_in_folder_or_empty` → `Error finding documents by mount point IDs and folder {collection, mountPointIdCount, folder, extension, recursive, error}`;
   - `count_embedded_chunks_or_empty`;
   - `search_chunk_content_or_empty`;
   - `search_file_links_by_name_or_path_or_empty`.

   The last two FOLD the module-target copies at `doc_mount_chunks.rs:352-363` and `doc_mount_file_links.rs:1367-1374`, red-first on a byte pin (target, camelCase, bare `error`).
2. **The overlay batch reads fall back.** Add twins beside `doc_mount_documents.rs:371,424/437` and repoint all 11 production callers (B2). Proofs:
   - **(a)** `vault_read_overlay_equivalence` gets a RENAME-plant case, RED first (v5 `Err` vs v4's drops + 9 + 2 + N + 1 lines);
   - **(b)** `mail_carina_tools_equivalence`'s `assert_send_divergence` fires **VANISHED** (red-first), and is then retired as B3 lays out;
   - **(c)** the single overlay on the plant answers `Unavailable` (v4's `CharacterVaultUnavailableError`), pinned in the same family.
3. **The projects/groups overlay drop lines** (`document_store_overlay.rs:425-432`) get v4's ERROR + WARN (B2 bytes; label via the `StoreEntity` trait). Pin them in `groups_tier2_equivalence` / `projects_routes_equivalence`, or a capture unit.
4. **`chatGet` / `listChats`** (`api/salon.rs`): v4's two handler catches (C3). The new `salon_reads_equivalence` plant cases `list_all_mount_plant` (red on status first) and `get_solo_mount_plant` (red on body first), with a Logger spy in `salon-reads.test.ts`.
5. **The chunk reads** (A): twins for `find_rows_by_mount_point_id`, `find_row_by_id`, `find_ids_by_link_id`, and the two counts, through the existing `find_by_filter_or_empty` / `find_by_id_or_none` and the new count fn. Repoint `embedding_scheduler.rs:166`, `reindex.rs:298`, `embedding_reindex_job.rs:561`, `embedding_generate_job.rs:919` (`None` → v4's WARN + markAsFailed arm, already ported), `character_archive/service.rs:1375`, `api/mount_points.rs:116,148`. Re-aim `mount_embedding_effects.rs:283` at the v5-only no-mount-index arm (A5), red-first as predicted.
6. **The census** (`doc_mount_fallback_sites_census.rs`):
   - fix the blobs self-call (`other-repo`);
   - widen `METHODS` to the two batch reads and the chunk reads (or a sibling census over them), so the new twins count as `converted`;
   - recount `COUNTS` with the arithmetic in the comment.
7. **The G1 conversion** (14 read-only route/listing sites, D2), each with a RENAME-plant arm in its covering family where the family drives v4's real route, and a capture-pin unit otherwise (`qtap_target_route`).

**Tier 2 — should land**

8. G2 (25) and G3 (22): per-site conversion, each after reading v4's downstream arm. Proof is a capture-pin unit on a planted rename plus a neutrality run of the covering family. A site whose v4 downstream differs is recorded as such.
9. `joined_file_links_or_empty` widened to `&str`, plus twins for `find_by_file_id` and `find_by_ids_with_content` (`listChats`' avatar read). Note the chunked `find_by_ids_with_content` logs once per chunk where v4 logs once; this matters only above 999 ids.
10. `chat_get`'s first read → `chats_read::find_by_id_or_none` (v4's `notFound('Chat')`). Add a `get_third` (Vex) case pinning the existing `Failed to fetch chat` arm.
11. The embedding scheduler's v4 lines (A6), plus `clear_embeddings_by_link_id_or_zero`.
12. `characters_read::find_names_by_ids` (`:427-439`). It logs v4's UNREACHABLE outer line, `Error resolving character names`, under `quilltap::memory` with the `sqlite error:` prefix. v4's inner `super.findByIds` → `findByFilter` falls back first (`characters.repository.ts:179-191`), so v4 logs `Error finding entities by filter {collection: characters}`. Fold it onto the home.

**Tier 3 — loud deferrals / rulings**

13. **The sync RULING** (§E): Option A vs Option B. B is recommended, with a both-ways pin and an upstream filing for the walker.
14. **`qtap_export` stays strict** under the 2026-08-03 ruling (G5), with a both-ways pin when the export family next regenerates.
15. `refresh_stats`: v4 writes zero counts on a failed read. It is a cached-stats write, and porting it is a fidelity-vs-harm question; recorded, not converted.
16. The other inline `quilltap::db` fallback lines outside the home (`conversation_chunks.rs:276-280`, `help_doc_chunks.rs:371-374`, `memories_read.rs:923-925`, `embedding_status.rs:424-428`) are not verified this survey. A guard widening ("every `quilltap::db` `Error …` literal lives in `fallback.rs`") is the follow-up.
17. v4's rethrow-mode chunk lines (`Error updating doc mount chunk embedding` etc.) are a different class (log + rethrow), unported.

## Files the lane would edit

Source (`crates/quilltap-core/src/`):
- `db/fallback.rs` (THIS lane owns it; **P4.139 must NOT add a home here**: it uses the existing `find_api_key_by_id_or_none` / `find_api_key_by_id_and_user_id_or_none`)
- `db/doc_mount_chunks.rs`, `db/doc_mount_documents.rs`, `db/doc_mount_file_links.rs` (twins + the search fold), `db/doc_mount_folders.rs` (only if a twin is missing for a G-site)
- `db/vault_read_overlay.rs` (`load_vault_file_maps` + wardrobe readers + the false comment), `db/document_store_overlay.rs` (`load_store_files` + drop lines + `read_properties` G2), `db/vault_wardrobe_write.rs`, `db/scenarios.rs`, `services/core_whisper.rs`
- `db/characters_read.rs` (Tier 2 item 12 only), `db/character_vault.rs`, `db/vault_character_update.rs`, `db/doc_mount_blobs.rs` (no change expected; census reclass only), `db/doc_mount_points.rs` (Tier 3, no edit)
- `api/salon.rs` (`chat_get` / `list_chats` catches), `api/mount_points.rs`, `api/characters.rs`, `api/projects.rs`, `api/scenarios.rs`, `api/chat_media.rs` (one site)
- `services/mount_index/{embedding_scheduler,reindex,list,read_file,store_file,scanner,reindex_file,link_groups,general_state,file_ops,folder_ops}.rs`
- `services/{embedding_reindex_job,embedding_generate_job,knowledge_injector,librarian_notifications,file_storage,image_job_storage}.rs`, `services/character_archive/service.rs`
- `photos/{mount_embedding_effects,save_image_to_album,character_gallery_service,user_gallery_service,avatar_rolls_service}.rs`, `tools/photo.rs`, `documents/mod.rs`
- `crates/quilltap-web/src/{files_routes,qtap_target_route}.rs`

Harness (`crates/quilltap-harness/tests/`):
- EDIT: `fallback_home_guard.rs`, `doc_mount_fallback_sites_census.rs`, `mail_carina_tools_equivalence.rs` (retire `assert_send_divergence`, move the overlay messages), `salon_reads_equivalence.rs` (two plant cases), `vault_read_overlay_equivalence.rs` (plant case)
- EDIT (G1 plant arms): `mount_points_routes_equivalence.rs`, `characters_reads_equivalence.rs`, `projects_routes_equivalence.rs`, `mount_read_equivalence.rs`, `files_routes_equivalence.rs`
- REGENERATE (neutrality): `mail_carina_tools`, `salon_reads`, `vault_read_overlay`, `groups_tier2`, `projects_routes`, `scenarios_routes`, `scenario_resolvers`, `core_whisper`, `vault_wardrobe_*` (read/write/public), `wardrobe_tier2`, `embedding_generate_jobs`, `embedding_remainder`, `character_archive_tier2`, `mount_points_routes`, `doc_mount_points_tier2`, `knowledge_injector`, `photo_tools`, `chat_gallery`, `post_office_librarian`, plus every G2/G3 covering family named in D2 that the lane converts; `sync_engine` + `system_export` as neutrality only

Oracle (`harness/oracle/`):
- `cases/salon-reads.test.ts` (plant key + Logger spy), `cases/vault-read-overlay.ts` (plant case), `cases/mail-tools.test.ts` (no change expected; regen only)
- per-family G1 cases as their plant arms land

Committed fixtures: **none rebuilt** (every plant is per-run, on a copy).

## Files the lane must READ but not edit

- `services/mount_index/sync/{apply_store,walk_store,mod}.rs` (RULING, §E)
- `services/qtap_export/records.rs` (G5, ruling-held)
- `services/quilltap_import/**` (strict by design)
- `db/database_store.rs` (P4.131's converted layer)
- `db/chats_read.rs` (existing `find_by_id_or_none`)
- `services/chat_enrichment.rs` (the overlay's callers; no edit needed)
- `tools/send_mail.rs`, `db/character_resolver.rs` (the throw path; behaviour moves without an edit)
- v4: `lib/database/repositories/{doc-mount-chunks,doc-mount-documents,doc-mount-file-links,base,dedicated-db,characters}.repository.ts`, `safe-query.ts`, `vault-overlay/read-overlay.ts`, `lib/database/document-store-overlay.ts`, `app/api/v1/chats/route.ts`, `app/api/v1/chats/[id]/handlers/get.ts`, `lib/api/middleware/context.ts`, `lib/mount-index/sync/{walk-store,apply-store,planner,index,types}.ts`, `lib/mount-index/embedding-scheduler.ts`

## Cross-lane adjacencies / risks

- **P4.139 (API keys)** must use the existing two key homes and NOT add a `fallback.rs` home. If it finds it needs one, it STOPs and hands the shape to this lane (a §S handoff). Both lanes touch `fallback_home_guard.rs`' `HOME_MESSAGES` only if P4.139 adds a home, which it must not.
- **P4.140 (Salon spine, Option V)** owns `services/{orchestrator,carina_query,build_context}.rs`.
  - No census site lives in those three files. `services/knowledge_injector.rs` (G2) is called from `build_context` but is a separate file (this lane's).
  - `api/salon.rs` `chat_get` already takes `zone: &TimeZone`. If Option V reshapes that parameter, the two lanes meet on `chat_get`'s signature. **Record a §S rule:** P4.142 edits only the error arms of `chat_get`/`list_chats` (`salon.rs:172-177`, `:216-223`, `:108-111`, `:134-139`) and never the signature.
  - P4.140's Carina `CHAT_MESSAGE` row does not intersect.
- **P4.143 (data/zod smalls)** owns import/restore, `chats_messages_read.rs`, and `route_trail.rs`.
  - `chat_get` calls `transcript_projection`, which may read through `chats_messages_read.rs`. No edit is needed there.
  - `qtap_export/records.rs` is export, not import/restore. If P4.143's scope is read to include export, G5 is its call, and this lane leaves it untouched either way.
- **P4.144** owns `harness/tools/recipe_sweep.py`'s `ALIAS_ASSIGN`. This lane only RUNS the sweep, but any new recipe header it writes (`salon_reads` plant env) must parse under P4.144's widened rule.
- **P4.141 / P4.145:** no intersection.
- **Runtime risk:** after the overlay converts, every character LIST read on a broken mount index drops vaulted characters instead of erroring. That is v4's behaviour and it reaches the hot turn path (bug 131 / P4.D172). Re-run `orchestrator_tier3` and `build_context_tier3` as neutrality (P4.140's families: run, don't edit).

## Versions / bumps

- **core** (every source edit)
- **harness** (every test edit)
- **web** (`files_routes.rs` / `qtap_target_route.rs`, G1)

Nobody else's crate moves. **host**, **cli**, **tauri**, **SPA** and **sqlite3mc-sys** are untouched (NEVER bump sqlite3mc-sys). Base at this survey: core 0.0.1146, harness 0.0.1074, web 0.0.207.
