# Survey C1 — documents / document stores / mount index / files / the Log / host boot + cadence / the cipher sys crate

Main @ `8ad1a0640` (2026-11-10). v4 read at the pin `624d0d25c` only (`git show` / `git grep`);
past-pin facts come from `git log 624d0d25c..HEAD -- <path>` / `git show --stat` (v4 HEAD
`0d73a4aee`), never the checkout. Paths are repo-relative to `~/source/quilltap-v5` (v5) or
the v4 pin. Nothing was built or run.

Orders read: P4.D354, P4.D355, P4.D365, P4.D376, P4.D380, P4.D381, P4.166, P4.171 (status
headers + lane records) and the three carried lists.

**Tally: 14 OPEN (two of them recommended OUT) · 3 CLOSED · 9 NO-ACTION · 3 NEEDS-RULING ·
6 DEFER-TO-CATCH-UP · 1 FINDING (new, unassigned).**

Suggested lanes (owned files in §Z): **C1-HINTS** (the store's realtime hint at the eight
emit-less blob writers, S–M) · **C1-DOCS** (Document Mode / doc-tool smalls + pins, M) ·
**C1-STORES** (mount-files route ERRORs, the `eprintln!` class, the folder-move NULL parent,
M) · **C1-BOOT** (the backfill failure row, the cadence record, the transcript stat WARN, S) ·
**C1-FK** (the `SQLITE_DEFAULT_FOREIGN_KEYS` hold — ruling R2, S–M).

---

## §A — The store's realtime `mountPoints` hint

