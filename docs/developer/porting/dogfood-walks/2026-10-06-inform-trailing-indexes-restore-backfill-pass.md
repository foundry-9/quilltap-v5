# Dogfood walk — 2026-10-06 (evening) — the `94fbb1ae3` fresh-instance-indexes + follow-ups smalls round

**Round walked:** P4.D254 ∥ P4.153 ∥ P4.154 ∥ P4.155 ∥ P4.156 ∥ P4.157 ∥
P4.158 (unified 2026-10-06). Agent-driven, on a COPY of Friday
(`~/qt-dogfood-friday`, rsynced by the human 2026-10-06 ~17:10 CDT).

**Build:** main `22e0fbc0f` (core 0.0.1242, host 0.0.187, web 0.0.222,
SPA 0.5.811), release build + `npm run build` (the human's, ~17:13–17:15).

**Server** (2-hour background limit, per the 2026-10-05 walk rule):

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```

## §1 Drift state at walk start

The ledger's §2 probe PASSED: v4 on `main` at `94fbb1ae3`, tree clean,
`94fbb1ae3..main`, `..origin/main` (after `git fetch --all`) and
`1a2b2164c..bugfix` all empty. §3 EMPTY, so no step can blame drift.

## §2 Pre-walk measurements (read-only, release CLI, before the first boot)

| # | measured | result | consequence |
|---|---|---|---|
| M1 | `migrations_state` | 201 rows; `--json` dump md5 `08de90fe…` (identical to the morning walk — v4 ran no migration since) | A1: v5 writes no ledger rows, the md5 must hold |
| M2 | secondary indexes (`sql IS NOT NULL`) | main **98**, mount-index **26**, llm-logs **7** | A1: P4.153 replays only at PROVISIONING — an existing instance's counts must not move |
| M3 | standing informs | **0** of 36 `chat_informs` rows have `permanent = 1` — the motivating standing inform no longer exists on the copy. The newest inform (13:30Z, *The Dull Gold Run* → Abigail, "Charlie is planning to replace the metal throughout your hull with Tessarium…") is a consumed one-shot | B1 RE-POSTS that passage as a standing inform ourselves |
| M4 | the Lantern | `storyBackgroundsSettings.enabled = 1`; `impersonationVoiceMode = 'ask'` | Lantern turned OFF on the copy before any test chat (walk rule since 2026-10-02) |
| M5 | stores | 76 `doc_mount_points` | the restore baseline |

## §3 The walk

Statuses: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.
Plants run on APFS clones (`cp -cR ~/qt-dogfood-friday ~/qt-dogfood-plant-*`),
each booted on its own port, so the main copy stays clean. The restore rows
run LAST and run from a PLANTED clone's backup.

### A — Boot on the fresh copy

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| A1 | CLAUDE | first boot; re-measure M1 + M2 | md5 `08de90fe…` unmoved; index counts 98/26/7 unmoved (no index created on a migrated instance); `/health` `structure` healthy; no new WARN/ERROR beyond the known corrupt-message skips | **PASS** — boot 22:19Z; ledger dump md5 still `08de90fe…`; indexes still 98/26/7 (P4.153's replay is provisioning-only — nothing created on a migrated instance); `/health` 200 `healthy`, `structure: All structural tables verified`; ZERO WARN/ERROR lines in the boot log. The Lantern then turned off through `chatSettingsUpdate` (DB `enabled = 0`) |

### B — The inform as a trailing section (P4.D254, `94fbb1ae3`'s motivating case)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| B1 | CLAUDE | *The Dull Gold Run*: a STANDING inform to Abigail with M3's Tessarium passage (via the Salon's Inform dialog, "Keep it standing" checked); then a real Abigail turn | the llm-logs request: `INFORM_BLOCK_HEADER` + the passage as the LAST trailing section (after recall/mail/progressions); NOT in the system prefix; the reply treats the plan as known fact (read it) | **PASS** — posted through the Salon's Inform dialog (Abigail only, "Keep it standing" checked): row `bc1e2f50…` `permanent = 1`, the chip `Informing Abigail on every turn in this chat`. Gary then asked Abigail what Charlie has planned for her hull; her turn's `CHAT_MESSAGE` request (`701b8681…`, 96 messages) carries `Things you now know, as of this moment — true in this story, and already known to you. Where any of it conflicts with an older memory or something in your records, this is the current truth:` + the passage in the FINAL user message, after the recall block and the progressions block (`You are carrying Charlie's daughter…`), before the Salon's turn-skip note (v4's order); the system message does NOT carry it. The reply: *"…he didn't ask, he told me: all of me, not a patch. Tessarium, because it holds a finish and it's stronger than what I have on."* — the same history carries v4's PRE-fix reply from this morning (*"Nobody's told me that, and I'd know… It's not in my hull."*), so the before/after sits in one transcript |
| B2 | CLAUDE | the same turn's `combined.log` | `[Inform] Delivering …` with camelCase keys and `rowIds` as an ARRAY | **PASS** — `combined.log`: `{"message":"[Inform] Delivering inform block as a trailing context section","context":{…,"chatId":…,"participantId":…,"onNewUserMessage":true,"rowIds":["bc1e2f50-…"]}}` (stdout shows the `rowIdsJson` field name; the file layer applies the `…Json` convention) |
| B3 | CLAUDE | a chained multi-character turn (nudge / continue on the same 4-seat chat) carrying only the inform | the logged request carries a trailing user message holding the inform block | **PASS** — `chatSend {continueMode: true, respondingParticipantId: Abigail, nudge: true}` (the SPA's nudge call): `[Inform] Delivering … onNewUserMessage=false rowIds=[]` (the standing row already consumed on its first delivery; `Built inform block … standing=1`); the request's LAST message is a user-role trailing-only message holding the scene note then the header + passage (char 257); the system message does not carry it |
| B4 | CLAUDE | cancel the standing inform | `Inform cancelled` DEBUG with `chatId=… batchId=… removed=… anyConsumed=… permanent=… recordDeleted=…` (#145) | **PASS** — the chip's × (`Withdraw the inform for Abigail`): `combined.log` `[Chats v1] Inform cancelled` `{chatId, batchId, removed: 1, anyConsumed: true, permanent: true, recordDeleted: false}` — camelCase (#145 closed live); the row gone, the record message kept (consumed). Instrument note: under an emulated 1400×900 viewport the pane's clicks landed offset (a capture listener saw (641,703) for a (574,629) click) — reset to the native size first |

### C — Repository fallbacks + the V8 twin + import refusals (P4.156, P4.154, P4.155) — planted clones

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| C1 | CLAUDE | clone: `chat_informs.chatId` renamed; list informs | 200 `[]`; ERROR `Error finding entities by filter`; NO `Error listing informs` | **PASS** — clone A (`chat_informs.chatId` → `chatId_x`, `--write` before boot; boot healthy): `chatInformsList` on *The Dull Gold Run* → 200 `{batches: []}`; ERROR `Error finding entities by filter collection="chat_informs" error=no such column: chatId` (bare), then DEBUG `Pending informs listed … batches=0 dropped=0`; no `Error listing informs` |
| C2 | CLAUDE | clone: `chats.id` renamed; inform POST + GET | 404 each; v4's `Error finding entity by ID` | **PASS** — a fresh clone with `chats.id` → `id_x` (`--write` before boot) still boots `healthy`; `chatInform` and `chatInformsList` on *The Dull Gold Run* each → 404 `Chat not found`, each after ERROR `Error finding entity by ID collection="chats" id=0c6cd0f0… error=no such column: id` (bare) |
| C3 | CLAUDE | clone: mount-index unreadable after boot; move a chat onto a project | 503 `Project document store unavailable`; four DEBUG `Dedicated database unavailable` lines | **BLOCKED(#150)** — clone B with its mount-index file overwritten by 4.6 KB of garbage: v5 REFUSES TO BOOT (`Startup failed: the engine did not assemble error=database open failed: sqlite error: file is not a database`, `/health` 503 `startupPhase: failed`). v4 retries the open three times (200/600/1500 ms backoff, a WARN each) and then logs `Failed to initialize mount index database — entering degraded mode` and serves on without the partition (`mount-index-client.ts:102-149`; the same for llm-logs, and for a failed integrity check in `*-protection.ts`). v5's `Db::open` propagates the sibling writer's open error, so `PartitionUnavailable` — the state this row exercises — is reachable only for an instance with no sibling path at all |
| C4 | CLAUDE | clone: Foundry-9's `properties.json` → `{`; GET the project (read path) and edit it (write path) | both WARN/ERROR tails read V8's `Expected property name or '}' in JSON at position 1 (line 1 column 2)` (#146) | **PASS** (read) + **#151** (write-path log) — clone A, `mountFileWriteRaw` `{` over Foundry-9's `properties.json`: `projectGet` → 500 `Failed to fetch project`, ERROR `[Projects v1] Error fetching project projectId=aade7f41… error=Project … has no usable document store (officialMountPointId=c536c5c8…): properties.json unparseable: Expected property name or '}' in JSON at position 1 (line 1 column 2)` — V8's sentence (#146 closed live; the morning walk read serde's `EOF while parsing…`). `projectUpdate` → 503 `{error: "Project document store unavailable", projectId}` (v4's body) — but with NO log line. v4's PUT reaches the same throw through `findById`'s hydrate (silent, `document-store-overlay.ts:158-166`) and its context middleware logs ERROR `[PUT /api/v1/projects/…] Project document store unavailable {projectId, officialMountPointId}` (`lib/api/middleware/context.ts:176-205`, and its group / character-vault twins). P4.23 ported the 503 envelope but not the three lines → **#151**. The "write path" V8 line (`read_properties`' `unparseable — refusing to treat as absent`) is unreachable through the route on either side: the hydrate refuses first |
| C5 | CLAUDE | a `.qtap` import carrying a 101-character project name | refused with v4's ZodError bytes; THREE ERRORs (`Data validation failed`, `Error creating project entity`, `Error creating project name=…`) before the WARN; no store created | **PASS** — clone A, Quilltap Plans' export re-keyed with a 101-`D` name and no store pointer, `import-execute` (multipart, `duplicate`): `success: true`, `projects: 0`, warning `Failed to import project "DDDD…": [ { "origin": "string", "code": "too_big", "maximum": 100, "inclusive": true, "path": ["name"], "message": "Too big: expected string to have <=100 characters" } ]`; log, in order: ERROR `Data validation failed collection="projects"`, ERROR `Error creating project entity`, ERROR `Error creating project … name=DDDD…`, then WARN `Failed to import project projectId=761c896b…`; mount-index stores 76 → 76 |
| C6 | CLAUDE | a `.qtap` import with an id-less memory and an id-less file item | the WARNs carry no `memoryId=` / `fileId=` | **FAIL(#152)** — no import failure could be posed through a memory: Duane Red Cloud's export with one memory stripped of its `id` and given `importance: 5`, `kind: "bogus-kind"` imported CLEANLY (`characters: 1, memories: 1`, no warning) and the row persisted (`ae6ecc10…`, `importance 5`, `kind bogus-kind`). v4's `memories.create` → `_create` validates against `MemorySchema` (`importance: z.number().min(0).max(1)`, `kind: MemoryKindEnum`, `lib/schemas/memory.types.ts:54-108`) and the import's catch WARNs `Failed to import memory` and skips (`import-entities.ts:518`) → **#152**. (A plain memory/file line in a `projects` export is silently not imported on either side — no character to map.) The id-less rendering itself stays unit-pinned (`an_id_less_item_omits_the_id_field`) |
| C7 | CLAUDE | a posed Gemini image endpoint answering only `promptFeedback.blockReason` | `blockReason=SAFETY`, no `finishReason=`; refusal classified | **BLOCKED(no seam)** — the Gemini image dialect hardcodes `https://generativelanguage.googleapis.com/v1beta` (`model/image_dialects.rs:585`; v4 likewise has no image base-URL override), so no posed endpoint can answer it short of intercepting the network. Pinned at the unit level (`image_dialects.rs:2594` and the corpus) |
| C8 | CLAUDE | a posed Google stream answering `content.text` (no parts) | the text delivered + `No parts found…` WARN | **PASS** — `harness`-style posed server (`scratchpad/google-posed.py`, every stream frame `candidates[0].content = {role: "model", text: "Posed reply carried on content.text, with no parts at all."}`), a GOOGLE profile on clone A with `baseUrl http://127.0.0.1:8899/v1beta`. With model `gemini-posed` the turn came back EMPTY (the empty-response ladder ran) — correct: the decoder's terminal fallback is gated on a THINKING model (v4 `provider.ts:847-850`; `is_thinking_model`). Re-pointed to `gemini-2.5-flash`: WARN `No parts found in Google response candidate context="GoogleProvider.extractTextFromResponse" modelName=gemini-2.5-flash finishReason=STOP` and the ASSISTANT row reads exactly the posed text. (The test chat had no user seat, so it ran as an all-LLM room and chained to v4's `maxChainDepth` 20 — v4's `isMultiCharacterChat` / `turn-orchestrator.service.ts:86`, faithful; the plant's construction, not a finding) |

### D — Restore + fresh instance (P4.158, P4.153) — LAST

Plants on a clone (`~/qt-dogfood-plant-restore`) BEFORE its backup: one
managed file removed from one character vault; a prompt RENAMED and a
heading-titled scenario in another vault; a third character's vault pointer
set to a second character's vault.

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| D1 | CLAUDE | backup the planted clone; `replace`-restore into the same clone | one `Backfilled a managed file …` WARN, the character reading the archived value, the vault's other files kept; the renamed prompt + heading scenario NOT duplicated (Prompts/Scenarios counts unchanged); the shared-vault character on a fresh vault with `Archived store already claimed …`; the `Starting` / `Restore operation completed` census pair | **PASS** — the plants, refined: Owen's vault pointer set to Gary's (`--write` before boot); on the running clone Xavier's `Prompts/Main.md` moved to `Prompts/The Usual.md` (frontmatter `name: Main` kept) and `Scenarios/Default.md` to `Scenarios/opening-scene.md` (`# Default` heading kept). Duane's managed file had to be removed from the ARCHIVE, not the vault — his `personality` lives only in the vault, so a pre-backup delete blanks the archived row too and the backfill would "restore" `''`; so the backup (`systemBackupCreate {compact: true}`, 43 s, 1,279,538,420 B) had the one `doc-mount-file-links.json` entry for `fc3a5d08…/personality.md` stripped (the row keeps the value). `replace`-restored into the clone: **8 m 29 s** (22:36:18 → 22:44:47Z). Results: WARN `Archived store already claimed by an earlier entity; falling back to a fresh store entity="character" entityId=d7cab8f8… (Owen) mountPointId=383398cc… claimedBy=e06c5f31… (Gary)` — Owen on fresh vault `07b2e033…`, Gary kept `383398cc…`; WARN `Backfilled a managed file the archived store was missing entity="character" entityId=bac855f9… mountPointId=fc3a5d08… relativePath=personality.md` — Duane's `personality` reads the archived text and his vault holds 26 links (his 25 + the backfill); Xavier's vault 19 links, still `Prompts/The Usual.md` + `Scenarios/opening-scene.md` with NO `Prompts/Main.md` / `Scenarios/Default.md` written beside them, `systemPrompts` `['Main']`, scenarios `['Default']` (the unification's parsed-name fix, live). Three MORE `Backfilled … relativePath=manifesto.md` WARNs fired on real data — Lt. Supertramp, Ygraine, Uther, whose live vaults genuinely lack `manifesto.md` (archived `manifesto: null`): the backfill writes `''`, exactly v4's projection (`managed-fields.ts:271` `character.manifesto ?? ''`), and the read stays `null` (`markdownToNullable`) — correct, not a finding. Census: INFO `Starting restore operation mode="replace"` … INFO `Restore operation completed … warningCount=29`; the 29 are the morning's #148 class (`Failed to restore file … UNIQUE constraint failed: files.id` for the kept archive bundles, etc.). Not compared against a v4 log of the same restore (no v4 run here) |
| D2 | CLAUDE | a freshly set-up v5 instance (`~/qt-dogfood-fresh`): measure its index counts BEFORE restoring | main/mount/llm-logs include the migration family (`migration_indexes.json`: 50/5/5 + generateDDL's); `idx_doc_mount_folders_mp_path` UNIQUE | **PASS** — `~/qt-dogfood-fresh` booted empty on :3004, `unlockState` `needs-setup`, `setup {passphrase: ""}` (the minted pepper redacted from the transcript): secondary indexes **main 99 / mount-index 24 / llm-logs 7** against the migrated copy's 98 / 26 / 7 (the morning walk's fresh instance had NONE of the 57). Name differences, all instance history: copy-only `idx_wardrobe_items_createdAt` (no longer in v4's `lib/`), `idx_doc_mount_folders_mp` and `idx_project_doc_mount_links_proj_mp` (the row's predicted history-only pair); fresh-only `idx_terminal_sessions_chatId` / `_startedAt` (in `migration_indexes.json` — Friday's table predates the migration that adds them). `idx_doc_mount_folders_mp_path` is `CREATE UNIQUE INDEX` on the fresh instance, plain on the copy (the ruled shape). Every other shared name differs only in `ASC` / quoting. UNIQUE set: main identical (3); mount-index fresh adds `idx_doc_mount_folders_mp_path`, lacks the history-only `idx_project_doc_mount_links_proj_mp` |
| D3 | CLAUDE | a duplicate connection-profile name on the fresh instance | refused (the UNIQUE index) | **PASS** — `connectionProfileCreate {name: "Dup Name"}` twice: the second → 409 `A connection profile named "Dup Name" already exists`, one row; `idx_connection_profiles_userId_name` is UNIQUE on the fresh instance as on the copy |
| D4 | CLAUDE | `replace`-restore D1's archive into the fresh instance — TIME it | far under 2 h 26 m (target: the same order as the 9 m restore into the migrated copy) | **PASS** — D1's planted archive (1,279,534,131 B; 55 characters / 1,052 chats / 97,339 messages / 36,024 memories / 8,506 llm logs) uploaded and `replace`-restored into the D2 instance: **7 m 17 s** (22:47:41 → 22:54:58Z) — against **2 h 26 m** for the same-size archive into the morning's index-less fresh instance, and FASTER than the 8 m 29 s into the migrated clone (no kept bundles to re-ingest: `warningCount=19` vs 29). 75,279 messages were in after ~4.5 min (the morning's run slowed steadily). The same plant outcomes: Owen's claim WARN, Duane's `personality` backfilled and readable. **#149 closed live** |
| D5 | CLAUDE | `sqlite_master` index names: restored fresh instance vs migrated copy | differences only Friday's history-only indexes (e.g. `idx_project_doc_mount_links_proj_mp`); fresh-instance's first REBOOT creates no index | **PASS** — after the restore the fresh instance still holds 99 / 24 / 7 (the restore drops and creates no index), so D2's comparison against the migrated copy stands as measured (history-only names, the ruled UNIQUE). Rebooted (SIGTERM, stale lock reclaimed silently): `/health` `healthy`, `structure: All structural tables verified`, FTS `eligible=86216 indexed=86216`, ZERO WARN/ERROR, and each partition's `sqlite_master` index dump byte-identical (`cmp`) to the post-restore snapshot |
| D6 | CLAUDE | a legacy shared outfit preset restored into the fresh instance | lands in the restored General | **PASS** — Friday has no legacy rows (`wardrobe_items` is EMPTY — wardrobe lives in the vaults), so on two throwaway instances (`~/qt-dogfood-tiny` / `-tiny2`, each `setup` fresh; Generals `09ef217d…` / `b0120ae0…`): tiny's compact backup (107 KB) gained a `data/wardrobe-items.json` with ONE shared row (`characterId: null`, "Dogfood Shared Duster") and was `replace`-restored into tiny2: `success`, `wardrobeItems: 1`, no warning; tiny2's `generalMountPointId` → the archive's `09ef217d…`, and the item's link is `09ef217d… / Wardrobe/Dogfood Shared Duster.md` — the restored General, not tiny2's wiped own (P4.158 R-C's pre-applied pointer, live; v4 files it in the wiped store) |

### E — The standing queue

| # | owner | gesture | status |
|---|---|---|---|
| E1 | HUMAN | the Lantern per-turn budget (needs generated portraits = image spend) | DEFERRED-TO-HUMAN |
| E2 | HUMAN | a real token-limit turn; the four planted proofs; dedup/summaries; the Brahma deep query; #101; the compression re-measure | DEFERRED-TO-HUMAN |

## §4 What NOT to expect

- v5 writes no `migrations_state` rows (the deferred runner).
- Instances provisioned BEFORE this round have NO migration-index backfill
  (phase-4.md NEXT 2(a), its own order) — only a fresh `setup` gets them.
- §S.2's whole-row inform validator on restore (deferred by name); the three
  shape-only `is_uuid` gates; P4.155's R-B whole-entity validation on restore.
- Real PDF text extraction (the refusing `DocumentTextExtractor`, recorded).
- REST `PUT /api/v1/chats/{id}`, `POST /api/v1/projects/{id}?action=…` and the
  chat-settings PUT are 405 (dispatch-only), as recorded.
- WaveSpeed (ruled unsupported).

## §5 Findings

**19 CLAUDE rows run: 15 PASS, 1 PASS with a log-text finding, 1 FAIL, 2 BLOCKED;
the two HUMAN standing-queue rows DEFERRED-TO-HUMAN. ZERO defects in the round's own surfaces;
three pre-existing port divergences found, all ORDER-PENDING (none fixable in
place).**

- **#150 — ORDER-PENDING (high):** a sibling database (mount-index / llm-logs)
  that cannot be opened stops v5's boot outright; v4 retries three times and
  boots DEGRADED without the partition (C3).
- **#151 — ORDER-PENDING (smalls):** v4's context-middleware ERROR for a
  store-unavailable 503 (`[PUT …] Project document store unavailable`, and the
  group / character-vault twins) is never logged by v5 (C4).
- **#152 — ORDER-PENDING:** a `.qtap` import writes a memory v4's
  `MemorySchema` refuses (`importance: 5`, an unknown `kind`) — P4.155's
  whole-entity validation covered projects and groups only (C6).

⭐ Headline proofs: `94fbb1ae3`'s own motivating case flipped on one transcript
(v4's morning reply "Nobody's told me that… It's not in my hull" → v5's
"Tessarium, because it holds a finish and it's stronger than what I have on");
the fresh-instance restore **7 m 17 s** against 2 h 26 m (#149 closed live);
the restore backfill, the claim fallback and the parsed-name matching all on a
real 1.28 GB archive; #145 and #146 closed live.

Instrument notes (not findings): an emulated 1400×900 viewport offsets the
pane's clicks (~1.12×) — measure with a capture listener or stay at the native
size; a Gemini stream's `content.text` fallback is gated on a THINKING model
name; a chat created with no user seat is an all-LLM room and chains to depth
20 (v4-faithful); a vault-only managed field must be removed from the ARCHIVE,
not the vault, to plant the backfill.
