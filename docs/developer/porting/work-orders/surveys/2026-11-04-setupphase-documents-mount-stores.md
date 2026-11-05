# Survey — documents, Document Mode, mount-point stores, file scanning (the `624d0d25c` catch-up)

## 0. Header

- **Date:** 2026-11-04. **Baseline:** v4 `236c04e35`. **Pin:** v4 `624d0d25c`
  (`4.13.0-dev.8`). Every v4 read below is `git show 624d0d25c:<path>` /
  `git diff 236c04e35 624d0d25c -- <path>` / `git show <sha>`; the v4 working
  tree (dirty with unrelated optimizer edits) was never read. **v5 main:**
  `ff1651177` (core 0.0.1427, host 0.0.223, web 0.0.256, cli 0.0.38, harness
  0.0.1110, SPA 0.5.971). Read-only: nothing built, no test run.
- **Commits in scope** (sha — date — subject — proposed lane):
  - `bf7a457f6` — 10-29 — iCloud placeholders in the scanner + `?action=scan` — **SCAN** (merged into **STORES**, §7)
  - `db4a76e39` — 10-29 — drop chokidar — **NO-PORT** (About line → SPA)
  - `aec626eff` — 10-31 — the mount-point document-store API — **STORES**
  - `c34986421` — 10-31 — Scriptorium folder browser (server half: folder rows) — **STORES** / **DOCSPA**
  - `5efe53ea7` — 10-31 — Document Mode refuses non-text; picker — **DOCCORE** / **DOCSPA**
  - `c17033f04` — 10-31 — General library, blob text editing, accessible-stores — **DOCCORE** / **DOCSPA**
  - `4310d435f` — 11-02 — resolved-file helpers (one behaviour change) — **DOCCORE**
  - `3d39c303c` — 11-02 — response helpers — **NO-PORT, VERIFIED**
  - `05264ace3` — 11-02 — one walk under the scanners; watchers — **VERIFY-ONLY** + NO-COUNTERPART
  - `a3d53eae1` — 11-02 — the document core (#133 / #134 / #137) — **DOCCORE**
  - `30a3b4cb7` — 11-03 — moves follow every open copy; edits over uploads drop stale text — **DOCCORE**
- **Issues:** `gh issue view` for #133, #134 and #137: all CLOSED, all
  "Provenance: Original to v4", "v5 status: Not assessed". **Measured here:
  v5 has all three** (§1.1). None is a convergence of a v5 filing.
- **Ledger rows read:** `drift-ledger.md:475` (`bf7a457f6`), `:476`
  (`db4a76e39`), `:496` (`aec626eff`), `:497` (`c34986421`), `:498`
  (`5efe53ea7`), `:500` (`c17033f04`), `:505` (`4310d435f`), `:507`
  (`3d39c303c`), `:517` (`05264ace3`), `:520` (`a3d53eae1`), `:523`
  (`30a3b4cb7`). Corrections are marked **LEDGER CORRECTION** below.
- **Out of my area, but on my files:** `0c47f4539` (`reach.ts`, the tier walk)
  edits the chat route's `accessible-stores` and `doc-edit/path-resolver.ts`.
  `600c2906f` (the browser family) edits `database-store.ts`'s normaliser,
  `path-utils.ts` (`isSafeRelativePath`), the `folders` route and the realtime
  `wardrobe` topic. §7.4 lists the collisions.

### Headline facts a planner must not miss

1. **v5 has all three of v4's document-core bugs, and a fourth gap behind
   them.**
   - **#133:** `documents/mod.rs:445` (write), `:545` and `:573` (rename) gate
     the refresh on `scope == DocEditScope::DocumentStore`, which is v4's
     baseline shape. v4 now gates on `mountPointId`
     (`lib/documents/core/write.ts:38-61`).
   - **#134:** v5's Document Mode has no blob fallback at all
     (`documents/mod.rs:755-776` reads `read_database_document` only). Its
     edit tools read through `shared::read_file_with_mtime`, and only
     `doc_read_file` falls back (`tools/doc_edit/text.rs:263-297`), reading
     `extractedText` rather than the blob's bytes.
   - **#137:** `tools/doc_edit/document_ui.rs:250-305` records the requested
     scope and the typed `mount_point` for a new blank document. It also
     creates the document with **`write_fs_file_with_mtime_check(&resolved.absolute_path, "")`
     (`:282-283`) even when the target is a DATABASE store** — mirroring v4's
     baseline raw `fs.writeFile`. A project with an official database store
     never gets the document row. v4's `ensureDocument`
     (`core/write.ts:103-115`) fixes both sides.
   - **The pinned R-13 asymmetry** ("a typed `self` on a NEW blank document
     is still stored", `document_ui.rs:301-304`) is RETIRED by #137's
     `storeName = publishedMountPoint(resolved, undefined)`.
2. **LEDGER CORRECTION (`30a3b4cb7`):** v5's `doc_move_file` /
   `doc_move_folder` never sweep `chat_documents` at all.
   - `file_management.rs:28-30`, `:175`, `:238`, `:922` and `:999` are
     "no-op seams": the corpus seeds no open rows. This is a gap older than
     this commit, not only its literal-match shape.
   - Only Document Mode's two rename routes sweep, through
     `ChatDocumentsRepository::rename_file_path_in_store`
     (`db/chat_documents.rs:508`, callers `api/documents.rs:910` and `:1677`).
   - The port replaces both repository methods (`:508`, `:550`) with
     `find_by_file_path` / `find_under_folder`, plus ONE
     `sweep_chat_documents_after_move` / `..._after_folder_move`
     (v4 `core/move.ts:132-290`) that all four sites call.
3. **The doc-edit "reindex deferral" is now the same code path as Document
   Mode's refresh.**
   - v4 deleted `triggerReindexIfNeeded` (`doc-edit/shared.ts`). Every tool
     write now goes through `writeDocument` → `afterDocumentWrite`:
     reindex → `reindexLinkGroupSiblings` → embedding enqueue +
     `refreshStats`.
   - v5's `services/mount_index/refresh.rs::refresh_now` (`:20-47`) lacks the
     link-group sibling step and is reached only from Document Mode.
   - **Ruling R-2** (§10) decides whether the tools join it. Either way the
     oracle seam must move (fact 5).
4. **`doc_*` tool behaviour moves under the core** (v4 hunks,
   `file-management-handlers.ts` / `text-handlers.ts` /
   `markdown-handlers.ts` / `document-ui-handlers.ts` at the pin). These are
   the differential reds to expect:
   - **Reads:** `doc_str_replace`, `doc_insert_text`, `doc_update_*`,
     `doc_read_frontmatter`, `doc_read_heading` and `doc_copy_file` now read
     an uploaded native-text blob as its own bytes (`readDocument`'s
     `blob-bytes`). Before, they answered "not found".
   - **`doc_read_file` text branch:** reads the blob's BYTES with
     `mtime = link.lastModified`, where it used to read `extractedText` with
     `mtime = blob.updatedAt`.
   - **Unified refusal texts:** the filesystem branches' error texts now
     match the database texts:
     - `doc_move_file`: `Source path is not a file: X` → `Path is a folder,
       not a file: X. Use doc_move_folder to rename or move folders.`
       (v5 `:212`).
     - `doc_copy_file`: → `Source path is a folder, not a file: X.`
       (v5 `:462`).
     - `doc_delete_file`: → `Path is a folder, not a file: X. Use
       doc_delete_folder to remove folders.` (v5 `:643`).
   - **Database move:** a FOLDER at the destination now counts as taken
     (`statDocument`, `core/move.ts:47-50`).
   - **`doc_open_document`** on an existing database document:
     `mtime = Date.now()` becomes the document's real mtime (v5
     `document_ui.rs:207-213` mints a now). A viewer-only file gets
     `Date.parse(viewer.updatedAt)`.
   - **Info lines:** `Moved file` / `Moved database document` →
     `Moved document` (+ `mountType`); `Deleted file` /
     `Deleted database document` → `Deleted document` (+ `mountType`).
5. **Harness: one IMPORT-DEAD, eight GARBLED seams, one STALE PREMISE**
   (§5.1).
   - **IMPORT-DEAD:** `cases/documents-rename-target.ts:16` imports
     `computeRenameTarget` from `operator-doc-actions`, which no longer
     exports it (it lives in `@/lib/documents/core`). It fails at the first
     call.
   - **GARBLED:** eight cases `doMock` the deleted
     `triggerReindexIfNeeded` export: `doc-text`, `doc-fm`, `doc-fs`,
     `doc-ui`, `doc-enum`, `doc-blob`, `doc-opacity` and
     `project-roster-access`. The mock now overrides nothing, and v4's live
     `afterDocumentWrite` races every dump.
   - **STALE PREMISE:** `cases/mount-read.ts:106-120` rebuilds the files-list
     route body by hand from `docMountFiles.findByMountPointId`. v4's route
     now reads `docMountFileLinks.listForBrowser`, so this case stays GREEN
     while v4 has moved. Re-aim it at the real route.
6. **`aec626eff` changes EXISTING wires, not just new ones.**
   - `GET …/files` rows become `DocMountBrowserEntry`: no `extractedText`,
     plus `allowEmbed` / `allowCharacterRead` / `allowCharacterWrite`
     (coerced booleans), `linkGroupId` and `linkCount`, with
     `description ?? ''` and `chunkCount ?? 0`, in a new key order. v4's
     `doc-mount-file-links.repository.ts:701-760` vs v5's
     `services/mount_index/list.rs` over
     `DocMountFilesRepository::find_links_with_content_json_by_mount_point_id`
     (`db/doc_mount_files.rs:164`).
   - `delete-folder` answers `deletedFiles: 0`, and a within-store
     `move-folder` answers `destMountPointId`.
   - The `availableActions` envelope grows from 13 to 15 (`unlink-file`
     after `link-file`, `copy-folder` after `move-folder`) —
     `quilltap-web/src/files_routes.rs:895-910`.
   - `GET …/files/{*path}?action=<unknown>` is now a 400, where it used to
     read the file (`dispatchAction`, `actions.ts:163-183`).
   - Filesystem folder delete and move now keep `doc_mount_folders` rows in
     step (`folder-ops.ts:81-87`, `:144-172`; v5 `folder_ops.rs:45-171` does
     not).
7. **`?action=render-html` needs `mammoth`; v5 has no DOCX converter at
   all.** The extractor seam refuses DOCX/PDF (prior survey R-2). See ruling
   R-1. The pure `neutraliseUnsafeHrefs` (`converters/docx-html.ts:24-29`) is
   portable on its own.
8. **v5 publishes `mountPoints` realtime hints only from job topics**
   (`realtime/job_topics.rs:83-147`). v4 now publishes per store from the
   parent chokepoints (`lib/mount-index/realtime.ts`): db-store events, the
   five file ops, folder ops, the folders route and unlink. On the SPA side,
   `core/realtime-topic-map.ts:132` ("NO v5 TARGET") gains
   `mountPoints.store(id)` + `lists`. Ruling R-5.
9. **LEDGER CORRECTION (`bf7a457f6`):** v5's scan has none of v4's INFO
   lines.
   - `scanner.rs:291-412` logs nothing on success, and the route
     `api/mount_files.rs:95-140` only `eprintln!`s on error.
   - So "the route INFO's `filesInCloud` key" has no v5 line to extend. The
     lane ports v4's three lines whole: `Starting mount point scan`,
     `Mount point scan completed` (+`filesInCloud`), and
     `Left cloud-evicted files unread; …`. It also ports the route's
     `[Mount Points v1] Scan completed`.
   - v5's `process_mount_file` reads + hashes BEFORE it stats
     (`scanner.rs:166-181`). On an evicted iCloud file that read DOWNLOADS
     the file. v4 now stats first and returns `in-cloud`
     (`scanner.ts:169-187`).
10. **LEDGER CORRECTION (`05264ace3`):** the files-side placeholder skip is
    in the files/ WATCHER (`lib/file-storage/watcher.ts`, `isCloudEvicted`),
    not the files/ scanner. Both scanners and `walkDisk` were rewritten onto
    `walkTree` with identical order, pruning and read-error lines (diffed
    hunk by hunk). For v5 this commit is **VERIFY-ONLY**: the mount-read /
    sync / files-scan families stay green.
11. **`3d39c303c` VERIFIED NO-PORT:** `created(d)` = `NextResponse.json(d,
    {status: 201})` and `successResponse(d)` = `NextResponse.json(d,
    {status: 200})` (`lib/api/responses.ts:63-68`, `:207-209` at the pin) —
    byte-identical.
12. **`4310d435f` hits v5:** `documents/mod.rs:627-631` reads EVERY
    `metadata` error as `NotFound`, and `:558-561`'s destination probe
    (`try_exists().unwrap_or(false)`) reads EACCES as "free". v4's
    `statResolvedFile` (`doc-edit/path-resolver.ts:~795-807`) treats only
    `ENOENT` / `ENOTDIR` as absent and throws the rest. The fs branches of
    `file_management.rs` (`fs_stat_is_file` / `fs_path_exists`,
    `:1035-1047`) have the same class.

---

## 1. Per commit, the shipped hunks

### 1.1 `a3d53eae1` + `30a3b4cb7` + `4310d435f` — the document core (DOCCORE)

**NEW at the pin:** `lib/documents/core/{errors,index,read,write,move,delete,view-target}.ts`.

- `errors.ts:14-20` — `DocumentConflictError`, `DocumentMissingError`, `DocumentReadOnlyError`.
- **`read.ts:41-64` `readDocument(resolved)`:**
  - `readFileWithMtime` first; on `ENOENT` only, and only for
    `mountType === 'database' && mountPointId`, it reads
    `readDatabaseBlobForDocument`.
  - DEBUG `readDocument: no document at path; reading the uploaded blob`
    `{mountPointId, relativePath, source, size}`.
  - Returns `{content, mtime, size, source: 'document'|'blob-bytes'|'blob-text', storedMimeType?}`.
- **`read.ts:73-85` `statDocument`:** a database store answers
  `databaseDocumentExists` → `'file'`, `databaseFolderExists` → `'folder'`,
  else `null`. On disk it uses `statResolvedFile`: `isFile` → `'file'`,
  anything else → `'folder'`.
- **`read.ts:92-97` `documentExists`:** a database store is `=== 'file'`; on
  disk it is `!== null`.
- **`write.ts:38-61` `afterDocumentWrite`** (#133):
  - Gated on `mountPointId`.
  - DEBUG `afterDocumentWrite: scheduling store refresh`.
  - Chain: `reindexSingleFile` → `reindexLinkGroupSiblings` →
    `Promise.all([enqueueEmbeddingJobsForMountPoint, refreshStats])`.
  - A catch logs WARN `Background re-index, embedding, or stats refresh
    failed after a document write` `{mountPointId, path, error}`.
- **`write.ts:69-95` `writeDocument`:** an mtime conflict (message contains
  `mtime mismatch` / `modified by another process`) → DEBUG
  `writeDocument: mtime conflict` and throws `DocumentConflictError(msg)`.
  Success → DEBUG `writeDocument: wrote document` `{relativePath,
  mountPointId, bytes: content.length}`, then `afterDocumentWrite`.
- **`write.ts:103-115` `ensureDocument`:** `{created:false}` when the document
  exists, else `writeDocument(resolved, '')` + DEBUG
  `ensureDocument: created a blank document`.
- **`move.ts:38-74` `renameDocument`** → `{ok:true}` or
  `{ok:false, reason}`:
  - Source `null` → `source-missing`; source `folder` →
    `source-not-a-file`.
  - Database: the destination is taken when `statDocument` finds anything,
    except on a case-only rename. `moveDatabaseDocument` runs, and a
    `DatabaseStoreError` CONFLICT → `destination-taken`.
  - Disk: the same test, then `moveResolvedFile` + `afterDocumentWrite(to)`.
- **`move.ts:132-174` `sameStoreMatcher`:**
  - `general` rows, or a caller with no store id → literal match.
  - `document_store` rows: literal, `mountPoint === store.mountPointId`, or
    `docMountPoints.findByName(name)` with exactly one match.
  - `project` rows: chat → project → `officialMountPointId`.
- **`move.ts:189-241` `sweepChatDocumentsAfterMove`:** walks
  `chatDocuments.findByFilePath(old)`. The logs:
  - INFO `Swept chat_documents after a document move` `{...logFields,
    scope, mountPoint, mountPointId, from, to, rowsUpdated}`;
  - DEBUG `No chat_documents rows to sweep after a document move`;
  - WARN `Failed to sweep chat_documents after a document move`.
- **`move.ts:248-290`:** the folder twin, over `findUnderFolder`, with the
  prefix rewritten and the title = new basename. INFO `Swept chat_documents
  after a folder move` / WARN `Failed to sweep chat_documents after a folder
  move`.
- **`delete.ts:17-37` `deleteDocument`** → `deleted | not-found | not-a-file`:
  - Database: `deleteDatabaseDocument`, else `statDocument` folder →
    `not-a-file`.
  - DEBUG `deleteDocument: deleted database document` / `deleteDocument:
    deleted file`.
- **`view-target.ts:21-58` `viewTargetFor`** (from `5efe53ea7`, moved here):
  - `null` without `mountPointId`.
  - Editable = database `detectNativeText` / disk `isTextFile`.
  - Otherwise the link's `{mountPointId, relativePath, fileName, fileType,
    fileSizeBytes, originalMimeType ?? null, updatedAt}`, or for an unindexed
    disk file the stat's shape with `fileType: 'blob'`.

**Repositories:**

- **`chat-documents.repository.ts`:** `renameFilePathInStore` /
  `renameFolderPathInStore` are DELETED. `findByFilePath(filePath)` and
  `findUnderFolder(folderPath)` (prefix `startsWith`, over `findAll()`)
  replace them. A `safeQuery` failure → `[]`, with lines `Error finding chat
  documents by file path` / `…under a folder`.
- **`doc-mount-file-links.repository.ts:1295-1310`** (`linkDocumentContent`):
  the UPDATE now also sets `extractedText = NULL, extractedTextSha256 = NULL,
  extractionStatus = 'none', extractionError = NULL`. On v5 this is
  `db/doc_mount_file_links.rs:886-895`.
- **`database-store.ts:119-148` NEW `readDatabaseBlobAsDocument`:**
  - native text only (`detectNativeText`);
  - `findByMountPointAndPath` → `readData`, read as UTF-8;
  - `mtime = new Date(blob.lastModified)`, `size = data.length`;
  - DEBUG `readDatabaseBlobAsDocument: reading an uploaded text blob as a
    document`.
- **`database-store.ts:150-171` NEW `readDatabaseBlobForDocument`:** bytes
  first, else `readDatabaseBlobText` → `source: 'blob-text'`.

**`path-resolver.ts`** (`4310d435f`): NEW `statResolvedFile`,
`moveResolvedFile` and `deleteResolvedFile`, with DEBUG `Moving resolved
file` / `Deleting resolved file`.

**Operator layer:**

- **`operator-doc-actions.ts`:**
  - `openDocumentFile` (`:196-244`): `viewTargetFor` → `{content:'',
    viewer}`, else `readDocument`.
  - A blank document goes through `ensureDocument`; `!created` →
    `DocumentConflictError('A file already exists at that name.')`.
  - `writeDocumentFile` (`:252-273`) refuses a viewer target:
    `DocumentReadOnlyError('<path> is shown read-only and cannot be written
    from the editor')`.
  - `resolveGeneralLibrary` (`:319-328`), `storeOptionFor` (`:334-349`).
  - `listAllEnabledStores` gains `exclude` (`:357-388`).
- **`operator-doc-http.ts`:**
  - `resolveDocumentForRoute`: WARN `Failed to resolve <subject> path for
    <verb>` → 400 `Could not resolve <filePath>: <msg>`.
  - `openDocumentResponse`: a missing document → 404; anything else → ERROR
    `Failed to open <subject>` + 500 `Failed to open document: <msg>`.
  - `openedDocumentPayload` adds `viewer` when there is one.
  - `readDocumentResponse` → `{content:'', viewer}` for a viewer target.
  - `writeDocumentResponse`'s read-only arm → WARN `Refused an editor write
    to a read-only <subject>` + 400.
  - `renameDocumentResponse` returns:
    - 404 `File not found: <old>` for a missing source;
    - 400 `Path is not a file: <old>` for a folder;
    - 409 for a taken destination.
  - `recentDocumentsResponse(docs, currentChatId)`.
  - `sweepRenamedDocumentTracking` is DELETED. Its WARN `Failed to sweep
    recent-document tracking after rename` is gone.

**Routes:**

- Chat route `app/api/v1/chats/[id]/actions/documents.ts`:
  - The rename/delete resolve-failure 400 text moves from `Could not resolve
    path: <msg>` (v5 `api/documents.rs:867`, `:1023`) to `Could not resolve
    <filePath>: <msg>`. The WARN text stays `Failed to resolve document path
    for rename|delete` (subject `document`), but its fields change: it now
    carries `filePath` / `scope` / `mountPoint` and drops `from` / `to`.
  - The open 500 text moves from `Failed to create blank document: <msg>`
    (v5 `:407`) to `Failed to open document: <msg>`.
  - `accessible-stores` gains `generalLibrary`. General is added to `seen`,
    so it leaves `stores`. The debug log gets `hasGeneralLibrary`. The
    project / vault / group reach is `0c47f4539`'s `resolveCastPool` — see
    §7.4.
- **Standalone `app/api/v1/documents/route.ts`:** `accessible-stores` →
  `{stores (General excluded), projectLibrary: null, generalLibrary}`. Rename
  and delete resolve first (the 400 above).

**Tools:**

- **`document-ui-handlers.ts`** (#137): the existing-file path uses
  `viewTargetFor` → `Date.parse(updatedAt)`, else `readDocument().mtime`.
  The blank path uses `ensureDocument`; `!created` → `Failed to create blank
  document: <path> already exists`. Then `scope = targetScope` and
  `storeName = publishedMountPoint(resolved, undefined)`, with DEBUG
  `doc_open_document: created a blank document`.
- **`text-handlers.ts`:** the non-text branch reads
  `readDocument().catch(ENOENT→null)` and uses `blob-text` only.
  `storedMimeType ?? 'application/octet-stream'` goes into both the header
  and `mimeType`.
- **`doc_move_folder`:** records the resolved store name (bug 256 — already
  ported) and passes `mountPointId` to the folder sweep.

### 1.2 `5efe53ea7` / `c17033f04` server halves (DOCCORE)

Only the pieces not already folded into §1.1:

- **`app/api/v1/projects/[id]/mount-points/route.ts:52-61`:** DEBUG
  `[Projects v1] Listed mount points for project` `{projectId, count,
  officialMountPointId}`. The body becomes
  `{mountPoints, officialMountPointId: project.officialMountPointId ?? null}`.
  v5 `api/projects.rs:1362` answers `{mountPoints}` only.
- **`lib/folder-browser/document-actions.ts`** (`DocumentViewTarget` type)
  is the SPA/server shared shape for `viewer`.
- **The mount watcher's `schedulePublish`** (`c17033f04`,
  `watcher.ts` +28): NO-COUNTERPART (no watcher in v5).

### 1.3 `aec626eff` + `c34986421` server half — the store API (STORES)

- **`files/route.ts:33`:** `listForBrowser`, plus DEBUG `[Mount Points v1]
  Listed files for browser` `{mountPointId, files, folders}` (`:56`).
- **`doc-mount-file-links.repository.ts:701-760` `listForBrowser`:**
  - SELECT order: `l.id, fileId, mountPointId, relativePath, fileName,
    folderId, originalFileName, originalMimeType, description,
    descriptionUpdatedAt, conversionStatus, conversionError,
    plainTextLength, extractedTextSha256, extractionStatus,
    extractionError, chunkCount, allowEmbed, allowCharacterRead,
    allowCharacterWrite, linkGroupId, lastModified, createdAt, updatedAt,
    f.sha256, fileSizeBytes, fileType, source, linkCount` — with **no
    ORDER BY**.
  - `linkCount = Number(row.linkCount) || 1`; `coerceAllow` (`:240-244`);
    v5 has `coerce_allow` at `db/doc_mount_file_links.rs:2346`.
  - DEBUG `Listed file links for browser`.
- **`:765-800` `clearLinkGroup(linkId)`** → `{groupId, dissolved}`, in one
  transaction. A group left with ≤1 member is dissolved. DEBUG `Cleared
  hard-link group from link`.
- **`files/[...path]/route.ts`:**
  - GET is `dispatchAction(req, {links, 'render-html'}, read)` (`:178`).
  - **`?action=links`** (`:125-160`): a missing link → 404 `File`.
    - The location set is `findByFileId` ∪ `findByLinkGroupId`, keyed by id.
    - `mountPointName` is the store's name, defaulting to `'Unknown store'`.
    - `grouped` and `self` flags.
    - Sorted self-first, then by `localeCompare` of the store name, then of
      the path (ICU collation in v5).
    - DEBUG `[Mount Points v1] Listed file locations`.
    - Answers `{linkGroupId ?? null, locations}`.
  - **`?action=render-html`** (`:162-176`):
    - `readMountFileBytes`; non-DOCX (neither `fileType === 'docx'` nor a
      `.docx` suffix) → 400 `Only .docx files can be rendered to HTML`;
    - else `renderDocxToHtml` → `{html}`, with DEBUG `[Mount Points v1]
      Rendered DOCX for viewing`;
    - errors go through `handleFileOpError(…, 'render file')`.
- **`[id]/route.ts`:**
  - `fileOpFailure` (`:570-577`): ERROR `[Mount Points v1] Error trying to
    <label>` → 500 `Failed to <label>`.
  - `handleCopyFolder` (`:738-760`): a schema failure → 400 `Copy folder
    requires fromPath and toPath`; INFO `[Mount Points v1] Copied folder`.
  - `handleUnlinkFile` (`:766-792`):
    - schema failure → 400 `Unlink requires a non-empty path`;
    - no link → 404;
    - publishes to self, and on dissolve to the other stores' siblings;
    - INFO `[Mount Points v1] Unlinked file from its hard-link group`;
    - answers `{mountPointId, path, groupId, dissolved}`.
  - `deleteFolderSchema` gains `recursive`; `moveFolderSchema` gains
    `destMountPointId` (`:552-563`).
- **`folder-ops.ts`:**
  - `deleteEmptyFolder` (`:57-95`) now also deletes the filesystem folder's
    `doc_mount_folders` row. DEBUG `Removed filesystem folder row with the
    folder` (`c34986421`).
  - `moveFolderWithinMount` (`:97-180`) carries fs folder rows: the source
    row is re-parented via `ensureFolderPath`, and descendants are
    prefix-rewritten. DEBUG `Moved filesystem folder rows with the folder`.
  - Recursive `deleteFolder` (`:264-305`):
    - `readFolderTree` → `Folder not found: <rel>` SOURCE_NOT_FOUND;
    - on a filesystem store, unindexed strays → CONFLICT `Folder holds N
      file(s) the store does not index (a, b, c, …); remove them on disk
      first.`;
    - per-file `deleteFile`, then `removeEmptyFolders` deepest first;
    - INFO `Deleted folder recursively`.
  - `planFolderTransfer` (`:315-333`): `A folder cannot be put inside itself:
    <to>` INVALID_PATH; `Destination already exists: <to>` DEST_EXISTS.
  - `copyFolder` (`:351-377`):
    - an undo on failure, with WARN `Folder copy failed; undoing`;
    - INFO `Copied folder`;
    - answers `{mountPointId, fromPath, destMountPointId, toPath, files,
      folders}`.
  - `moveFolder` (`:379-…`):
    - within one store → `{...result, destMountPointId}`;
    - across stores → per-file `moveFile`, a move-back undo (WARN `Folder
      move failed; moving back`, ERROR `Could not move a file back after a
      failed folder move`), INFO `Moved folder between stores`.
- **`file-ops.ts:951-990`:** the five exported ops become publishing
  wrappers.
- **`realtime.ts`** (NEW): `publishMountChange(id)` =
  `publishRealtime('mountPoints', id)`.
- **`db-store-events.ts`:** every emit also publishes.
- **`topic-map.ts`:** `mountPoints` with an id →
  `[mountPoints.store(id), mountPoints.lists]`.
- **`converters/docx-html.ts`** (NEW, 43 lines): `SAFE_HREF =
  /^(https?:|mailto:|#)/i`. `neutraliseUnsafeHrefs` replaces `\shref="…"`
  whose `&amp;`-decoded, trimmed value fails the test with ` href="#"`.
  `renderDocxToHtml` = `mammoth.convertToHtml` + DEBUG `Rendered DOCX to
  HTML` (service `MountIndex:DocxHtml`).

### 1.4 `bf7a457f6` + `05264ace3` — the scanner (STORES, ex-SCAN)

`lib/mount-index/scanner.ts` at the pin:

- `ScanResult.filesInCloud` (`:41`), between `filesDeleted` and
  `chunksCreated`.
- `emptyScanResult` (`:47-58`).
- `processMountFile` (`:169-187`): `detectFileType` (unsupported) first, then
  `fs.stat`, then `isEvictedCloudPlaceholder(stat)` → `{status:'in-cloud'}`
  before any read.
- The scan counts it (`:377`) after adding it to `seen`, and logs the
  completion INFO with `filesInCloud` (`:419-428`) + the cloud INFO
  (`:430-436`).
- `lib/files/cloud-placeholder.ts`: `STAT_BLOCK_BYTES = 512`,
  `isNotFullyMaterialized`, and `isEvictedCloudPlaceholder(stat, platform)`
  = `(darwin|win32) && size > 0 && blocks === 0`.
- The route `[id]/route.ts:239-275`: INFO `[Mount Points v1] Scan completed`
  `{…, filesInCloud, …}`; the body is the whole `scanResult`.
- The `05264ace3` `walkTree` rewrite (`lib/files/walk.ts:52-87`): verified
  order-equal for `walkDirectory`, `listFilesystemFolders`, `walkDisk` and
  `scanDirectory`.
- **NO-PORT:** `scanAllMountPoints({only})`, the startup gate, both watchers,
  `watcher-shutdown.ts`, and chokidar (`db4a76e39`).

---

## 2. The v5 surface map (what v5 does TODAY)

| v4 unit at the pin | v5 home (`ff1651177`) | ported by |
|---|---|---|
| `operator-doc-actions` / `-http` | `crates/quilltap-core/src/documents/mod.rs` (1065 l.: open `:376`, write `:430`, rename `:512`, delete `:598`, list `:665`, read `:755`); `api/documents.rs` (1757 l.) | P4.6w; fs branches P4.6bg; standalone P4.9j4 |
| refresh chain | `services/mount_index/refresh.rs:20-47` (`refresh_now`), `:118` (`DbMountRefreshScheduler`, wired `quilltap-host/src/host.rs:1000`) | P4.6y unit J |
| `doc_*` handlers | `tools/doc_edit/{text,markdown,file_management,document_ui,shared}.rs` | W4.6 tool waves; P4.32 reindex un-mock |
| blob readers | `db/database_store.rs:247-302` (`read_database_blob_text`, `list_database_blob_texts`) | P4.D354 (bugs 260/261) |
| `chat_documents` | `db/chat_documents.rs:508,550` | P4.6w |
| `linkDocumentContent` | `db/doc_mount_file_links.rs:734-900` | P4.6v / P4.D209 |
| `path-resolver` | `doc_edit/path_resolver.rs` (1693 l.) | P4.6 doc-edit; P4.6bg |
| files list | `services/mount_index/list.rs` + `api/mount_files.rs:63` | P4.6v |
| folder ops | `services/mount_index/folder_ops.rs:45,84` + `api/mount_files.rs:323,341` | P4.6y |
| scan | `services/mount_index/scanner.rs:156,291` + `api/mount_files.rs:95` | P4.6y unit F |
| availableActions | `quilltap-web/src/files_routes.rs:895-910` | P4.6y / P4.D210 |
| realtime | `realtime/job_topics.rs:83-159`; `realtime/publish_sites.rs` (wiring pins) | P4.D124 |
| project mount-points | `api/projects.rs:1362` | P4.6l |

---

## 3. Schema / D23

No schema moves in this area. `linkGroupId` already exists (bug 186 / link
groups). No migration, no ensure.

---

## 4. Log lines (v4 text → v5 home)

| Lvl | v4 text (pin) | v5 today |
|---|---|---|
| DEBUG | `readDocument: no document at path; reading the uploaded blob` {mountPointId, relativePath, source, size} | replaces `text.rs:283-289` (`doc_read_file: no document at path; reading the uploaded blob's text`) |
| DEBUG | `readDatabaseBlobAsDocument: reading an uploaded text blob as a document` | NEW, `database_store.rs` |
| DEBUG | `afterDocumentWrite: scheduling store refresh` | NEW |
| WARN | `Background re-index, embedding, or stats refresh failed after a document write` {mountPointId, path, error} | `refresh.rs:37-46` today `eprintln!`s two other texts |
| DEBUG | `writeDocument: mtime conflict` / `writeDocument: wrote document` / `ensureDocument: created a blank document` | NEW |
| DEBUG | `renameDocument: moved database document` / `…moved file`; `deleteDocument: deleted database document` / `…deleted file` | NEW |
| INFO/DEBUG/WARN | `Swept chat_documents after a document move` / `No chat_documents rows to sweep after a document move` / `Failed to sweep chat_documents after a document move`; the folder twins | NEW (the old `Synced…` / `Failed to sweep recent-document tracking after rename` retire) |
| INFO | `Moved document` {from, to, scope, mountType}; `Deleted document` {path, scope, mountType} | tool INFO texts in `file_management.rs` |
| DEBUG | `doc_open_document: created a blank document` {chatId, filePath, requestedScope, scope, mountPoint} | NEW |
| DEBUG | `openDocumentFile: opening read-only` | NEW |
| WARN | `Refused an editor write to a read-only <subject>` | NEW |
| WARN | `Failed to resolve <subject> path for <read\|rename\|delete>` | `api/documents.rs:585,867,1023` |
| ERROR | `Failed to open <subject>` | NEW (chat + standalone) |
| DEBUG | `Moving resolved file` / `Deleting resolved file` | NEW, `path_resolver.rs` |
| DEBUG | `[Projects v1] Listed mount points for project` | NEW, `api/projects.rs` |
| DEBUG | `[Mount Points v1] Listed files for browser`; repo `Listed file links for browser` | NEW |
| DEBUG/INFO | `[Mount Points v1] Listed file locations`; `…Unlinked file from its hard-link group`; `…Copied folder`; `…Rendered DOCX for viewing` | NEW |
| ERROR | `[Mount Points v1] Error trying to <label>` | NEW (copy-folder only) |
| DEBUG/INFO/WARN/ERROR | folder-ops: `Removed filesystem folder row with the folder`, `Moved filesystem folder rows with the folder`, `Deleted folder recursively`, `Folder copy failed; undoing`, `Copied folder`, `Folder move failed; moving back`, `Could not move a file back after a failed folder move`, `Moved folder between stores` | NEW |
| INFO | `Starting mount point scan` / `Mount point scan completed` (+filesInCloud) / `Left cloud-evicted files unread; they are indexed once downloaded` | **ABSENT in v5** (fact 9) |
| INFO | `[Mount Points v1] Scan completed` (+filesInCloud) | **ABSENT** (`api/mount_files.rs:95-140`) |

---

## 5. Harness / oracle impact

### 5.1 IMPORT-DEAD / GARBLE / STALE at the pin

- **IMPORT-DEAD:** `harness/oracle/cases/documents-rename-target.ts:16`.
  Re-aim the import to `@/lib/documents/core`. The vectors are unchanged
  (`core/move.ts:86-106` is byte-equal to the old body).
- **GARBLED seam** (8 cases, each `jest.doMock('@/lib/tools/handlers/doc-edit/shared',
  …triggerReindexIfNeeded…)`): `doc-text.test.ts:186-195`, `doc-fm`,
  `doc-fs`, `doc-ui`, `doc-enum`, `doc-blob`, `doc-opacity:169`,
  `project-roster-access:173`.
  - `writeDocument` calls `afterDocumentWrite` from inside its own module, so
    no module mock can reach it.
  - The faithful seam is `jest.doMock('@/lib/doc-edit', () => ({...actual,
    reindexSingleFile: () => new Promise(() => {})}))`. `core/write.ts:9` is
    the ONLY barrel consumer of `reindexSingleFile`. `database-store`'s
    awaited chunk pass dynamic-imports `@/lib/doc-edit/reindex-file`
    (`post-write-reindex.ts:41`), so P4.32's real chunk-on-write stays real.
  - **Under R-2 (A)**, instead let the chain run and await it with a recorder
    before each dump (the "oracle async tails" pattern).
- **STALE PREMISE:** `cases/mount-read.ts:106-120` (`listCase`) — re-aim it
  to call `@/app/api/v1/mount-points/[id]/files/route` GET (as `mount-ops`
  loads routes, `mount-ops.test.ts:276-277`). `mount_read_equivalence.rs:343`,
  `:415` then move.

### 5.2 Families that move (v4 drift reds, not v5 bugs)

| family (rs ← oracle) | what moves |
|---|---|
| `documents_routes_equivalence` ← `documents-routes.test.ts` (`:450` accessible-stores, `:504` rename, `:518` delete) | `generalLibrary` key + General leaves `stores`; the 400 resolve text; the open 500 text; `viewer`; rename 404/400 arms |
| `doc_text_equivalence` / `doc_fm_equivalence` / `doc_fs_equivalence` / `doc_ui_equivalence` / `doc_enum_equivalence` / `doc_blob_equivalence` / `doc_opacity_equivalence` / `project_roster_access` | the seam (§5.1); blob-bytes reads + mtime; unified fs texts; `Moved document` / `Deleted document`; `doc_open_document` mtime + #137 row scope |
| `documents_rename_target_equivalence` | IMPORT-DEAD only (vectors green after the re-aim) |
| `chat_documents_tier2_equivalence` ← `chat-documents-tier2.ts` | `renameFilePathInStore` / `renameFolderPathInStore` gone → `findByFilePath` / `findUnderFolder` |
| `doc_mount_file_links_tier2_equivalence` | `linkDocumentContent` clears the four extraction fields; NEW `listForBrowser`, `clearLinkGroup` |
| `upload_blob_text_backfill_equivalence` | stays green — verify only; `readDatabaseBlobText` is unchanged |
| `mount_ops_equivalence` ← `mount-ops.test.ts:333-346,384` | `folder_delete_*` bodies gain `deletedFiles: 0`; `folder_move_*` gain `destMountPointId`; fs folder rows move/delete in the state dump |
| `mount_index_equivalence` ← `mount-index.test.ts:255-258` | `scan_*` bodies gain `filesInCloud: 0` |
| `mount_points_routes_equivalence` / `query_param_semantics` | the 15-entry `availableActions`; `GET …/files/{path}?action=x` → 400 |
| `mount_read_equivalence` | after the STALE re-aim: the browser-entry shape |
| `projects_routes_equivalence` ← `projects-routes.test.ts:453-454` | `officialMountPointId` on `mount_points_iota` / `_kappa` |
| `mount_link_groups_equivalence` | home for `?action=links` / `unlink-file` rows |
| `realtime/publish_sites.rs` (unit pins) | new publish sites, if R-5 (A) |

**Closest recipes for the NEW differentials:**

- `mount-ops.test.ts` / `mount_ops_equivalence`: copy-folder, recursive
  delete, cross-store move incl. the undo leg, and unlink.
- `mount-link-groups.ts`: links.
- `documents-routes.test.ts`: viewer / generalLibrary / read-only refusal.
- `chat-documents-tier2.ts` + `doc-text.test.ts`: the cross-store sweep, with
  open rows planted in two chats under `project` and the store name. The
  corpus has seeded none until now.
- `documents-rename-target.ts`: the pure tier-1 pair (`isEvictedCloudPlaceholder`
  / `isNotFullyMaterialized` with platform injected, and
  `neutraliseUnsafeHrefs`).

### 5.3 Committed fixtures

- No builder change is forced.
- The sweep corpus needs `chat_documents` rows. Plant them in-case: the
  committed `mounts-*` pair has none.
- A filesystem fixture of an evicted placeholder is not reproducible. Prove
  `in-cloud` through the pure family plus an injected `stat` seam on
  `process_mount_file` (fake `blocks = 0`).

---

## 6. SPA halves (for a DOCSPA lane)

v4 files at the pin:

- `app/salon/[id]/components/DocumentPickerModal.tsx` (rewritten),
  `DocumentViewerPane.tsx` (NEW), `DocumentPaneBinding.tsx`,
  `hooks/useDocumentMode.ts`, `documentModeApi.ts`;
- `components/document-browser/*` (NEW: `DocumentStoreBrowser`,
  `DocumentRow`, `DocumentViewer`, `DocumentFileDialogs`,
  `DocumentDestinationDialog`, `ProjectStoreBrowser`,
  `useDocumentStoreSource`);
- `components/files/FilePreview/FilePreviewDocx.tsx`;
- `lib/folder-browser/{document-actions,preview-kind}.ts`;
- `app/scriptorium/[id]/*` (`FileTable` deleted);
- `app/prospero/[id]/components/FilesCard.tsx`;
- `components/chat/LibraryFilePickerModal.tsx`;
- `components/ui/BaseModal.tsx` + `hooks/useClickOutside.ts` (layered
  dialogs);
- `lib/query/keys.ts`, `AboutView.tsx`.

v5 targets:

- `apps/web/src/app/documents/document-picker.ts:97,357,401` — the
  `projectLibrary` button → add `generalLibrary` + the folder-browser pick
  mode.
- `documents/document-api.ts:29-37,103` — `AccessibleStoresResult` +
  `generalLibrary`; `viewer` on open/read.
- `documents/standalone-wire.ts:71-82,121` — likewise.
- `documents/document-pane.ts` / `document-mode.ts` — the read-only viewer
  pane; refuse saves on a viewer.
- `core/core-contract.ts:7410,7439` — `OpenDocumentResultDto` +
  `ProjectLibraryTargetDto` (rename to a library target).
- `screens/scriptorium/{store-detail,file-table,file-detail-row}.ts` —
  compare shapes with v4's `DocumentStoreBrowser`. This is D18's bespoke file
  manager (ngx-explorer REJECTED, `phase-4.md:263`), so v4's SVAR removal
  has no v5 counterpart. Port the behaviours: links / unlink /
  copy-folder / cross-store move / recursive delete / the DOCX viewer under
  R-1.
- `screens/scriptorium/scriptorium.api.ts:47-71`:
  - the `files` row type gains `linkGroupId`, `linkCount` and `allow*`, and
    drops `extractedText`;
  - `ScanResult` gains `filesInCloud`;
  - ⚠ `errors` is typed `Array<{file, error}>`, but v4's `ScanResult.errors`
    is `string[]` — an existing v5 SPA type error to fix with it.
- `screens/prospero/cards/project-files-card.ts` — browse the project's
  `officialMountPointId` store.
- `core/realtime-topic-map.ts:132` (+ `.spec.ts:102-107`) — `mountPoints`
  gains targets.
- `screens/about/about-page.ts:427` (drop `File Manager: SVAR`), `:455`
  (drop `@svar-ui/react-filemanager`), `:458` (drop `chokidar`), plus the
  Scriptorium blurb.
- **Layered dialogs:** v4's `BaseModal` now ignores an Escape or outside
  click that belongs to a dialog layered over it. Measure whether this
  converges v4 onto the ruled `WARDROBE_VIEWER_ESCAPE_LAYERED` divergence
  (`drift-ledger.md:408`) for any surface; the wardrobe viewer itself is a
  different hook.

---

## 7. Lane split, by FILE OWNERSHIP

An honest split is **two server lanes and one SPA lane**. The scanner is
small, and its only shared file with the store API is `api/mount_files.rs`,
so it rides **STORES**. Split it out (as SCAN: `scanner.rs`,
`cloud_placeholder.rs`, `mount_scan` in `api/mount_files.rs:95-140`) only if
STORES is over budget.

### 7.1 DOCCORE — the document core, Document Mode, the doc tools

**Owns:**

- NEW `crates/quilltap-core/src/documents/core/{mod,read,write,moves,delete,view_target}.rs`
  (or sections of `documents/mod.rs`);
- `documents/mod.rs`;
- `api/documents.rs`, except `chat_reachable_stores` (`:1188-1300`, §7.4);
- `tools/doc_edit/{text,markdown,file_management,document_ui,shared}.rs`;
- `db/chat_documents.rs`;
- `db/database_store.rs` (the blob readers `:230-302` + NEW `read_database_blob_as_document` / `_for_document`);
- `services/mount_index/refresh.rs` (→ `after_document_write`, + link-group siblings);
- `doc_edit/path_resolver.rs` (NEW `stat_resolved_file` / `move_resolved_file` / `delete_resolved_file` only);
- `api/projects.rs:1362` (`officialMountPointId` + DEBUG);
- `db/doc_mount_file_links.rs` — ONLY the `link_document_content` UPDATE (`:886-895`).

**Oracle:** the 8 seam re-aims, `documents-rename-target.ts`,
`documents-routes.test.ts`, `chat-documents-tier2.ts`, the `doc-*` cases,
`projects-routes.test.ts` rows.

### 7.2 STORES — the mount-point store API + realtime + scanner

**Owns:**

- `api/mount_files.rs` (new handlers + `mount_scan`);
- `services/mount_index/{folder_ops,file_ops,list,scanner}.rs`;
- NEW `services/mount_index/{realtime,cloud_placeholder,docx_html}.rs`;
- `db/doc_mount_file_links.rs` — ONLY NEW `list_for_browser` / `clear_link_group` (appended);
- `quilltap-web/src/files_routes.rs` (`AVAILABLE` + the GET action dispatch on `files/{*path}`);
- `realtime/publish_sites.rs` (pins);
- `db/database_store.rs` emit sites (publish).

**New `Request` variants** go in `api/types.rs` (`MountFolderMove` sits at
`:1988`) and `api/engine.rs` (`:4232`). Both are shared with every lane, so
they are add-only.

**Oracle:** `mount-ops.test.ts`, `mount-index.test.ts`, `mount-read.ts`
(STALE re-aim), `mount-points-routes.test.ts`, `mount-link-groups.ts`,
`query-param-semantics.test.ts`.

### 7.3 DOCSPA — the §6 files

Owns every `apps/web/src/app/documents/*` and
`screens/scriptorium/*` file, `project-files-card.ts`, and
`core/realtime-topic-map.ts`'s `mountPoints` arm. It stacks on DOCCORE and
STORES for the wire.

### 7.4 Collisions and named contracts

- **`db/doc_mount_file_links.rs`** is DOCCORE's (one UPDATE) and STORES'
  (two appended methods). The regions are disjoint; state the contract in
  both orders.
- **`db/database_store.rs`** is DOCCORE's (blob readers) and STORES'
  (`publish_mount_change` at the emit sites). `600c2906f`'s browser-family
  lane also edits its `normalise_relative_path`: `allowRoot` → `''` for the
  root, and a FileOpError INVALID_PATH class (`database-store.ts:55-58`,
  `path-utils.ts:30-44`). Name a stacking order.
- **`api/documents.rs::chat_reachable_stores`** (`:1188-1300`) and
  **`doc_edit/path_resolver.rs`** (the tier walk / roster) belong to
  `0c47f4539`'s reach lane. DOCCORE edits only `accessible_stores_body`
  (`:1122-1183`, `generalLibrary`) and appends the stat helpers.
- **`quilltap-web/src/files_routes.rs`, the folders route
  (`api/mount_files.rs:476-629`, `is_path_safe`) and the realtime topic map**
  are shared with `600c2906f` (`isSafeRelativePath`, the `wardrobe` topic)
  and `aec626eff`'s folders-route publish. Give STORES the folders-route
  publish, and the browser-family lane the normaliser swap.
- **`about-page.ts`** is touched by `db4a76e39`, `c34986421`, `856ed4f2b`,
  `70d2859d8` and other areas. ONE lane (the vendor / About rider) takes
  every line.
- **The generated tool catalog**
  (`tools/definitions/data.rs`) does not move in this area: no tool
  description changed (verified against the hunks).

---

## 8. Tiering

| item | tier |
|---|---|
| `cloud_placeholder`, `neutralise_unsafe_hrefs`, `compute_rename_target` (re-aim) | tier-1 exact |
| document core reads / writes / moves / sweeps, `listForBrowser`, `clearLinkGroup`, folder copy / move / recursive delete, `linkDocumentContent` | tier-2 DB diff |
| route bodies / status / `availableActions` | tier-2 route families (byte compare) |
| `render-html` | per R-1 (refusal pinned both ways, or structural) |

---

## 9. Dogfood rows

- **#133:** a Document Mode save in a project chat → the file is chunked at
  once (check `chunkCount`).
- **#134:** a chat-uploaded `.md` opens, edits and saves in Document Mode,
  and `doc_str_replace` edits it. The blob becomes a document with its
  extraction fields cleared.
- **#137:** `doc_open_document` with no path, in a chat with no project →
  the row says `general` and Document Mode opens it.
- **The sweep:** the same file is open by store name in chat A and as
  `project` in chat B; a `doc_move_file` moves both rows.
- **The Scriptorium:** links / unlink / copy-folder / cross-store move /
  recursive delete. A filesystem store with a stray unindexed file refuses.
- **The scan:** a planted evicted placeholder (`brctl evict`) on a COPY
  store → `filesInCloud: 1` and no download.

---

## 10. Rulings owed to the human

- **R-1 — `?action=render-html` (mammoth).** Options:
  - **(A) recommended:** a named refusal for `.docx`
    (`DOCX_RENDER_HTML_UNAVAILABLE`, pinned both ways). Keep v4's non-DOCX
    400 and the route shape. The SPA viewer offers download. This is
    consistent with v5's refusing DOCX/PDF extractor; revisit when the
    converter ruling (prior survey R-2) lands.
  - **(B):** a Rust `.docx` → HTML subset over the existing `zip` crate +
    `quick-xml`. It can never be byte-equal to mammoth, so it is a permanent
    structural-only family.
  - **(C):** mammoth's browser build inside the Angular viewer. The server
    still refuses.
- **R-2 — the doc-edit reindex deferral.** Options:
  - **(A) recommended:** port `after_document_write` as the ONE chokepoint
    (gate = `mountPointId`, + link-group siblings) under both Document Mode
    and the tools. Production goes through `DbMountRefreshScheduler`; tests
    use the synchronous `run_refresh`. This retires the "standing reindex
    deferral" (`shared.rs:11-34`) for database stores and fixes #133 in the
    same move. The oracle awaits the chain.
  - **(B):** fix only Document Mode's gate (#133) and keep the tools seamed,
    with the oracle seam re-aimed as in §5.1.
- **R-3 — the Windows arm of `isEvictedCloudPlaceholder`.**
  `MetadataExt::blocks` is unix-only. **Recommend:** `#[cfg(not(unix))]` →
  `false`, recorded as a divergence until Tauri-Windows.
- **R-4 — fs-branch error classes (`4310d435f`).** **Recommend:** CONVERGE.
  Only `NotFound` and `NotADirectory` read as absent; every other `io::Error`
  propagates (→ 500). This applies at `documents/mod.rs:558-561,627-631` and
  `file_management.rs:1035-1047`.
- **R-5 — parent-side `mountPoints` realtime hints.** **Recommend:** port
  `publish_mount_change` at v4's chokepoints, each pinned in
  `publish_sites.rs`. The alternative is to keep job-only hints and leave the
  SPA's arm empty.
- **R-6 — a v4 filing candidate (draft only, the human's go-ahead first).**
  `findUnderFolder` (`chat-documents.repository.ts`) matches the prefix
  case-SENSITIVELY. Its sibling `planFolderTransfer` / `under()` is
  case-insensitive, and database folder paths are NOCASE. So a
  case-differing open row is not swept after a folder move. **Measure on v4
  before drafting.**
