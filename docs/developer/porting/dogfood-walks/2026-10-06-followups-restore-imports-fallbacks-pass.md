# Dogfood walk — 2026-10-06 — the `07b8f0209` follow-ups + restore round

**Round walked:** P4.147 ∥ P4.148 ∥ P4.149 ∥ P4.150 ∥ P4.151 ∥ P4.152
(unified 2026-10-05). Agent-driven, on a COPY of Friday (`~/qt-dogfood-friday`,
rsynced by the human 2026-10-06 ~01:15).

**Build:** main `7dc269c7c` (core 0.0.1234, host 0.0.185, web 0.0.222,
SPA 0.5.809), release build + `npm run build` (the human's, 01:11–01:14).

**Server** (2-hour background limit, per the 2026-10-05 walk rule):

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```

The engine came up unlocked on its own (jobs dispatching within 8 s), so
there was no passphrase step.

## §1 Drift state at walk start

The ledger's §2 probe PASSED: v4 on `main` at `07b8f0209`, tree clean,
`07b8f0209..main`, `..origin/main` (after `git fetch --all`) and
`1a2b2164c..bugfix` all empty. §3 EMPTY, so no step can blame drift.

## §2 Pre-walk measurements (read-only, release CLI, before the first boot)

| # | measured | result | consequence |
|---|---|---|---|
| M1 | `migrations_state` | 201 rows, newest `2026-10-05T04:23:58.768Z`; `--json` dump md5 `08de90fe…` | A1: v5 writes no ledger rows, so the md5 must hold across boots |
| M2 | stores in the mount index | **76** `doc_mount_points` (one `Quilltap Uploads`); `Friday Character Vault` holds **807** links | the restore targets are 76 stores, Friday on the 807-link vault (the order wrote 77/805 from the 2026-10-05 copy; v4 has moved the instance on since) |
| M3 | `doc_mount_points` columns | all 18 incl. `totalSizeBytes`, `conversionStatus`, `conversionError`, `storeType` | the pre-ALTER arm is a PLANT on a clone (drop the four columns) |
| M4 | the Lantern | `chat_settings.storyBackgroundsSettings.enabled: true` | turned OFF on the copy before any test chat (walk rule since 2026-10-02) |
| M5 | known corrupt rows | the boot WARNs `Skipping corrupted chat message` for `f2e0170b…` / `2ab36187…` | already recorded in the 2026-09-29/09-30/10-03 walks; not a finding |

## §3 The walk

Statuses: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.
Plants run on APFS clones (`cp -cR ~/qt-dogfood-friday ~/qt-dogfood-plant-*`),
each booted on its own port, so the main copy stays clean for the restore
rows, which run LAST.

### A — Boot on the fresh copy

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| A1 | CLAUDE | first boot; re-dump `migrations_state` | md5 `08de90fe…` unmoved; `/health` `structure: healthy`; no new WARN/ERROR beyond M5 | **PASS** — after boot the dump md5 is still `08de90fe…`; `/health` 200 `healthy` with `structure: All structural tables verified`; the only WARNs are M5's two corrupt-message skips. The Lantern was then turned off through `chatSettingsUpdate` (DB `enabled:false`) |

### B — Imports (P4.148, #140)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| B1 | CLAUDE | `.qtap` export of a chat, `import-execute` with `duplicate` while the original exists | warnings read `Failed to import message in chat "…": UNIQUE constraint failed: chat_messages.id`, bare, with no `sqlite error:` | **PASS** — `Chat with Vault Test Harness` (`69e2185b…`, 8 messages) exported (9,883 B) and imported `duplicate`: `chats 1, messages 0`, seven warnings each exactly `Failed to import message in chat "Chat with Vault Test Harness": UNIQUE constraint failed: chat_messages.id` (the 2026-10-05 walk's `sqlite error: ` prefix gone — #140 closed live). The messages lost are #139's v4-faithful quirk |
| B2 | CLAUDE | import a group with `color: "red"` | refused with v4's Zod bytes; no group row, no store | **PASS** — Celestial Engineering Team exported, re-keyed, mount pointers dropped, renamed `DOGFOOD 1006 red group`, `color: "red"`: `success: true`, nothing imported, warning `Failed to import group "DOGFOOD 1006 red group": [ { "origin": "string", "code": "invalid_format", "format": "regex", "pattern": "/^#(?:[0-9a-fA-F]{3}){1,2}$/", "path": ["color"], "message": "Invalid string: must match pattern …" } ]` (`JSON.stringify(issues, null, 2)`; the pattern is v4 `lib/schemas/common.types.ts:77`'s `HexColorSchema`); no group row; stores 76 → 76 |
| B3 | CLAUDE | import a project with `color: 5` | refused with v4's Zod bytes; no project row, no half-written store (mount-index count unmoved) | **PASS** — Quilltap Plans re-keyed as `DOGFOOD 1006 five project`, `color: 5`: warning `Failed to import project "…": [ { "expected": "string", "code": "invalid_type", "path": ["color"], "message": "Invalid input: expected string, received number" } ]`; not in `projectList`; stores 76 → 76 (validated before the store write) |
| B4 | CLAUDE | import a project carrying `allowAnyCharacter: null` | imported OPEN (`allowAnyCharacter: true`) | **PASS** — `DOGFOOD 1006 null-open project` (`allowAnyCharacter: null`, empty roster): `projects: 1`, no warnings; `projectList` row `e7a576a8…` `allowAnyCharacter: true`; stores 76 → 77 (its own store) — the unification's blocking-regression fix, live |
| B5 | CLAUDE | `add-character` on a project with a non-uuid `characterId` | v4's 400 Zod envelope | **PASS** (dispatch — v5 has no REST project route; `POST /api/v1/projects/{id}?action=add-character` → 405, as for chat settings in the 2026-10-05 walk) — `projectCharacterAdd {characterId: "not-a-uuid"}` → 400 `Validation error`, `details: [{origin: "string", code: "invalid_format", format: "uuid", pattern: …, path: ["characterId"], message: "Invalid UUID"}]` (zod 4's `z.uuid()` issue) |

### C — The project-detail SPA (P4.152)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| C1 | CLAUDE | project detail: collapse the Characters card, re-expand with the picker open | the picker's search input focused (`document.activeElement`) | **PASS** — the null-open test project (`e7a576a8…`), Allow Any Character switched off (`Only roster characters may use the project files and wardrobe`), "Add character" → focus `INPUT` `Search characters to add`; the card header clicked (real click) → `aria-expanded=false`, focus on the header button; clicked again → `true` and focus back on `Search characters to add` (the `afterRenderEffect`, live) |
| C2 | CLAUDE | posed 400 on each of the nine project-detail save handlers (a fetch interceptor in the page) | each toasts v4's fixed sentence (from `ProjectDetail` handlers at the pin) | **PASS** + **FAIL(#143) → FIXED** — a page-level `fetch` wrapper answered every `projectUpdate` / `projectChatRemove` with 400 `{kind: bad-request, message: "POSED-BOOM"}`. H2–H8 (the seven selects, `form_input` change events) toasted, in order: `Failed to update agent mode setting`, `…answer confirmation setting`, `…default roleplay template`, `…avatar generation setting`, `…default image profile`, `…Lantern image announcement setting`, `…background display mode`; H1 (Edit → rename → Save) `Failed to update project`; H9 (LUC Ranch, a chat's `Remove from project`) `Failed to remove chat` — each v4 `useProjectDetail.ts` / `useProjectChats.ts`'s thrown sentence at the pin, `POSED-BOOM` never shown. **#143:** after each refusal the seven selects kept SHOWING the refused value (DOM `Agent Mode=disabled` etc. while the project still held `enabled`); v4's selects are controlled by `project` and snap back. Fixed in both cards; re-run on LUC Ranch after the rebuild + reload: `Agent Mode=enabled`, `Story Backgrounds=theme` after their refusals |

### D — Repository fallbacks (P4.149) — planted clones

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| D1 | CLAUDE | BLOB in `connection_profiles.name` (clone) | the profile list drops ONE row with v4's two lines; the title job completes; a backup omits the row | **PASS** (the list; the title job and backup not re-run — both pinned in `title_update_tier3` / the backup collect, and the clone was stopped for the #144 rebuild) — clone A, `Claude Desktop` (`86421365…`) `name = X'DEADBEEF'`: `connectionProfileList` 42 of 43, the planted id absent; ERROR `Data validation failed collection="connection_profiles"` then WARN `Safe validation failed`, each `error=` v4's `JSON.stringify(issues, null, 2)` (`"expected": "string", "code": "invalid_type", "path": ["name"], "message": "Invalid input: expected string, received Float32Array"` — P4.130's measured `Float32Array`) |
| D2 | CLAUDE | `chats.userId` renamed (clone) | `listChats` 200 `[]` with the filter line | **PASS** — clone B, `ALTER TABLE chats RENAME COLUMN userId TO userId_x`: `listChats` → 200 `{"type":"chats","data":[]}`, ERROR `Error finding entities by filter collection="chats" error=no such column: userId` (bare). Boot survived (`/health` healthy) with two WARNs — `Failed to scan for incomplete conversations; skipping reconciliation error=sqlite error: no such column: c.userId` (**#147**: v4 logs `err.message`, bare — FIXED) and a job-runner `Job failed … error=sqlite error: …` (v5's own line, v4's is `Job failed in child`) |
| D3 | CLAUDE | a BLOB `title` on one chat (clone) | the list minus one chat with the two validation lines | **FAIL(#144) → FIXED** — clone A, a BLOB `title` planted on `066406d9…`: the two lines fire on `listChats` (`Data validation failed collection="chats" error=Invalid column type Blob at index: 3, name: title` + the WARN — rusqlite's text, the recorded divergence). The count could not prove the drop (the planted chat is `brahma`, which `listChats` excludes anyway — re-planted on a Salon chat after the fix, below). **But every project GET on the clone answered 500 `Failed to fetch project`** (`[Projects v1] Error fetching project … error=Invalid column type Blob…` for LUC Ranch, which does not even hold that chat): `project_chats` reads `chats_read::find_all`, which failed the whole read on one bad row, where v4's `_findAll` runs `validateSafe` per row and drops it → **#144**. **Re-run after the fix** (release rebuilt, core `find_all` through the per-row drop; the BLOB re-planted on Salon chat `0cc44bc8…` in LUC Ranch): `listChats` 942 of 943, the planted chat absent, the two lines per read; LUC Ranch's GET **200** with `_count.chats: 2` (the unplanted copy: 3) |
| D4 | CLAUDE | a project's store corrupted (clone); GET the project; move a chat onto it | `[Projects v1] Error fetching project`; the move answers 503 with v4's body | **PASS** — clone A, Foundry-9's `properties.json` overwritten with `{` (`mountFileWriteRaw`, 1 B): `projectGet` → 500 `Failed to fetch project` with ERROR `[Projects v1] Error fetching project projectId=aade7f41… error=Project … has no usable document store (officialMountPointId=c536c5c8…): properties.json unparseable: EOF while parsing an object at line 1 column 1` (the tail is serde's sentence where v4's is V8's `JSON.parse` message → **#146**, log text only); `chatUpdate {chat: {projectId: Foundry-9}}` on `69e2185b…` → **503** `{error: "Project document store unavailable", projectId: "aade7f41…"}` (v4's body), the chat's `projectId` still NULL. The REST `PUT /api/v1/chats/{id}` is 405 in v5 (dispatch-only, as recorded for other verbs) |
| D5 | CLAUDE | standing inform cancelled under a planted `BEFORE DELETE` trigger (clone) | `removed: 0`, `Error deleting entity` then the wrap's line | **PASS** — clone A, trigger `BEFORE DELETE ON chat_informs … RAISE(ABORT, 'dogfood planted delete failure')`; a standing inform posted to `69e2185b…`'s one LLM seat (batch `bde9d9cb…`), then `chatInformCancel`: 200 `{success: true, removed: 0, recordDeleted: true}`; ERROR `Error deleting entity collection="chat_informs" id=939aa0cf… error=dogfood planted delete failure` then ERROR `Error deleting pending informs by batch collection="chat_informs" batchId=bde9d9cb… error=…` (bare); the row stays (`permanent = 1`); the record message deleted (nothing consumed — v4 `inform.ts:224-237`). The cancel's DEBUG reads `chat_id=… any_consumed=… record_deleted=…` in the FILE log too (`combined.log`), where v4's keys are `chatId`/`anyConsumed`/`recordDeleted` → **#145** |
| D6 | CLAUDE | the same cancel with `batchId` renamed (clone) | the filter line + `count=0` DEBUG | **PASS (v4's real arm)** — clone B, an inform row planted, `batchId` renamed: `chatInformCancel` → **404** `Inform batch not found` after ERROR `Error finding entities by filter collection="chat_informs" error=no such column: batchId`. v4 does the same: `handleCancelInform` reads `findByBatchId` FIRST (a fallback `findByFilter` → `[]`) and answers `notFound('Inform batch')` (`inform.ts:209-212`), so the bulk-delete wrap's `count=0` arm is unreachable through this plant on either side (it is pinned at the repository in `chat_informs_tier2`) |
| D7 | CLAUDE | a malformed `permanent` value on an inform row | moved to F4 — it is a restore-archive plant (P4.147's 💸) | **MOVED → F4** |

### E — Boot / host (P4.150) — planted clones

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| E1 | CLAUDE | `doc_mount_points` planted pre-ALTER (four columns dropped; clone) | four `Migrated doc_mount_points: added … column` lines; `/health` `structure` healthy | **PASS** — clone B, the four columns `DROP`ped from the mount index: boot logged INFO `Migrated doc_mount_points: added totalSizeBytes column`, `… conversionStatus …`, `… conversionError …`, `… storeType …` (no fields, v4's bytes), the table back to its 18 columns, `/health` `structure: All structural tables verified` |
| E2 | CLAUDE | planted duplicate avatar rolls (clone) | the collapse's ONE line with `durationMs` | **PASS** — clone B, v4's ledger row `collapse-duplicate-avatar-rolls-v1` deleted and one more avatar roll unkeyed (one was already unkeyed on the live instance — v4's ledger row was all that held the pass off): the pass ran — v4's seven `Keeping avatar roll still serving as a character portrait` INFOs (bug 143's seven protected portraits), `Collapsing avatar rolls avatarRows=1114 configurations=1102 victims=5`, then ONE `Collapsed duplicate avatar rolls … victimsDeleted=5 protectedKept=7 albumCopiesKept=3 blobsDeleted=2 chatsChanged=1 charactersChanged=3 messagesChanged=5 durationMs=892`; no v5-only host line (P4.150's `log_collapse_ran` deletion holds) |
| E3 | CLAUDE | a posed Google desk answering a parts-less candidate | v4's `No parts found` WARN | **PASS** — a posed Generative Language endpoint (`scratchpad/google-posed.py`, every candidate `{content: {role: "model"}, finishReason: "STOP"}` — no `parts`), the instance's GOOGLE key forwarded only to 127.0.0.1; `connectionProfileTestMessage {provider: GOOGLE, baseUrl: http://127.0.0.1:8899/v1beta, modelName: gemini-posed}` on clone B → one `POST /v1beta/models/gemini-posed:generateContent` (the profile `baseUrl` override reaches the Google transport) and WARN `No parts found in Google response candidate context="GoogleProvider.extractTextFromResponse" modelName=gemini-posed finishReason=STOP`; the answer `Test message successful! Model responded but returned empty content.` (the streaming twin is pinned by the unit tests, not re-posed) |

### F — Restore (P4.147, #141/#142) — LAST, it rewrites the copy

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| F1 | CLAUDE | backup the copy; `replace` restore into the copy | 76 stores from the 76-store archive; Friday on the 807-link vault; LUC Ranch on its archive store; zero `Uploads mount has not been provisioned` warnings | **PASS** — `systemBackupCreate {compact: true}` (42 s; 55 characters / 1,050 chats / 96,846 messages / 77 stores — B4's project added one) → 1,267,949,076 B downloaded; uploaded (`?action=upload`, `-T`) and `systemRestoreExecute {mode: replace}` (9 m 18 s). Summary `docMountPoints: 77`, `docMountFileLinks: 8650`. Against a pre-backup snapshot: the 77 store ids IDENTICAL, all 55 characters' vault pointers identical (Friday on `76fb388c…`, **807** links), all 8 projects' and 4 groups' official pointers identical (LUC Ranch on `50369598…`), `userUploadsMountPointId` the archive's `b5875cd2…`; **zero** `Uploads mount has not been provisioned` lines; `hinge-log.md` and a kept bundle both download (200, 1,382 B / 419,580 B). #141 and #142 closed live. Every link count identical except Quilltap Uploads 62 → 73, and 29 warnings, all v4-faithful → **#148**: (a) the ten kept ARCHIVE bundles (spared by the wipe, `keepArchivedCharacterBundles` default) are re-ingested into Uploads `restored/…` THEN refused `UNIQUE constraint failed: files.id` — ten warnings and ten orphan copies, v4's exact order (`restore.ts:575-605`); (b) 18 `File not found in backup:` — project files whose rows still carry a legacy `<projectId>/<name>` disk key while their bytes live only in the project store; v5 AND v4 (`manager.ts:379-399`) 404 them on the source already, so the backup has nothing to collect; (c) the compact-backup notice. Also v4-shaped: 19 `Seeded connection-profile columns the archive predates` DEBUGs (an omitted null `fallbackProfileId` seeded from the pre-restore row, v4 bug 103) |
| F2 | CLAUDE | the same archive restored into a FRESHLY provisioned instance | the ten character-archive bundles + `hinge-log.md` in the archive's Uploads store; zero Uploads warnings | **PASS** — a fresh instance (`~/qt-dogfood-fresh`, `setup` with no passphrase as Friday has none; it minted its OWN Uploads `f1d9f366…`), the PLANTED archive (F3 + F4's plants) uploaded and `replace`-restored: `userUploadsMountPointId` → the archive's `b5875cd2…`; that store holds 73 links incl. the ten `restored/…character-archive.qtap` and `restored/hinge-log.md`; the bundle rows' keys `mount-blob:b5875cd2…:…`; three sampled files download (200: 1,382 / 419,580 / 530,056 B); **zero** Uploads warnings; `files: 2337` (2,355 less F1's 18 dangling rows). **⚠ It took 2 h 26 m** (06:55 → 09:21, the message phase slowing steadily) where F1 took 9 m — the v5-fresh schema lacks 57 indexes the migrated copy carries → **#149** |
| F3 | CLAUDE | a restored project keeping its `icon` on the fallback arm | icon present after restore | **PASS** — the archive's store for `DOGFOOD 1006 null-open project` removed (so the fallback arm runs) and its row given `icon: "🧭"`, `color: "#123456"`: restored onto a freshly minted store `09d864b6…` (77 = the archive's 76 + that one) with `icon: 🧭`, `color: #123456`, `allowAnyCharacter: false` (C1's toggle) read back through `projectGet`; the store's four links reported `Skipped doc-store file link … its document store is not in the backup` (and its folder + project↔store link) — the plant's own consequence |
| F4 | CLAUDE | an archive whose `chat-informs.json` carries a malformed `permanent` | the row skipped with v4's bytes + WARN; the rest restored | **PASS** — two rows planted (`permanent: "true"`, `permanent: null`): warnings `Failed to restore inform: [ { "expected": "boolean", "code": "invalid_type", "path": ["permanent"], "message": "Invalid input: expected boolean, received string" } ]` and `… received null` (v4's `JSON.stringify(issues, null, 2)`); WARN `Failed to restore chat inform informId=…` per row; DEBUG `Restored chat informs total=35 restored=33`; neither id present after |

## §4 What NOT to expect

- v5 writes no `migrations_state` rows (the deferred runner).
- Real PDF text extraction (the refusing `DocumentTextExtractor`, recorded).
- The smalls round's OPEN items (phase-4.md NEXT 2): the preserve-arm
  completeness guard (a vault missing a managed file restores that field
  blank), the import's whole-entity `_create` validation (name 1–100 etc.),
  the chat-informs reads that still propagate.
- WaveSpeed (ruled unsupported).

## §5 Findings

**Two v5 defects found and FIXED in place, one high-severity provisioning gap
ORDER-PENDING, three log-text divergences, one v4-faithful note.**

- **#143 — FIXED** (`b6a52aeb3`, SPA 0.5.810): a refused project-detail select
  kept showing the refused value (C2).
- **#144 — FIXED** (`28b056b8f`, core 0.0.1235): one unreadable chat row made
  every project GET answer 500 — `chats_read::find_all` now drops the row as
  v4's `_findAll` does (D3).
- **#147 — FIXED** (`28b056b8f`): the render-reconcile scan WARN's
  `sqlite error: ` prefix (D2).
- **#149 — ORDER-PENDING (high):** a v5-provisioned instance carries NONE of
  the 57 secondary indexes (46 main, incl. four UNIQUE ones; 6 mount-index; 5
  llm-logs) a v4-migrated instance has — `idx_chat_messages_chatId` among them —
  so every per-chat read full-scans; F2's restore into a fresh instance took
  2 h 26 m against F1's 9 m. Measurement owed FIRST: does a real v4 first boot
  (migrations before anything else) create them?
- **#145 / #146 — ORDER-PENDING (log text):** the `[Chats v1]` inform lines'
  snake_case keys; the overlay's `unparseable` detail in serde's words.
- **#148 — v4-faithful (RECORDED):** F1's 28 file warnings (the kept ARCHIVE
  bundles re-ingested then refused; 18 dangling legacy project-file keys).

Instrument notes (not findings): `recipe_sweep.py` must run with
`--v5w ~/source/quilltap-v5` (the real path's space breaks every regen); in
zsh `set -- $x` does not word-split, so a launch loop over a list silently
starts nothing — launch each server as its own background command. The REST
`PUT /api/v1/chats/{id}`, `POST /api/v1/projects/{id}?action=…` and the chat
settings PUT are 405 in v5 (dispatch-only), as the 2026-10-05 walk recorded.