### A1 — P4.D381 item 18: the four named outer emit sites   [OPEN]
- Source: P4.D381 status header; status-log 202718; phase-4.md 7245.
- How the chokepoint was wired (the model to copy): `6f9633ac2` added
  `crate::services::mount_index::realtime::emit_document_written(mount_point_id, &rel)` /
  `emit_document_deleted` inside `db/doc_mount_file_links.rs:730` / `:1342` (the repository
  twins of v4's ONE `writeDatabaseDocument` / `deleteDatabaseDocument`) and
  `services/wardrobe_image_bridge.rs:298` (v4 `:213`); pinned by
  `realtime/publish_sites.rs` (`the_repository_write_and_delete_twins_publish`, `:1942`, each
  with a silence leg) and by two census rows in
  `crates/quilltap-harness/tests/realtime_publish_sites_guard.rs` (`(file, "realtime::emit_document_", count, why)`).
  `72600dbab` then moved two publish-ORDER pins (`db/wardrobe_location.rs`,
  `inventory/location.rs`: the store hint lands AHEAD of the topic).
- The four sites, measured on main (none emits; each is a direct `link_blob_content` call that
  v4 follows with `emitDocumentWritten`):

  | v4 at the pin | v5 site | note |
  |---|---|---|
  | `lib/photos/character-gallery-service.ts:196` | `photos/character_gallery_service.rs:337-370` (comment `:369-370` "host-side recorded seams (no-op in the core)") | emit after `chunk_and_insert_extracted_text`, before `Ok(save_output…)` |
  | `lib/photos/user-gallery-service.ts:253` | `photos/user_gallery_service.rs:661-692` (same comment) | same position |
  | `lib/photos/save-image-to-album.ts:318` | `photos/save_image_to_album.rs:421-461` | sits beside the `side_effects.invalidate_mount_point` / `enqueue_embedding_jobs` seam (`:458-459`); v4 order is invalidate → **emit** → enqueue |
  | `lib/file-storage/character-vault-bridge.ts:191` | `services/image_job_storage.rs:157-232` `write_main_avatar_to_vault` (the `kind === 'main'` arm; `link_blob_content` at `:199`) | emit after the link, before `Ok(AvatarWrite…)`; v4's `refreshStats` follows it |
- Done: one `emit_document_written` per site, a `publish_sites.rs` test per site with a silence
  leg (the refused / failed write publishes nothing), four census rows.
- Past the pin: `git log 624d0d25c..HEAD -- lib/photos lib/file-storage` is EMPTY — not on a
  rewritten surface (`ebb0ca13a` / `1dc11e947` do not touch these files). Safe now.
- Size: S.

### A2 — NEW (same class, found measuring A1): four hand copies of v4's `storeMountFile` blob write never emit   [OPEN]
- v4 `lib/mount-index/store-file.ts:430` emits `emitDocumentWritten` for EVERY `storeMountFile`
  caller. v5's `services/mount_index/store_file.rs:644` carries it, but four v5 writers
  re-implement the blob branch around `link_blob_content` and have no emit
  (`ggrep -c emit_document` = 0 in each file):

  | v4 caller of `storeMountFile` | v5 hand copy | reached by |
  |---|---|---|
  | `character-vault-bridge.ts:215` (history avatars), `lantern-store-bridge.ts:107` | `services/image_job_storage.rs:269-343` `store_blob_to_mount` (link at `:310`) | avatar / story-background jobs |
  | `project-store-bridge.ts:144`, `user-uploads-bridge.ts:107` | `services/file_storage.rs` `store_mount_blob` (link at `:1152`; the `_indexed` bridges layer extraction on top) | every project-file / chat upload |
  | `lantern-store-bridge.ts:107` via `saveGeneratedImage` | `tools/generate_image.rs:1298-1400` `save_generated_image` (link at `:1362`) | the `generate_image` tool |
  | `wardrobe-image-bridge.ts:145` | `services/wardrobe_image_bridge.rs:215` (the WRITE; only its delete `:298` emits) | wardrobe / inventory item pictures |
- Consequence: an open store browser on Uploads / a project store / Lantern Backgrounds / a
  vault does not refetch after an upload, a generated image or a new item picture.
- Done: one emit per copy (after the link, v4's position: after the optional extraction, before
  the result), a `publish_sites.rs` pin + census row each. Do NOT move the emit into
  `link_blob_content` itself — v4's `linkBlobContent` does not emit, and `file_ops.rs:489` /
  `store_file.rs:644` / `apply_store.rs:259` would then publish twice.
- Trap (memory note "emit-less chokepoint twins"; `72600dbab` / `f46370443`): every harness
  family whose comparand records v5's realtime publishes over these writers gains a
  `(mountPoints, <id>)` entry — expect moved rows in the avatar / upload / photo families
  (`salon_mutations`, `projects_routes`, `subprompts_routes`, `store_name_reconcile`,
  `wardrobe_wear_import_remap`, `mount_case_repair` are the ones that mention `mountPoints`
  today); v4's side publishes them already, so each move should CONVERGE — re-run by name.
- Size: S–M (the pins and the union reds are the cost).

### A3 — "no filesystem-store realtime hint"   [NEEDS-RULING — R1]
- Measured: in v4 a FILESYSTEM store's hint comes from the watcher
  (`lib/mount-index/watcher.ts:187`, debounced per store; `c17033f04`'s `schedulePublish`).
  v5 has no watcher (the standing "chokidar-equivalent watcher (host cadence)" deferral,
  `phase-4.md:1176`; P4.D380 item 21 NO-COUNTERPART), so a Document-Mode / doc-tool write to
  a filesystem store publishes nothing: `documents/core/*` and
  `services/mount_index/refresh.rs::refresh_after_document_write` contain no publish
  (database stores are covered by `db/database_store.rs:592`). File-op and folder-op routes
  DO publish for filesystem stores (the five wrappers, `folder_ops.rs`).
- Options: (a) leave it under the watcher deferral (NO-ACTION, recorded as
  `FS_STORE_HINT_AWAITS_WATCHER`); (b) publish once from `refresh_after_document_write` when
  the mount is a filesystem store — v5's in-app writes then behave as v4's do (the hint
  arrives, coalesced), external edits still wait for the watcher. A named v5-side stand-in,
  one line + one pin + one census row.
- Recommendation: **(b)**, in lane C1-STORES (it owns `refresh.rs`). S.

---

## §B — Document Mode, the document core, the doc tools (P4.D380, P4.D376)

### B1 — `accessible_stores`' user   [OPEN — micro-ruling inside]
- The round record's wording ("should come from `chat.userId`") reads backwards against the
  code. Measured: v5 `api/documents.rs:1494-1498` ALREADY reads `chat.userId` (P4.D376's
  lane-local call 5: "v4's session user owns the chat"); v4
  `app/api/v1/chats/[id]/actions/documents.ts:269` passes the SESSION `user.id`
  (`resolveCastPool({ userId: user.id, … })`), and `repos.chats.findById` is not
  user-scoped (`lib/api/middleware/context.ts:116` `getRepositoriesSafe()`), so the two differ
  for a chat whose `userId` is not the session user. v4's session user is the fixed
  `ffffffff-ffff-ffff-ffff-ffffffffffff` (`lib/auth/single-user.ts:18`) = v5
  `api/engine.rs:46` `SINGLE_USER_ID`.
- Effect: `resolve_cast_pool` drops a character whose `userId` differs from the pool's user
  (`db/reach.rs:285`, `:557`). On a chat row carrying a foreign / empty `userId`, v4 keeps
  the session user's characters' vaults and v5 keeps the chat-owner's.
- Done: pass `SINGLE_USER_ID` (the engine arm `api/engine.rs:4471-4474` → a `user_id`
  argument, as `SystemHome` does at `:4635`), plus one `documents_routes` row with a
  foreign-`userId` chat (both sides). Differential:
  `crates/quilltap-harness/tests/documents_routes_equivalence.rs` (`chat_accessible_stores_*`,
  `p4d376_accessible_stores_cast_pool`) / `harness/oracle/cases/documents-routes.test.ts:320,327,620`.
- Size: S. Meeting point: `api/engine.rs` (one arm).

### B2 — a malformed `chat_documents` row with NULL `isActive`   [OPEN]
- v5: `db/chat_documents.rs:581` `is_active: row.get::<_, i64>(6)? != 0` and `:613`
  `Value::Bool(r.get::<_, i64>(6)? == 1)` — a NULL cell is `InvalidColumnType`, so the read
  ERRORS (`find_by_id` → 500; the P4.D380 finders fall back to `[]` for the WHOLE result).
  The column is nullable: `fresh_schema.json` `"isActive" INTEGER DEFAULT 1`.
- v4: `lib/database/backends/sqlite/backend.ts:445-452` — a NULL boolean hydrates
  `undefined`, a number hydrates `value === 1`, anything else `Boolean(value)`; then
  `ChatDocumentSchema.isActive: z.boolean().default(true)`
  (`lib/schemas/chat-document.types.ts:28`) → **`true`**. (The open-list SQL filter
  `isActive = 1` still excludes the row on both sides.)
- Two defects in one helper: NULL → error (v4 `true`), and `:581`'s `!= 0` reads `2` as
  `true` where v4 reads `false` (`:613` already uses `== 1`).
- Done: one `js_bool_cell(row, 6, default = true)` for both readers (NULL → default, INTEGER
  → `== 1`, TEXT → non-empty); rows in `chat_documents_tier2_equivalence` /
  `harness/oracle/cases/chat-documents-tier2.ts` planting `isActive` NULL and `2` (plants
  through a raw UPDATE on both sides, self-checked — memory note "tier-R vacuous-equal rows").
- Size: S.

### B3 — P4.D380 Tier 2 item 18: capture pins for the ported operator ERROR / WARN arms   [OPEN]
- Lines ported in `api/documents.rs`, unpinned: WARN `<Subject> save conflict detected`;
  ERROR `Failed to read / write / rename / delete / open <subject>`; ERROR `Failed to resolve
  stores for standalone document picker` (status-log 201231). The lane stopped because each
  needs a planted store failure that fails on BOTH sides.
- Done: per arm, a `documents_routes` row whose plant makes v4's AND v5's read throw (memory
  notes "throwing plants must fail both reads": make the read column itself generated /
  rename the table the op reads — an extra VIEW fails only v4), comparing status + body +
  lines. Family: `documents_routes_equivalence.rs` (its `cap` / lines rows at `:895`),
  oracle `documents-routes.test.ts`.
- Note: `chat_accessible_stores` (`api/documents.rs:1427`) swallows its error as `Err(_e) =>
  server_error(…)` with NO line — check v4's catch for an ERROR line while planting.
- Size: M (harness-heavy; production edits only where a plant exposes a wrong line).

### B4 — P4.D380's seven unpinned lines   [OPEN]
- DEBUG `openDocumentFile: opening read-only` / `file not readable` (`documents/mod.rs`,
  writer thread), DEBUG `resolveGeneralLibrary: Quilltap General is missing or disabled`
  (`documents/mod.rs`; the corpus's General is enabled), DEBUG `doc_open_document: created a
  blank document` (`tools/doc_edit/document_ui.rs`), INFO `Moved document` / `Deleted
  document` (`tools/doc_edit/file_management.rs:193-199`, `:536-541`), WARN `Failed to find
  blob by path` (`db/database_store.rs`).
- Done: `global_capture` unit pins (the writer-thread ones cannot use `captured_with` —
  memory note "capture rig vs the writer thread"), or lines rows in `doc_fs` / `doc_ui` /
  `documents_routes` where the family already records lines.
- Size: S–M. Same lane as B3 / B5.

### B5 — P4.D380 item 22: eleven v4 INFO lines never ported   [OPEN]
- Measured absent on main (grep of each literal over `crates/quilltap-core/src`,
  `crates/quilltap-host/src` = 0). v4 at the pin:
  `lib/tools/handlers/doc-edit/file-management-handlers.ts:302` `Copied document`
  `{source_mount_point, source_path, dest_mount_point, dest_path, bytes}`; `:416` `Created
  database folder` `{path, scope}`; `:438` `Created folder` `{path, scope}`; `:483` `Deleted
  database folder` `{path, scope}`; `:537` `Deleted folder` `{path, scope}`; `:599` `Moved
  database folder` `{from, to, scope}`; `:671` `Moved folder` `{from, to, scope}`;
  `text-handlers.ts:257` `JSON validation failed on write` `{path, error}`; `:266` `JSON
  serialization failed on write` `{path, error}`; `document-ui-handlers.ts:206` `Opened
  document in Document Mode`; `:298` `Closed document in Document Mode` (read each line's
  field list at the pin).
- v5 homes: `tools/doc_edit/file_management.rs`, `tools/doc_edit/text.rs`,
  `tools/doc_edit/document_ui.rs`.
- Past the pin: `2f0a033ca` rewrites `text-handlers.ts` (+43) but only `handleGrep` /
  `handleListFiles` (the `guardOwnVault` hunks) — the write handler's two lines are untouched;
  `file-management-handlers.ts` and `document-ui-handlers.ts` are unchanged. Safe now.
- Differential: `doc_fs_equivalence`, `doc_text_equivalence`, `doc_ui_equivalence` (they
  compare no tool INFO lines today — add lines rows or `global_capture` unit pins).
- Trap: an `error` field name is taken by the file layer's envelope (`log_file.rs`); v4 passes
  `error` as CONTEXT here → use the `errorJson` suffix rule.
- Size: S–M.

### B6 — K12 / P4.D380 item 25: `mtime` `0` where v4 gives `NaN`   [NO-ACTION — recorded both ways]
- `db/database_store.rs:283`, `:328`, `:384` (`…​.unwrap_or(0)`); `UNPARSEABLE_LAST_MODIFIED_READS_ZERO`
  is asserted both ways in `doc_blob_document_reads_equivalence.rs:23,94`. Reachable only
  with an unparseable `lastModified` / `updatedAt` cell; converging means an `Option<i64>`
  through four reader structs and the mtime-conflict compare. Leave recorded.

### B7 — P4.D380 H1: the dead "4. Quilltap General" step in `chat_reachable_stores`   [CLOSED]
- `api/documents.rs:1650-1652` — the block is gone ("No fourth step").

### B8 — P4.D380 item 19 / the case-sensitive folder sweep (#147)   [DEFER-TO-CATCH-UP]
- v4 `88c9cc81d` (past the pin) makes `chatDocuments.findUnderFolder` and
  `moveFolderWithinMount` case-insensitive and adds `sweepOpenDocuments`. v5 ports the pin's
  behaviour verbatim; nothing to do before that commit is ported.

### B9 — P4.D380 items 21 / 23 / 24 (`schedulePublish` NO-COUNTERPART, `edit_custom_tool`'s reader, `TOOL_REFRESH_IN_CLOSURE`)   [NO-ACTION — recorded]

### B10 — P4.D376's two recorded divergences in `reach_equivalence`
- `RAW_CHARACTER_ROW_UNVALIDATED` (a `userId: ''` character admitted): **DEFER-TO-CATCH-UP** —
  `1dc11e947` rewrites `characters.repository.ts` (+486 / −694) and adds `CharacterRowSchema`.
- `HYDRATED_OWNER_ROW_UNVALIDATED` (`ProjectsRepository::find_by_id` reads a BLOB-named row,
  `GroupsRepository::find_by_id` `Err`s, both silent, where v4's `findById` is null after its
  two lines): **OPEN, recommended OUT** — pre-existing, outside reach, the row-validation
  class of the projects / groups repositories (another area's files); M.

---

## §C — The mount-point store API (P4.D381)

### C1 — item 17: fifteen `eprintln!` route failures in `api/mount_files.rs`   [OPEN]
- Measured (15, unchanged since lane close): `:204` (`map_file_op_only`'s generic arm),
  `:265` move file, `:307` copy file, `:345` link file, `:365` delete file, `:1052` update
  file, `:1157` create folder, `:1268` / `:1272` write file (item route), `:1314` writing
  file, `:1426` / `:1430` upload blob, `:1454` list blobs, `:1487` delete blob, `:1534` blob
  description.
- v4 lines (all `logger.error(msg, ctx, error)`): `app/api/v1/mount-points/[id]/route.ts:607`
  `[Mount Points v1] Error moving file` `{mountPointId}`; `:646` `…Error copying file`;
  `:678` `…Error linking file`; `:906` `…Error writing file`; `:949` `…Error deleting file`;
  `folders/route.ts:99` `…Error creating folder in mount point`;
  `files/[...path]/route.ts:59` `` `…Error ${action}` `` with `ctx` (the item route: `write
  file`, `update file`, … — read each `handleFileOpError(…, action, ctx)` call for its `ctx`
  keys); `blobs/route.ts:40` `…Error listing blobs`, `:133` `…Error uploading blob`;
  `blobs/[...path]/route.ts:136` `…Error deleting blob` `{mountPointId, path}`, `:171`
  `…Error updating blob description` `{mountPointId, path}` (also `:100` `…Error serving blob`
  — check whether v5's serve arm logs it).
- Model: `log_route_failure` (`api/mount_files.rs:422-443`, static callsites per literal) and
  the scan arm `:182-188`; pins in `mount_ops_equivalence.rs:188` (the ERROR list) — extend
  that list, or unit capture where a failing plant is impractical.
- Past the pin: `1dc11e947` touches `mount-points/[id]/route.ts` by 6 lines only. Safe now.
- Size: M (mechanical; 15 sites, ~11 distinct literals).

### C2 — the same class one level down: 28 `eprintln!` in `services/mount_index/`   [OPEN — optional second unit]
- Not in item 17's text, same defect (a line that never reaches the log file):
  `file_ops.rs:386,597,744,866,977,996,1005,1011` (8), `refresh.rs:42,48,138,142,171,174,206,215`
  (8), `general_state.rs:112,119,128` (3), `reindex_file.rs:165,264` (2), `store_file.rs:519,651`
  (2), `link_groups.rs:57`, `converters.rs:105`, `reindex.rs:345` (1 each; `embedding_scheduler.rs:138`
  is a comment). Also `api/settings.rs` 5, `api/salon.rs` 2 (other areas).
- Each needs its v4 line MEASURED (several are v4 `logger.warn`s with different text, e.g.
  `store-file.ts`'s best-effort stats refresh, `general-state.ts`'s parse fallbacks; some have
  no v4 line at all and should become silent or a v5-named DEBUG). Not a mechanical swap.
- Size: M. Take it only with C1's lane; otherwise carry by name.

### C3 — `move_database_folder` leaves a stale `parentId` on a move to the top level   [OPEN]
- v5: `db/database_store.rs:966-1001` — `dest_parent_id` stays `None` when `dest_dir == "."`
  and `DmfUpdate { parent_id: None, .. }` (`db/doc_mount_folders.rs:78-84`) means "unchanged",
  so `a/sub` → `sub` keeps `a`'s id as its parent.
- v4: `lib/mount-index/database-store.ts:584-610` — `destParentId: string | null = null`,
  `update(sourceFolder.id, { parentId: destParentId, … })` writes NULL.
- Done: the explicit clear P4.D381 used for the filesystem move
  (`services/mount_index/folder_ops.rs:228-236`: `UPDATE doc_mount_folders SET parentId =
  NULL WHERE id = ?1` when the parent is `None`) — or widen `DmfUpdate.parent_id` to a
  tri-state and retire both explicit clears (touches every `DmfUpdate` literal; prefer the
  one-statement fix). Row: a nested-folder → top-level move in `mount_ops_equivalence`
  (database store; dump `doc_mount_folders.parentId`) and/or `doc_fs_equivalence`
  (`doc_move_folder`).
- Past the pin: `88c9cc81d` rewrites the CALLER `moveFolderWithinMount` (adds
  `sweepOpenDocuments` after `moveDatabaseFolder`, case-insensitive matching on the
  filesystem arm) but does NOT touch `moveDatabaseFolder` in `database-store.ts`. The fix is
  not thrown away; do it now.
- Size: S.

### C4 — `DOCX_RENDER_HTML_REFUSED`, `PLACEHOLDER_CHECK_UNIX_ONLY`   [NO-ACTION — standing named refusals; the first re-opens on a DOCX converter ruling]

### C5 — value-only log renderings (a `null` field omitted; route ERRORs carry `error=<message>`)   [NO-ACTION — recorded, the file layer's rendering]

---

## §D — Files (P4.D354)

### D1 — Tier 2 item 19: the backfill family has no planted `list_unextracted_by_extensions` failure row   [OPEN]
- v4 `lib/startup/backfill-upload-blob-text.ts:107` WARN `Uploaded-blob text backfill failed`
  (the outer catch); v5 `services/mount_index/upload_blob_text_backfill.rs` (WARN + its silence
  leg unit-pinned only).
- Done: one row in `upload_blob_text_backfill_equivalence.rs` /
  `harness/oracle/cases/upload-blob-text-backfill.test.ts` whose plant fails the listing on
  both sides; compare the returned counts + the one WARN.
- Trap: the read is likely a fallback on one side — measure which line v4 actually emits
  (the repository's inner `Error …` line may fire INSTEAD of the outer WARN; P4.D380's item-9
  correction is the same shape).
- Size: S (harness-only unless the plant exposes a wrong line).

### D2 — H5: the extractor order (the seam → `Result`, v4's catch arm, the pdfjs rules, the re-queue of pre-round `failed` rows)   [NO-ACTION this round — blocked]
- Every part hangs on a production pdf / docx `DocumentTextExtractor` (the standing deferral,
  `REFUSING_SEAM_LEAVES_PDF_UNEXTRACTED`), which needs a crate ruling. Not a smalls item.

### D3 — K10: the upload text backfill runs inside the boot   [OPEN — record; ruling R3 if anyone wants more]
- v5: `crates/quilltap-host/src/host.rs:2003-2012` — called synchronously in the mount-index
  boot closure (after `ensure_builtin_mounts_with`), so the engine is not ready until it ends.
- v4: `instrumentation.ts:711-729` — chained onto `mountScanSettled` AFTER PHASE 3.3's
  database scan; the server is already `isReady` (`:1051-1052`), and only the UI gate
  (`backgroundSettled`, `:1071-1079`, capped at 5 minutes) waits for it. So the note's "fire
  and forget" is half right: requests are served, the startup screen is held.
- One-time cost (a no-op once every native-text upload has text; pdf / docx skipped while the
  seam refuses), so the divergence is boot LATENCY on the first boot only.
- Done (recommended): record it as a named divergence `UPLOAD_TEXT_BACKFILL_IN_BOOT` in the
  `host.rs` comment + the backfill module doc + the order header, and add the first-boot time
  to the dogfood queue. Moving it to a post-ready writer task is M, touches host cadence, and
  buys nothing after the first boot — not recommended.
- Size: S (docs + a comment). Meeting point: `host.rs`.

### D4 — K12   → see B6.

### D5 — Tier 3 items 24–30 (unreachable converter-throw arm, the two unreachable backfill lines, the R-1 carve, `assert_persistent_target` without a production caller, NO-PORTs, `ProcMountInfo` off Docker)   [NO-ACTION — recorded]

---

## §E — The Log carrier + terminals (P4.D355)

### E1 — the orphan-transcript sweep: a stat failure is a silent skip   [OPEN — optional]
- v5: `crates/quilltap-host/src/host.rs:2922-2931` — `Err(_) => (false, 0)` folds a failed
  `metadata` into `is_file: false`.
- v4: `lib/database/repositories/terminal-sessions.repository.ts:246-265` — a non-ENOENT
  `fs.stat` / `fs.unlink` failure WARNs `Failed to remove orphaned terminal transcript`
  `{sessionId, filePath, error}`.
- Done: `TranscriptEntry` (`services/scheduled_maintenance.rs`) carries the stat error;
  the sweep WARNs on a non-NotFound one. Unit-pinned (v4's arm needs an EACCES plant).
- Size: S. Low value; take it only if C1-BOOT has room.

### E2 — Tier 3 items 25–30 (`TERMINATE_NON_UNIX_IS_KILLER`, the job-child sentence, the listing-`Err` arm, the Log out of `.qtap`, help, no SPA)   [NO-ACTION — recorded]
- K6 / K7 / the `ariel_writers_tier3` cell / H9: other surveys'.

---

## §F — The cipher sys crate (P4.D365)

### F1 — `SQLITE_DEFAULT_FOREIGN_KEYS` held (`SQLITE_DEFAULT_FK_HELD`)   [NEEDS-RULING — R2]
- State on main: `crates/quilltap-sqlite3mc-sys/build.rs:71` `HELD = [("SQLITE_DEFAULT_FOREIGN_KEYS",
  "SQLITE_DEFAULT_FK_HELD")]` (printed as a `cargo:warning`, asserted non-stale at `:105-110`);
  the twin table `sqlite_build_parity_equivalence.rs:71` (`HELD = ["DEFAULT_FOREIGN_KEYS"]`,
  carved both ways at `:258-272`). Production: the writer sets `foreign_keys = ON`
  (`db/mod.rs:395`) and — since the unification — so does the CLI's writable open
  (`crates/quilltap-cli/src/dbopen.rs:123-134`, test `:195`). **H3's CLI divergence is CLOSED.**
- What still differs from v4: every OTHER raw connection (read-only pools, tests, harness
  plants, any production helper opening its own `Connection`) runs with FKs off where v4's
  driver enforces them. Reads cannot violate a key, so the live difference is fixtures.
- What converging takes:
  1. empty `HELD` in `build.rs` and in the parity test (59 compile options equal, no carve);
  2. migrate the FK-off fixture premises — the ONE the lane's full-parity gate found is
     `restore_vintage_state.rs:691-714` (`annotations_restore_onto_a_vintage_instance_without_colliding`
     seeds a `conversation_annotations` row whose chat the vintage instance lacks, through a
     raw connection): seed the chat, or `PRAGMA foreign_keys = OFF` on that seed connection;
  3. keep or drop the CLI's explicit pragma (keep — harmless, self-documenting);
  4. CLAUDE.md's cipher paragraph ("`SQLITE_DEFAULT_FOREIGN_KEYS` held") at unification.
- Cost, measured by P4.D365 (status-log 197782): a cold amalgamation compile is **1 m 12–14 s
  dev/test**, and rides inside ~2 m 22 s for release — NOT four minutes. Changing `build.rs`
  re-runs the build script, which recompiles the C; the sys crate's VERSION does not need a
  bump (the rule is "bumped only when the vendored amalgamation moves" — the bytes do not
  move). Who pays: the owning lane (dev + release, once each) and the unifier (once each).
  Sibling lanes clone main's pre-change `target/` and never see it. "Every lane recompiles"
  is true only if the change lands on main BEFORE `/setupphase`'s stamp → build → file cycle,
  which would absorb it into the cloned artifacts for free.
- Risk: the lane measured with full parity BEFORE 27 siblings landed — `cargo test -p
  quilltap-core --lib` 4,200 / 0 and exactly one workspace red. The union has more fixtures;
  the count of FK-off premises today is unmeasured (no cargo in this survey).
- Options: (in) its own tiny lane, with the decision rule "≤ 5 fixture reds → migrate each;
  more → restore the hold and record the list"; (pre) the planner flips it on main before
  the setup build, gated by one `cargo test --workspace --no-fail-fast`; (out) carry the hold.
- Recommendation: **in, as its own lane (C1-FK)** — the production gap is already closed,
  so this is a test-suite migration with a cheap, well-understood compile cost, and it removes
  a both-ways carve and a `cargo:warning` from every build. If the planner prefers zero
  cross-lane risk, **out** costs nothing but the carve.

### F2 — H3: the CLI `db --write` FK divergence   [CLOSED — `dbopen.rs:131-134`, pinned `:195`]

### F3 — R-2 (`SQLCipher driver loaded` NO-PORT), `REAL_INSTANCE_OPEN_AWAITS_DOGFOOD`   [NO-ACTION — the second is the dogfood pass's first row]

---

## §G — P4.166 (the degraded mount index)

### G1 — 156.c: the merge on a degraded mount + the missing `[ChatMerge]` INFO lines   [DEFER-TO-CATCH-UP]
- Still open on main: `services/chat_merge.rs:466` (`let Some(mount_w) = w.mount_index()
  else …`), `:250` / `:295` (`read_mount_index`); `Merged character into chat` / `Merge
  complete` absent (grep = 0); `degraded_mount_routes_equivalence.rs:85` keeps
  `chat_merge_vaultless` in `UNCLAIMED_ROWS` (`DIVERGENCE:` line `:641`).
- v4 past the pin REWRITES the surface: `348172b9c` `lib/chat/apply-chat-merge.ts` +15 / −72
  (the Green Room / opening refactor), then `3db965416` +9 (the arrival inventory pass). An
  M-size `chat_merge.rs` unit now would be redone with the catch-up.

### G2 — character delete on a degraded mount (v4 200, v5 500)   [DEFER-TO-CATCH-UP]
- Still open: `api/characters.rs:2202` (`with_both_conns`), `UNCLAIMED_ROWS` `character_delete`
  (`DIVERGENCE:` `:624`). v4's handler `characters/[id]/handlers/delete.ts` is unchanged past
  the pin, but the fail-soft vault clean-up it relies on lives in `characters.repository.ts`
  (`1dc11e947`, +486 / −694) and `lib/cascade-delete.ts` moves with `ebb0ca13a` (+8, the
  avatar timeline). Port with those.

### G3 — the five unported `[Characters v1]` INFO lines (`Avatar updated`, `Tag added`, `Tag removed`, `ControlledBy toggled`, `Default partner updated`)   [DEFER-TO-CATCH-UP]
- Absent on main (grep = 0). v4 `characters/[id]/handlers/post.ts` is rewritten by
  `1dc11e947` (67 lines) and `4089469a8` (17). Characters area, not this one.

### G4 — the announcement job's degraded arm (order item 7, unmeasured)   [OPEN — the wardrobe survey's file]
- Only "the rollback ENQUEUES `WARDROBE_OUTFIT_ANNOUNCEMENT` on a degraded mount" was
  measured; the job's own degraded run was not. P4.170's file; S. Meeting point, not a C1 lane.

### G5 — the unmeasured C1 sites kept on the neutral refusal (gallery remove / save-by-id, `group_scenarios_union`, depiction guidelines, the scenario reads, preview-avatar, transfers POST)   [NO-ACTION — recorded; each is a degraded-rig measurement, none is a known wrong answer]

---

## §H — P4.171 (host boot)

### H1 — v4's two store backfills (PHASE 3.4a `backfillProjectStores`, 3.4b `backfill-group-stores`)   [OPEN — recommended OUT]
- Still unported: `services/startup_scenarios.rs:19-24` records it; no `backfill_*_stores` in
  the workspace. v4 `lib/startup/backfill-project-stores.ts` (134 lines) /
  `backfill-group-stores.ts` (134): `findAllRaw` → `ensureOwnerOfficialStore` → populate the
  four overlay files from the raw row ONLY when `properties.json` is genuinely absent → INFO
  `Project store backfill scanning {total}` / `… complete {result}` every boot.
- Why out: (1) the populate step reads v4's WIDE pre-cutover row (the legacy store-resident
  columns) — post-cutover those columns are gone and v4 itself writes defaults; v5's raw
  finders carry five slim columns, so a faithful port is a new wide read for data no live
  instance has; (2) its callee `lib/mount-index/ensure-owner-store.ts` /
  `ensure-official-store.ts` is rewritten past the pin by `1dc11e947` (+120 / −… in the
  second); (3) the observable residue is two INFO lines per boot and the `provisioned` count
  on an instance with a storeless owner. M for a boot path that P4.171's scenario heals
  already cover in effect. Carry by name; revisit after the `1dc11e947` port.

### H2 — the closing overlay re-read of `update_character` (`RECORDED_COUNT_DIVERGENT_WARNS` 2 / 1)   [DEFER-TO-CATCH-UP — `1dc11e947` rewrites `characters.repository.ts`'s update]

---

## §I — FINDING (new, not on any list)

### I1 — `photos/save_image_to_album.rs:47` declares v4's `SINGLE_USER_ID` as `00000000-0000-0000-0000-000000000000`
- v4 `lib/auth/single-user.ts:18` is `ffffffff-ffff-ffff-ffff-ffffffffffff` (and v5's own
  `api/engine.rs:46` / `services/provisioning/mod.rs:73` agree with v4). The local constant
  feeds `IngestImageRequest.user_id` at `:297` — the mount-blob-fallback ingest of `keep_image`
  (v4 `save-image-to-album.ts:193` `userId: SINGLE_USER_ID`), i.e. the `files` row's `userId`.
- On main since `baa9f6f87` (2026-07-06, W4.9b). Not measured: whether
  `photo_tools_equivalence` has a row through the fallback ingest (if it does, the id is
  masked or the oracle mocks the module — check before fixing).
- Done: re-export the one constant; a `files.userId` assert on the fallback-ingest row.
- Size: S. The file is in lane C1-HINTS (A1), so it rides there.

---

## §Z — Lanes, owned files, meeting points

| lane | scope | owned source files | harness / oracle |
|---|---|---|---|
| **C1-HINTS** (S–M) | A1 + A2 + I1 | `photos/character_gallery_service.rs`, `photos/user_gallery_service.rs`, `photos/save_image_to_album.rs`, `services/image_job_storage.rs`, `services/file_storage.rs`, `tools/generate_image.rs`, `services/wardrobe_image_bridge.rs`, `realtime/publish_sites.rs` | `realtime_publish_sites_guard.rs`; re-run every family that records `mountPoints` |
| **C1-DOCS** (M) | B1 + B2 + B3 + B4 + B5 | `api/documents.rs`, `db/chat_documents.rs`, `documents/mod.rs`, `tools/doc_edit/{file_management,text,document_ui}.rs`; ONE arm in `api/engine.rs` (B1) | `documents_routes_equivalence.rs` + `documents-routes.test.ts`; `chat_documents_tier2_equivalence.rs` + `chat-documents-tier2.ts`; `doc_fs` / `doc_text` / `doc_ui` |
| **C1-STORES** (M) | C1 + C3 + A3(b) (+ C2 if taken) | `api/mount_files.rs`, `db/database_store.rs`, `services/mount_index/refresh.rs` (+ for C2: `file_ops.rs`, `general_state.rs`, `reindex_file.rs`, `store_file.rs`, `link_groups.rs`, `converters.rs`, `reindex.rs`) | `mount_ops_equivalence.rs`, `doc_fs_equivalence.rs`, `realtime_publish_sites_guard.rs` (ONE row, for A3) |
| **C1-BOOT** (S) | D1 + D3 + E1 | `crates/quilltap-host/src/host.rs` (two fenced spots: `:2003-2012` comment, `:2922-2931`), `services/scheduled_maintenance.rs`, `services/mount_index/upload_blob_text_backfill.rs` (doc only) | `upload_blob_text_backfill_equivalence.rs` + its oracle case |
| **C1-FK** (S–M, R2) | F1 | `crates/quilltap-sqlite3mc-sys/build.rs`, `crates/quilltap-cli/src/dbopen.rs` (comment) | `sqlite_build_parity_equivalence.rs`, `restore_vintage_state.rs`, any fixture the gate names |

The five lanes' source files are disjoint. Shared between lanes / areas:

- **`realtime_publish_sites_guard.rs`** — C1-HINTS (eight rows) and C1-STORES (one row, A3).
  Add-only census rows; the unifier unions them (memory note: census counts unioned by
  NUMBER, not text). If the planner wants zero overlap, move A3 into C1-HINTS and give it
  `refresh.rs` (then C2 must not touch `refresh.rs`).
- **`db/database_store.rs`** — C1-STORES (C3); B4's `Failed to find blob by path` pin should be
  a TEST-only addition from C1-DOCS in a harness family, not a source edit there.
- **`api/engine.rs`** — C1-DOCS needs one arm (`ChatAccessibleStores`); every area touches
  this file — resolve by line.
- **`crates/quilltap-host/src/host.rs`** — C1-BOOT's two fenced spots; the Storyteller lane
  owns `storyteller_spine.rs`, not `host.rs`, but any boot-cadence item from another survey
  lands here.
- **`photos/*`, `services/wardrobe_image_bridge.rs`, `tools/generate_image.rs`,
  `services/image_job_storage.rs`** — the wardrobe / image surveys may want these (P4.D377 /
  P4.D386 items, the "in-chat image generation unwired" verification). C1-HINTS's edits are
  one line per function, fenced.
- **`services/file_storage.rs`** — the files / carriers survey (K7, restore) may edit the
  bridges; C1-HINTS adds one line in `store_mount_blob`.
- **`restore_vintage_state.rs`** — C1-FK's one seed fix vs any carrier lane adding cases.
- A1 + A2 should land in ONE change (one census move, one round of union-red triage).
  B3 + B4 + B5 share `documents_routes` / the doc families' regen — one lane.

## Recommended OUT of the round

- **DEFER-TO-CATCH-UP (six):** G1 156.c (`348172b9c` + `3db965416` rewrite
  `apply-chat-merge.ts`), G2 character delete and G3 the `[Characters v1]` lines and H2 the
  closing re-read and B10's `RAW_CHARACTER_ROW_UNVALIDATED` (all on `1dc11e947`'s character
  repository; G2 also `ebb0ca13a`), B8 the folder-sweep casing (`88c9cc81d`).
- **H1 the two store backfills** — wide pre-cutover read, callee rewritten by `1dc11e947`,
  two INFO lines of residue.
- **B10 `HYDRATED_OWNER_ROW_UNVALIDATED`** — the projects / groups row-validation class.
- **D2 the extractor order (H5)** — blocked on a pdf / docx crate ruling.
- **B6 / K12** — recorded both ways; raw-data only.
- **C2** unless C1-STORES has room (each of the 28 sites needs its v4 line measured).
- **G4** belongs to the wardrobe survey's lane.

## Rulings needed

- **R1 (A3):** a filesystem store's hint for in-app document writes — (a) leave under the
  watcher deferral, or (b) publish from `refresh_after_document_write`. Recommend (b).
- **R2 (F1):** converge `SQLITE_DEFAULT_FOREIGN_KEYS` — in (own lane, ~1 m 15 s cold compile
  per profile for the lane and the unifier, fixture migration bounded by a ≤ 5-reds rule),
  pre-setup on main, or out. Recommend in.
- **R3 (D3 / K10):** record `UPLOAD_TEXT_BACKFILL_IN_BOOT` as a named divergence (recommended)
  or move the backfill behind readiness (M, host cadence).
- **(micro, B1):** confirm the convergence direction — v4's SESSION user (`SINGLE_USER_ID`),
  not `chat.userId` as the round record's sentence reads.
