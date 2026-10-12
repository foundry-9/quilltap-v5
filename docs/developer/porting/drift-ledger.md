# The v4 drift ledger

**The single standard record of where v4 stands relative to the oracle
baseline, what each drift commit touches, where it intersects work this port
has already landed, and what has been done about it.** Written by
`/driftcheck` (and by `/unify` when a round moves the baseline); read by
`/setupphase`, `/carryout`, `/unify`, and `/dogfood`. Those commands do NOT
re-derive drift — they run the §2 freshness probe and then trust this file.

Ownership: this file is written only from a main-checkout session
(`/driftcheck`, `/setupphase` marking rows ORDERED, `/unify` marking rows
ABSORBED, `/dogfood` when a walk changes a row's facts). **Lane agents never
write it** — a lane that finds the probe failing STOPs and reports instead.

---

## §1 Current state

_Updated only by `/driftcheck` and `/unify`. Every field here is what the §2
probe verifies against._

- **Oracle baseline: `01a83539d`**: "feat(wardrobe): full-screen picture
  viewer with save to a document store" (v4 main, 2026-10-09 14:44,
  `4.10.0-dev.143`), adopted when the `01a83539d` memory-programme +
  bugs-183–186 + wardrobe-viewer drift catch-up round was unified, all
  fourteen lanes (P4.D265 → {P4.D266 ∥ P4.D267 ∥ P4.D268 ∥ P4.D269 ∥ P4.D270 ∥
  P4.D271 ∥ P4.D272 ∥ P4.D273 ∥ P4.D274 ∥ P4.D275} ∥ P4.D276 ∥ P4.D277 ∥
  P4.D278, 2026-10-12). The thirteen rows it absorbed (`1825bfd53` …
  `01a83539d`) are retired to §6. CLAUDE.md's Status bullet agrees (the
  round's bullet: "the oracle baseline MOVES to `01a83539d`"; its "Oracle
  baseline" bullet names it). UNCHANGED by this check — a `/driftcheck`
  never moves it.
- **Checked:** 2026-10-12 evening (`/driftcheck` — the §2 probe FAILED
  against the close probe's tips: after `git fetch --all` from the main
  checkout at `e7e9f2cc8`, `545e299bb..main` and `69d38dd60..origin/main`
  each listed new commits. The human has since PULLED: local `main` and
  `origin/main` now AGREE. Three commits past `69d38dd60` classified from
  their shipped hunks — `8050958be`, `97f0d8cb8`, `4ce64fa1e` — and the
  eleven earlier rows confirmed UNPROCESSED, unchanged; three rows
  appended). Previous checks: 2026-10-12 (`/driftcheck` — the unification's
  close probe, seven rows appended); 2026-10-12 (`/unify` of the
  `01a83539d` round); 2026-10-12 morning (`/driftcheck` — the four
  wardrobe rows classified); 2026-10-09 late afternoon (`/driftcheck` —
  `01a83539d` classified); 2026-10-09 afternoon; 2026-10-09 morning (probe
  PASSED); 2026-10-08 (`/unify` of the `f5e953a3f` round + the evening
  `/driftcheck`).
- **v4 `main` HEAD at check: `4ce64fa1e`** ("Merge origin/main; renumber
  the folder-picker bug to 229", 2026-10-12 14:17, `4.10.0-dev.155`) — local
  `main` and `origin/main` are the SAME commit. **FOURTEEN commits past the
  baseline** (`01a83539d..main`): the eleven already in §3, plus
  - `8050958be` ("Fix bug 228: importing a chat with "Create New" dropped
    every message (#89)", 14:12, `.154`, parent `69d38dd60`) — a `.qtap`
    import fix in `lib/import/quilltap-import/`;
  - `97f0d8cb8` ("fix(wardrobe): a click inside the folder picker no longer
    closes the wardrobe dialog (bug 199)", 14:15, `.150`, parent
    `545e299bb`) — the in-flight dirt the close probe recorded, now
    committed (SPA-only: `components/wardrobe/` + its jsdom test + docs);
  - `4ce64fa1e` — the merge of the two lines; its OWN resolution (the
    `--cc` hunks) is the README badge, the version stamp to `.155`, and the
    picker fix RENUMBERED 199 → **229** (file renamed to
    `bugs/fixed/bug-229-folder-picker-closes-dialog.md`, two comments and
    one test title) — the bug-number collision the close probe predicted,
    resolved in v4's favour of the memory audit's 199. The merge adds no
    code of its own: `git diff 8050958be 4ce64fa1e` is exactly
    `97f0d8cb8`'s content with the renumber.
  - **CONVERGENCE check: NONE.** Bug 228 reads `Provenance: Original to v4`
    (found live on `V4test`), `v5 status: Not assessed`; bug 229 is v4's
    own SPA defect on a surface (wardrobe folders) v5 has not ported. The
    port's filings (180, 183, 184, 185) are all absorbed by this baseline.
- **v4 `bugfix` tip at check:** `1a2b2164c`, UNMOVED (`1a2b2164c..bugfix`
  and `..origin/bugfix` empty). **`release` tip:** `8fbf2afe0` (`release:
  4.9.2`), UNMOVED; still no `release: 4.10.0` squash.
- **Checkout at check:** branch **`main`**, tree **CLEAN** (`git status
  --short` empty) — the six-path picker dirt the close probe recorded is
  now commit `97f0d8cb8`. The §2 probe now reads `4ce64fa1e..main` (one
  tip, both refs).
- **Verdict: DRIFT PENDING — 14 commits (§3): 5 NO-PORT? (docs only: the
  wardrobe refactor plan `3e6e2f590`; the two feature specs `0841000c7`; the
  memory refactor spec `c19e5d0a9` + its audit appendices `0fc8ce709`; and
  the merge `4ce64fa1e`, whose own resolution is a renumber + stamps), 7
  PORT (`7a8d6dacc`, `9ecebf986` [+ PORT-NEW], `e23fb095b`, `124c8aca3`,
  `69d38dd60`, `8050958be` bug 228, `97f0d8cb8` bug 229 [SPA, rides
  `4a38222e6`]), 2 PORT-NEW + PORT (`4a38222e6` wardrobe folders,
  `545e299bb` scenarios from anywhere).** The eleven earlier rows stay
  UNPROCESSED, unchanged; the three new rows are UNPROCESSED. In brief, from
  the hunks (the §3 rows carry the full classification):
  - `8050958be` — **bug 228** (20 files, +910 / −69; `lib/` hunks only in
    `lib/import/quilltap-import/`): a chat imported under a FRESH id (the
    `duplicate` / "Create New" strategy, or any import without
    `preserveIds`) now gets fresh MESSAGE ids through NEW
    `message-id-remap.ts` (`remintChatMessageIds`, `remapImportedMessageId`)
    — `chat_messages.id` is global, so every insert beside the original
    failed and the copy landed empty with one `UNIQUE constraint failed`
    warning per message. The chat's own references follow the per-chat map
    (`swipeGroupId` in its `swipe-<messageId>` form only,
    `summaryAnchorMessageIds` — an unresolvable anchor clears the summary,
    the anchors and `lastSummaryTurn` together, `otherExtractionWatermark
    MessageId`, `courierCheckpoints[*].lastResolvedMessageId` incl. the
    JSON-string form, unreadable ones dropped); NEW `IdMappingState.
    messages` (source chat → source msg → new id, written only after the
    insert lands) carries it to inform `recordMessageId` /
    `consumedByMessageId` (`remapChatInform` — the refusal reason keeps the
    SOURCE id), annotation `sourceMessageId`, memory `sourceMessageId` and
    file `linkedTo` (`remapLinkedTo` resolves a message only through a chat
    named in the same array). **The duplicate branch's phantom `idMaps.
    chats` entry is GONE** — it now records the real created id, and the
    duplicate and fresh-id create paths are one. `help/system-import-
    export.md` +1 line. **v5 carries v4's PRE-fix shape exactly**:
    `services/quilltap_import/entities.rs:791` `import_chats` adds every
    message under its bundle id and records the ruled "Phantom-map quirk"
    (`entities.rs:829-834`, the module header) — so v5 SHARES bug 228, and
    the phantom pins flip by design with the port.
  - `97f0d8cb8` + `4ce64fa1e` — **bug 229** (SPA-only, 10 files, +167 /
    −26): the Move-to-folder picker's state moves from `WardrobeBrowser`
    into `useWardrobeItemActions` (`actions.folderPicker`) so the dialog's
    `overlayOpen` counts it; a mousedown inside the portalled picker had
    closed the whole wardrobe dialog. v5 has NO folder browser yet (it
    arrives with `4a38222e6`), so this is a rider on that row's SPA half —
    port the picker WITH the fix, never the pre-fix shape. The merge's own
    hunks only renumber 199 → 229 and stamp `.155`.
  - `0841000c7` / `c19e5d0a9` / `0fc8ce709` — docs only (`docs/CHANGELOG.md`,
    `features/ROADMAP.md`, three NEW specs; no code, no help page, no version
    stamp). The memory spec PLANS v4's next programme (seven phases, one PR
    each, "fifteen decisions locked"): six abstractions (one candidate shape,
    one extraction context, admission decided in the parent over host-RPC,
    three named scores, one instance-wide clock, a staged recall pipeline, one
    maintenance job), **and four schema moves it names itself as "a D23
    follow for quilltap-v5"** — retire `vector_entries` for memories (DROP
    TABLE after a verify-and-copy), `memories.supersededAt`, extraction
    cursors (`add-memory-extraction-cursors-v1`), housekeeping settings to
    `instance_settings.memoryMaintenance` with the `chat_settings.
    autoHousekeepingSettings` column DROPPED, plus `recompute-reinforced-
    importance-v1` over every row. NONE of that has shipped; `124c8aca3` is
    its Phase 0 (fixes only, no schema).
  - `4a38222e6` — **wardrobe folders** (66 files, +4,378 / −180): user
    folders under every tier's `Wardrobe/`; `WardrobeItem.folder` DERIVED
    FROM THE FILE PATH on read, written back as the path (never frontmatter);
    the recursive read + the ONE recursive `isWardrobeItemDocumentPath`
    predicate (except `images/`, except `instructions.md` at any depth) now
    shared with the wear-ledger export / import and the restore id remap;
    `projectArrayIntoVaultFolder` recursive + case-insensitive sweep that
    owns only files parsing as items; five folder actions on the route
    factory (`GET ?action=folders`, `POST ?action=create-folder /
    rename-folder / delete-folder / move-items`; path rule 400, missing 404,
    collision / non-empty 409); transfers keep an item's folder; `wardrobe_list`
    / `wardrobe_read` carry `folder` READ-ONLY (` [folder: …]` after the
    title; input schemas unchanged); the SPA browser (breadcrumb, tier rows,
    select mode + bulk move, the picker, the editor's Folder field); `help/`,
    `API.md`, the export schema. **NO DDL** (folders are the existing
    `doc_mount_folders` rows) — but the vault FILE LAYOUT moves.
  - `545e299bb` — **scenarios from anywhere** (88 files, +5,003 / −2,114):
    ONE `lib/scenarios/location.ts` chokepoint (`resolveScenarioLocation` /
    `classifyScenarioMount` / `lookupScenarioRef`) REPLACING the deleted
    `general-scenarios.ts` / `project-scenarios.ts` / `group-scenarios.ts` and
    `resolveStoreScenarioTier` (every scenario route, the group-union route,
    the startup heals and the project / group create hooks through it; the
    character tier read-only); `GET /scenarios?action=everywhere` +
    `?action=read` by ref; `scenarioRef` on chat create and `?action=scenario`
    (refused mount → 400 `ScenarioRefRefusedError`; the resolver returns
    `{ scenarioText, scenarioSource, scenarioNotes }`); `?action=scenario`
    takes `reload` (without it a revised source keeps the stored body +
    new notes); `GET /chats/[id]?action=scenario-source`; **a NEW MIGRATION
    `add-chat-scenario-source-v1`** (below); the `.qtap` export schema +
    import reconcile + new-account restore remap of `scenarioSource.
    mountPointId`; the SPA `ScenarioBrowserDialog`, the in-chat control's
    provenance line / Reload / notes box, New Chat's single
    `scenarioSelection`; `help/scenario-browser.md` NEW + four help pages.
  - `124c8aca3` — memory refactor **Phase 0** (56 files, +2,697 / −288; 24
    NEW open bug files 203–226 + four fixed): **199** `buildContext`'s
    trailing-only message (no new user message: continue / chained /
    autonomous) now carries `coreWhisperLLMContext`, `llmRecallText` and
    `suparnaMailLLMContext` (it carried only the scene note, progressions,
    inform and the turn note — v5's P4.D254 port has EXACTLY that omission:
    `build_context.rs:3937-3955`); **200** `deleteMemoryWithUnlink` /
    `deleteMemoriesWithUnlinkBatch` REMOVE the deleted rows' vectors (per-
    caller removal in the cascades, housekeeping, dedup and `deleteMemory
    WithVector` deleted; the item DELETE answers 404 on a missing row); the
    gate decides on LIVE rows only — widens the search up to `4 × GATE_TOP_K`
    past orphan vectors (WARN `[MemoryGate] Vector index holds entries with no
    memory row; ignoring them`, debug `[Gate] Ignored N orphan vector(s)`),
    `bestMemory` no longer optional; **201** NEW `lib/memory/policy-
    protection.ts` (`policyProtectionOf`: MANUAL → `manual`, `CONSOLIDATED` →
    `digest`, `tier === 'cold'` → `cold`): the chat wipe deletes only hot AUTO
    rows plus digests with no surviving member (INFO `[Memory] Chat wipe
    spared manual, cold and digest rows`; the route answers `sparedCount`,
    debug retitled `Wiped a chat's extracted memories`; the regenerate job
    logs `spared`), the dedup tool folds only hot AUTO rows (`sparedCount`,
    "Kept aside" in the card); **202** closed not-a-defect — comments only.
  - `69d38dd60` — **bug 227** (17 files, +635 / −56): `buildContext` no longer
    calls `markAlerted`; it returns `mailAlertsOwed: { vaultId, paths } |
    null` (debug `[Suparna] Mail announced; alerted flip deferred to
    delivery`) and `markMailAlertsDelivered` flips the letters from the
    finalizer (`via: 'finalizer'`), the preserved-partial path, the tool-only
    save and the turn skip — a failed turn announces them again; **a swipe
    skips the mail check** (`regenerationOfMessageIds` non-empty); the
    trailing sections are built ONCE (`trailingContextSections`: Core whisper
    → recall → mail → progressions → inform → turn note) and used by both
    branches, the trailing-only message pushed only when non-empty (the Core
    whisper was ALSO missing from the no-new-message branch after
    `124c8aca3`); `help/post-office.md` two sentences + the re-announce rule.
    v5 marks alerted at collect time (`services/suparna_mail.rs:101`) —
    v4's pre-fix shape.
  - **Where they land:** `4a38222e6` on EXACTLY the surfaces the four
    wardrobe rows already hit (`9ecebf986`'s route factory, location, pool,
    `wardrobe-writes.ts`, the parsers, the tools, the SPA browser the refactor
    split out) — v5 today: `vault_overlay.rs:1227` (`is_wardrobe_item_
    document_path`, non-recursive by design, P4.D71), `db/vault_wardrobe_
    write.rs:46` (`project_array_into_vault_folder`, flat), `db/vault_wardrobe_
    public.rs`, `tools/wardrobe_{list,read}.rs` (P4.D262), `services/wardrobe_
    transfers.rs` (P4.D112), `services/qtap_export/records.rs` + `quilltap_
    import/wardrobe_wear.rs` + `backup/uuid_remapper.rs` (P4.D264), `api/
    wardrobe*.rs` + `services/wardrobe_item_route_steps.rs`, the SPA
    `apps/web/src/app/wardrobe/*` (P4.D261 / P4.D277). **Order it INTO the
    wardrobe catch-up round, after `9ecebf986` and before the SPA half** —
    it is built on the refactor's factory and location and cannot be ported
    onto v5's pre-refactor shape; the folder-picker fix (`97f0d8cb8`, bug
    229 after the `4ce64fa1e` renumber) joins its SPA half. `545e299bb` on the scenario surfaces: `db/scenarios.rs` +
    `api/scenarios.rs` + the project / group scenario routes (P4.6n — the
    scenarios round, 2026-07-11; P4.D129 smalls), `services/scenario_
    selection.rs` + `services/chat_scenario.rs` + the SPA `chat/sidebar/chat-
    scenario-control.ts` (P4.D115 / P4.D116), `services/chat_create.rs` +
    `api/chat_send.rs`'s create (P4.4 unit 2), `screens/new-chat/*` (the
    New-Chat vertical) + `scenario/scenario.api.ts`, `services/builtin_
    mounts.rs` + `host.rs`'s startup heals, `quilltap_import/reconcile.rs`
    (P4.155) + `backup/uuid_remap.rs` + `restore/orchestrator.rs` (P4.9G),
    `core/realtime-topic-map.ts`, `provisioning/fresh_schema.json` (D23).
    The Scenario Builder (P4.D216–P4.D219) is UNTOUCHED by it (the spec says
    so and the hunks agree). **Its own lane group** — by the spec's own word
    it "shares nothing but the author" with the folders — a D23 move + a new
    `lib/scenarios/` family + a carrier + a dialog is a round's worth. The
    memory pair lands on the programme just unified: `services/memory_gate.rs`
    (P4.D266's gate), `services/memory_service.rs:189` (`delete_memories_by_
    chat_id_with_vectors` — no protection partition; P4.D126's wipe
    chokepoint, P4.D270), `db/memories.rs:1052` (`delete_with_unlink` —
    vectors removed by callers), `services/memory_dedup.rs` (P4.43 / P4.D268),
    `services/housekeeping.rs` (P4.D267), `api/memories.rs`, and on the
    chat spine: `services/build_context.rs:3937` (P4.D254's trailing-only
    list), `services/suparna_mail.rs` (P4.9E2A / P4.D234 — `mark_alerted` at
    collect), `services/message_finalizer.rs:1047` (the inform mark — mail
    goes beside it), `services/primary_stream.rs` + `orchestrator.rs` (the
    tool-only save + the turn skip). **Order `124c8aca3` + `69d38dd60`
    TOGETHER** — the second rewrites the first's `trailingOnly` line into the
    one list and completes bug 199. **Recommended split:** (1) the wardrobe
    catch-up round — the four recorded rows + `4a38222e6` + `97f0d8cb8` (bug
    229, the picker), `3e6e2f590` and the merge `4ce64fa1e` ratified there; (2) a second round of two
    parallel lane groups that share no v5 file with the wardrobe set —
    scenarios-from-anywhere (server: the location + D23 + carriers; SPA: the
    dialog + New Chat + the control) and the memory Phase 0 + bug 227 pair
    (`build_context.rs` / the gate / the wipe / the mail flip) — with the
    three doc commits ratified NO-PORT there. Either order; the memory pair
    is the smaller and the more data-relevant (the wipe), so if only one
    fits, take it first. **`8050958be` (bug 228) is a third, small lane in
    round (2)** — `quilltap_import/entities.rs` `import_chats` + a NEW
    message-remap module + `reconcile.rs`'s inform remap + `files.rs`'s
    `linkedTo` + `memories.rs`'s `sourceMessageId` + the annotation remap;
    it shares `reconcile.rs` and the chat import with the scenarios lane's
    `.qtap` reconcile of `scenarioSource`, so either FOLD it into the
    scenarios server lane or sequence the two (228 first — it is the
    smaller and a data-loss fix). It is independent of the wardrobe round.
  - **Schema: a D23 move IS PENDING — the EIGHTH re-dump.** `545e299bb`'s
    `migrations/scripts/add-chat-scenario-source-v1.ts` (registered in
    `index.ts`, `dependsOn: ['add-chat-scenario-text-field-v1']`,
    `introducedInVersion: '4.10.0'`, `PRETTY_LABELS` "Noting where each chat's
    scene was found"): `chats.scenarioSource TEXT DEFAULT NULL` (JSON `{
    mountPointId, path, bodySha256 }`) and `chats.scenarioNotes TEXT DEFAULT
    NULL`, each `addColumnIfMissing`, no backfill. `lib/schemas/chat.types.ts`
    adds both to `ChatMetadataSchema` AND `ChatMetadataBaseSchema` right after
    `scenarioText` (plus `ScenarioSourceSchema` / `ScenarioRefSchema`), so
    v4's `generateDDL` (`schema-translator.ts` — a `ZodObject` field renders
    `TEXT`, stored as JSON) lands them in a fresh instance after
    `"scenarioText" TEXT` — the re-dump must come from a pin AT or past
    `545e299bb`, and an existing instance needs the migration re-homed as an
    UNSTAMPED table-gated boot ensure (the `add-memory-tiers-v1` /
    `impersonation-voice-mode-v1` shape), differentially compared against
    v4's REAL migration. No index. `4a38222e6`: NO DDL, NO migration (its
    `migrations/lib/legacy-wardrobe-rows.ts` +1 is `folder: ''` on the legacy
    row read). `124c8aca3` / `69d38dd60` / `8050958be` / `97f0d8cb8`: none (bug 228's
    `IdMappingState.messages` is in-memory import state, not schema). `public/schemas/qtap-export.
    schema.json` moves TWICE: `WardrobeItem.folder` (`4a38222e6`) and the
    chat's `scenarioSource` / `scenarioNotes` (`545e299bb`) —
    `qtap_schema_embed_guard` / `public_schemas_vendor_guard` will redden on a
    HEAD regen. The memory spec's four moves are PLANNED, not shipped.
  - **Dependencies: NONE moved** by the ten past the four wardrobe rows
    (each `package.json` + `packages/quilltap/package.json` hunk is the
    version stamp alone — `.148` / `.149` / `.150` / `.151` / `.153` /
    `.154` / `.155`; the only `package-lock.json` hunks, in `97f0d8cb8` and
    the merge, are the root package's own `version` lines; `plugins/dist/`
    untouched). v4's `CLAUDE.md` gains the
    wardrobe-folders and the scenarios chokepoint rules.
  - **Help / docs moved past the baseline (cumulative, `01a83539d..origin/
    main`):** `help/` **131 → 132 files** — NEW `help/scenario-browser.md`;
    `help/chats.md`, `general-scenarios.md`, `groups.md`,
    `project-scenarios.md` (`545e299bb`); `help/wardrobe.md`,
    `project-wardrobe.md` (`7a8d6dacc`, `9ecebf986`, `4a38222e6`);
    `help/chat-participants.md`, `memory-regenerate.md`, `system-tools.md`
    (`124c8aca3`); `help/post-office.md` (`69d38dd60`);
    `help/system-import-export.md` (`8050958be`, one line). HEAD's `help/`
    counted: 132. Docs: `API.md`,
    `DDL.md`, `IMAGE_GENERATION.md`, `features/ROADMAP.md`,
    `features/avatar-configuration-cache.md`, `features/scenario-builder.md`,
    NEW `features/memory-system-refactor.md` (1,563 lines) + `features/
    complete/{wardrobe-refactor,wardrobe-folders,scenarios-from-anywhere}.md`,
    `bugs.md` + 43 bug files (187–202 + 227–229 fixed, 203–226 open), `CLAUDE.md`,
    `.claude/commands/update-documentation.md` — the `docs/v4/` + `help/`
    re-vendors ride the catch-up rounds (each round re-vendors what it
    absorbs; `help_tree_embed_guard`'s `VENDORED_FILE_COUNT` moves 131 → 132
    with the scenarios round).
- **Regen rule: PIN REQUIRED at `01a83539d`** (§5.1) — UNCHANGED. v4 HEAD
  is fourteen commits past the baseline (the tree is now CLEAN, but HEAD
  alone requires the pin), so **a pinned worktree is mandatory for EVERY
  regen, no exceptions** — a regen from the live checkout imports the
  wardrobe refactor + folders (route bodies, the recursive predicate, tool
  text, the bug-229 picker for any jsdom oracle), the scenario location (the
  three deleted tier modules), the memory Phase-0 gate / wipe / dedup shapes
  and the mail flip, and bug 228's reminted message ids on every `.qtap`
  chat import. `provisioning_equivalence` REQUIRES `QT_FRESH_SCHEMA_LIVE` dumped
  FROM THE PIN (a HEAD dump carries `scenarioSource` / `scenarioNotes`). ⚠
  **The dependency trap:** the root tree moved at `a9c99a4a0` (INSIDE the
  baseline); no lockfile hunk since, so the root `node_modules` symlink and a
  fresh `npm ci --offline` tree agree today — still build the pin's own tree
  (§5.1), as the unification did (`/tmp/qt-v4-pin-unify-01a83539d`, `npm ci
  --offline` in 6 s).
- **Live-checkout guards at HEAD (predicted, not run):** the four wardrobe
  commits' set from the previous §1 (the `wardrobe_list` definitions
  snapshot, every tier's item-routes family, `chat_outfit` routes,
  `wardrobe_transfers`, `preview_avatar`, `outfit_summary`,
  `system_backup_equivalence`, the Almanack phase-6 roll-up, the vault-overlay
  parser pins, the tool-picture job's Concierge lines) PLUS, from the seven:
  the wardrobe tool families' formatted text (` [folder: …]`) and output
  keys (`folder`), the vault-overlay parser pins (the recursive predicate, the
  `folder` key, the `frontmatter carries a folder key` line), the projection
  sweep pins, the wear-ledger export / import / remap families, the
  scenarios routes families (general / project / group — the modules are
  GONE; `?action=everywhere` / `read` are new arms), the chat-create route
  (`scenarioSource` / `scenarioNotes` in the row; `scenarioRef`), the
  `chat_scenario` action family (`reload`, the kept-scene line), the
  `.qtap` chat export / import / reconcile families, `provisioning_
  equivalence` (two chats columns), `system_backup_equivalence` (the chat
  row), `help_tree_embed_guard` (132 vs 131) + `help_tree_equivalence` +
  `host_help_docs_boot`, `qtap_schema_embed_guard` + `public_schemas_vendor_
  guard`, the memory-gate tier-3 family (the orphan widen + the WARN), the
  chat-wipe + regenerate-chat families (`spared` / `sparedCount`), the dedup
  family (`sparedCount`, the candidate-rows debug), the housekeeping family
  (no per-caller vector removal), the memories item DELETE (404 on a missing
  row), the memory delete chokepoint (`vectorsRemoved`), the buildContext
  families (the trailing-only list, `mailAlertsOwed`, the deferred-flip
  debug), the mail tier-3 family (`markMailAlertsDelivered` lines, the swipe
  skip), the finalizer / preserved-partial / turn-skip families, and — from
  `8050958be` — every `.qtap` chat-import family that imports WITHOUT
  `preserveIds` or with `duplicate` (message ids minted, not kept; the
  duplicate branch's phantom `chats` map entry gone; the inform /
  annotation / memory / file `linkedTo` sidecars following the new ids),
  the import-warnings census (the `UNIQUE constraint failed:
  chat_messages.id` per-message warnings vanish on the duplicate rows).
  **Predicted GREEN:** `provider_sdk_version_guard`, `zod_version_guard`,
  `builtin_prompt_templates_guard`, `migration_indexes` (no index moved).
- **The workspace gate at the baseline:** the round record in
  `status-log.md` has the counts.
- **Schema state (`01a83539d`):** `fresh_schema.json` is the SEVENTH D23
  re-dump (P4.D265, from the `01a83539d` pin — exactly three statements: the
  four `memories` tier columns in generateDDL's slot, `chats.
  "otherExtractionWatermarkMessageId"` after `timelineMode`, and `CREATE
  UNIQUE INDEX "idx_doc_mount_points_name_nocase" ON "doc_mount_points"
  ("name" COLLATE NOCASE)`); `migration_indexes.json` re-dumped through v4's
  REAL `MigrationRunner` (+ `idx_memories_character_tier`; 62 → 63).
  v4's `add-memory-tiers-v1` is re-homed as an UNSTAMPED, table-gated,
  idempotent boot ensure (`db::memory_tiers_repair` — the migration's
  append-ADD column text, the index, the NULL-tier heal, the chat column;
  differentially compared against v4's real migration); the NOCASE index on
  an existing instance is a mount-index boot ensure
  (`ensure_mount_point_name_unique_index`, after the collision repair,
  P4.D274), and `reconcile_store_names` runs at boot / after restore / after
  import / on character create-rename-delete (bug 186). Every committed
  `*-main.db` pair (43) was WIDENED IN PLACE through v4's real migration at
  the pin. Earlier state (the SIXTH re-dump — `wardrobe_wear_stats` +
  `wardrobeImageSettings`, their ensures; the FIFTH — `impersonationVoiceMode`)
  unchanged. There is NO `migration_tables.json`. **The EIGHTH re-dump
  (`chats.scenarioSource` / `scenarioNotes`, `545e299bb`) is PENDING — it
  lands with the scenarios round, from a pin at or past `545e299bb`.**
- **Real-instance note:** the 2026-10-09 memory warning is RETIRED by this
  baseline (v5 carries the tiers, keeps `CONSOLIDATED` digests through
  restore, DEMOTES where v4 demotes); live Friday is ALREADY past
  `add-memory-tiers-v1` and carries the NOCASE index, so the first v5 boot
  after the round is a no-op on both ensures; P4.D274 measured ZERO planned
  renames on live Friday and on every committed fixture, but the dogfood COPY
  is older — re-run the plan probe on the copy before its first v5 boot (the
  owed dogfood pass's first row). ⚠ **From the close probe, and NOW
  ARMED — the human's checkout has PULLED to `4ce64fa1e` (2026-10-12
  evening), so the first v4 launch from it applies everything below to live
  Friday. Three hazards once v4 runs at or past `545e299bb` / `124c8aca3`
  on live Friday:** (1) v4's boot APPLIES `add-chat-scenario-source-v1` — live
  `chats` gains two nullable columns v5 at the baseline does not know. A
  v5 read of the wider row is expected to tolerate them as the
  `chat_informs.permanent` ALTER was tolerated (named-column reads;
  `/health`'s structural pass checks dedicated tables, not `chats` columns)
  — MEASURE it on the copy rather than assume — but a v5 chat UPDATE that
  rewrites `scenarioText` leaves a `scenarioSource` behind that no longer
  describes it, and a v5 backup / `.qtap` export DROPS the two keys a v4
  archive carries — restore-direction rows for the scenarios round. (2) Once items are FILED INTO FOLDERS on a live tier, v5's
  non-recursive predicate (`vault_overlay.rs:1227`) and flat projection do
  not see `Wardrobe/<sub>/*.md`: those items vanish from v5's pool (an
  outfit naming one dangles), and a v5 projection of that tier writes the
  visible items at the root while the nested originals stay — a DUPLICATE
  per filed item on the next v4 read. (3) v4's chat wipe now SPARES MANUAL,
  cold and digest rows; v5's deletes every row with the `chatId` (bug 201's
  data loss) and leaves orphan vectors behind on every per-row delete (bug
  200). **Run no v5 build against live Friday data after v4 has been run
  past `545e299bb`, and no v5 chat wipe at all, until the scenarios and the
  memory Phase-0 catch-ups land** — the same shape as the retired 2026-10-09
  warning; the dogfood COPY taken before the human pulled is safe. (Bug 228 adds
  no live hazard: v5's `duplicate` chat import fails the same way pre-fix
  v4 did — an empty copy plus per-message warnings, the original untouched —
  so the advice is only "do not rely on v5's Create New chat import".) v4 HEAD's
  backups carry `wardrobeItems: []` and `folder` on every wardrobe item; the
  `52d6e7ecd` / `07b8f0209` notes (the `chat_informs.permanent` ALTER, the
  voice-mode translation) stand.
- **`help/**`:** whole at `01a83539d` (131 files, md5-identical — P4.D278;
  `help/memory-consolidation.md` NEW, eight pages updated). Past the pin:
  twelve pages touched and ONE new (`help/scenario-browser.md`) — the list
  under "Help / docs moved" above; each catch-up round re-vendors its own.

## §2 The freshness probe

What every consuming command runs before trusting §1 — four read-only
commands, no classification, no judgment:

```bash
git -C ~/source/quilltap-server branch --show-current
git -C ~/source/quilltap-server status --short
git -C ~/source/quilltap-server log --oneline <§1 main HEAD>..main
git -C ~/source/quilltap-server log --oneline <§1 bugfix tip>..bugfix
```

Pass = the branch and tree state match §1 and both logs are **empty**. Then
§1's verdict and regen rule stand; proceed on them. Any mismatch = the ledger
is stale: `/setupphase`, `/unify`, and `/dogfood` run `/driftcheck` (or its
§4 procedure) before continuing; a `/carryout` lane **STOPs and reports**.

A dirty tree matters even when HEAD hasn't moved: uncommitted `lib/`, `app/`,
`packages/`, or `plugins/` edits poison any regen run from the checkout
(§5.1's mid-lane note). Infra-only dirt (CI files, docs) still gets recorded
here so the next probe doesn't re-alarm on it.

## §3 The drift table

Classes: **PORT** (behavior change on a ported surface), **PORT-NEW** (new v4
feature to port), **CONVERGENCE** (v4 adopting a fix this port made first —
both-directions pins will trip at the baseline move *by design*; retire them
by measurement, §5.4), **NO-PORT?** (docs/infra/tests-only candidate —
needs ratification with evidence, never by subject line alone).

Dispositions: **UNPROCESSED** → **ORDERED(order-id)** (set by `/setupphase`)
→ **ABSORBED(round)** or **NO-PORT-RATIFIED(round)** (set by `/unify` when
the baseline moves past the commit). A row leaves this table for §6 only
when absorbed/ratified.

| sha | date | subject | class | intersects (already-ported work) | disposition |
|---|---|---|---|---|---|
| `3e6e2f590` | 2026-10-09 | docs: wardrobe refactor plan; file bugs 187-197 from the audit | **NO-PORT?** (docs only — classified 2026-10-12 from the hunks: 14 files, +942 / −1 — NEW `docs/developer/features/wardrobe-refactor.md` (356 lines: the audit of the wardrobe subsystem and a seven-phase plan around ONE wardrobe location type + ONE per-request wearable pool), `docs/developer/bugs.md` (the Status line + eleven rows), eleven NEW `docs/developer/bugs/bug-187…197-*.md`, `docs/CHANGELOG.md`; no code, no help page, no version stamp — still `4.10.0-dev.143`). **CONVERGENCE check: NONE** — every one of the eleven reads `Found: 2026-10-09, code audit for the wardrobe refactor plan`; v4's own audit, none filed by this port (the port's filings are 180, 183, 184, 185; `status-log.md` / `dogfood-findings.md` name no bug past 185). The plan is the planning input for `7a8d6dacc` (its "fix 187, 188, 189 and 191 before any structural work") and `9ecebf986` (the seven phases); **read the post-`9ecebf986` copy at `features/complete/wardrobe-refactor.md`** — its §6 "As built" records the departures (no `archive-item.ts`; `buildCompositeTypes` client-safe in `composite-types.ts`; `refresh-vault-wardrobe` + `move-shared-wardrobe-to-general` kept behind `drop-wardrobe-items-table-v1`; `wearable.ts` not absorbed; take-off stays split; the outfit summary uses one pool per cast member with shared project/General reads; bug 196 kept the `WARDROBE_ITEM_IMAGE` log type; NOT done: `useWardrobeInstructions`, a wardrobe realtime topic, the legacy `images/route.ts` decode tail). Recorded by `/setupphase` at the planning close probe, 2026-10-09, PAST the `01a83539d` round's pin, so NOT this round's | — (planning input for the two wardrobe rows below; `docs/v4/` re-vendor — bugs + the feature spec) | UNPROCESSED |
| `7a8d6dacc` | 2026-10-09 | fix(wardrobe): bugs 187, 188, 189, 191 — cross-tier component refs, idempotent archive tool, chat Concierge for tool pictures, no wearing archived items | **PORT** (classified 2026-10-12 from the hunks; 30 files, +507 / −108; NO schema / DDL / migration / index move; no dependency move — `package-lock.json` +4 / −4 is the version stamp `4.10.0-dev.144`, `README.md` the badge; NOT a convergence — all four are v4's own audit findings). **187** `vault-overlay/parsers.ts` `resolveAndCheckComponentItems`: an unmatched component ref that is a UUID (`COMPONENT_UUID_REF`) is now KEPT as written — it names a part in another tier this reader cannot see (debug `Wardrobe item references a component outside this folder; keeping ref`) — so the next write no longer re-projects the stripped list; only an unmatched SLUG is dropped (the WARN retitled `Wardrobe item references unknown component slug; dropping ref`). `vault-readers.ts` / `shared-wardrobe.ts` comments only (the General seed stays until `9ecebf986` removes it). **188** `wardrobe-archive-handler.ts` goes through `archivedPatch`: an already-archived item WRITES NOTHING and returns `{ success: true, action: 'archived', already_archived: true }` (debug `Wardrobe item already archived; keeping its original date`), formatted `"<title>" (<id>) was already archived; nothing changed.`; `WardrobeArchiveToolOutput.already_archived?` (the executor's payload projection does NOT carry it — `formattedText` does); `WardrobeRepository.archive` / `unarchive` DELETED; `help/wardrobe.md`'s `wardrobe_archive` bullet gains "Retiring a garment twice changes nothing". **189** `generateWardrobeItemImage` takes `chatId?`; `handlers/wardrobe-item-image.ts` passes `payload.chatId ?? null`; the chat is loaded (WARN `[WardrobeItemImage] Chat unreadable; the failover will re-read its Concierge state` on failure), `resolveConciergeSettings(chatSettings, chat)` (debug `Concierge policy resolved` with `conciergeState` / `routeDirect`), an Unmoderated chat routes direct through `resolveImageProviderForDangerousContent` (INFO `Unmoderated chat: routed direct to the uncensored desk`), and the failover receives `chatId`, `chat`, `primaryVia` — a Locked chat never fails over, a refusal is ledgered and announced there. The editor's Generate (no chat) is unchanged. **191** NEW client-safe `lib/wardrobe/wearable.ts` (`archivedWearMessage`, `wearRefusal`, `newlyWornArchivedItems`): `?action=equip`'s `wear` / `replace` / `add_to_slot` modes refuse an archived item with 400 `Item "<title>" is archived and cannot be worn` (INFO `[Chats v1] Refused to wear an archived item`); `set_all` refuses NEWLY worn archived items only — an archived item already in the current slots passes (INFO `Refused a fitting that puts on an archived item`, 400 with the same sentence); `wardrobe_wear`'s refusal unchanged in words (now via `wearRefusal`); `wardrobe-control-dialog.tsx` gates the client. `help/wardrobe.md` "Show archived" paragraph. ⚠ Every line of this commit is rewritten again by `9ecebf986` one hour later (the handler through `setItemArchived`, the equip modes through `wear-ops`, the parser's seed removed) — port the two together | `vault_overlay.rs:1380` (`Wardrobe item references unknown component; dropping ref` — v5 drops UUIDs today; the Phase-2 vault overlay, P4.D71), `tools/wardrobe_archive.rs` + `db/wardrobe.rs` (P4.9f1 / P4.D262), `services/wardrobe_item_image_generation.rs` (`chat_id: None` at :387 / :432) + `services/wardrobe_item_image_job.rs` (P4.D263), `api/chat_outfits.rs` (the equip action — no archived check; P4.6bh / P4.9f1 / P4.D112), `tools/wardrobe_wear.rs:267` (already v4's sentence), SPA `apps/web/src/app/wardrobe/wardrobe-control-dialog.ts` (P4.9f2 / P4.D261), `help/wardrobe.md` (P4.D260 / P4.D276). Lands on top of this round's P4.D275 (`claude/p4-wardrobe-wear-readout-save-60d2c2`, the tools + routes at `01a83539d`). Reddens (predicted): the vault-overlay parser pins, the `wardrobe_archive` tool family, the `chat_outfit` routes family (`set_all` + the three modes), the item-image job tier-3 family (the Concierge lines) | UNPROCESSED |
| `9ecebf986` | 2026-10-09 | refactor(wardrobe): one location, one wearable pool, one outfit pipeline, one route factory; fix bugs 190, 192-197 | **PORT + PORT-NEW — a REFACTOR THAT MOVES BEHAVIOUR** (classified 2026-10-12 from the hunks, §5.3; 261 files, +13,511 / −11,718 — `components/` and `lib/hooks/` are roughly half; carries out all seven phases of `features/complete/wardrobe-refactor.md`). **NO schema / DDL / migration / index move:** `DDL.md` prose only (two path references); `lib/schemas/wardrobe.types.ts` adds four PURE helpers (`isComposite`, `addIdToSlot`, `removeIdFromSlot`, `equippedSlotsEqual`) + comments, the zod shapes unchanged; `llm-log.types.ts` adds `IMAGE_SPEND_LOG_TYPES = ['IMAGE_GENERATION', 'WARDROBE_ITEM_IMAGE']` over the UNCHANGED enum; the NEW `migrations/lib/legacy-wardrobe-rows.ts` is a raw read of the retired `wardrobe_items` table for the EXISTING `cutover-characters-to-vault` script + `refresh-vault-wardrobe` (the `drop-wardrobe-items-table-v1` migration dates from 4.7.0; the repository no longer knows the table). **No dependency move** (stamps `4.10.0-dev.145`). v4 `CLAUDE.md` gains the chokepoint rule ("The wardrobe is four tiers behind two abstractions"). **NOT a convergence** (190, 192–197 are v4's audit). **(a) TOOLS (model-visible):** `wardrobe_list`'s input schema DROPS `include_presets` (the OpenAI parameters SNAPSHOT moves); the seven tools dispatch through ONE `WARDROBE_TOOL_TABLE` with ONE `WardrobeToolContext` carrying `pendingWardrobeAnnouncements` (the executor's seven arms → one; payload projections unchanged), so **`wardrobe_create` + `equip_now` now fires `notifyWardrobeChanged`** — the avatar refresh AND Aurora's announcement (bug 193; it fired only the avatar); every tool resolves against ONE per-request `WearablePool` (`lib/wardrobe/pool.ts`: character > group > project > General, **archived dropped per tier BEFORE shadowing** — an archived personal item no longer masks a shared namesake; `findInPool` by id then title; WARN/debug `[WearablePool] …`), replacing `resolveWardrobeItemAcrossTiers` / `findByIdForCharacter` / `findByIdsForCharacter` / `findWearablePoolForCharacter` / `findArchetypes` / `hydrateComponentGraph` / `shared-tiers.ts` (all DELETED); `wardrobe_create`'s component resolution gains the GROUP tier and both not-found sentences become `… was not found in this character's wardrobe, their groups, the project, or Quilltap General`; new refusals `A wardrobe item must cover at least one slot`, `That character has no wardrobe to hang this in`; composite `types` through `buildCompositeTypes(components, designated)` — WIDEN, NEVER NARROW: `wardrobe_update` without `types` keeps every designated slot (bug 195); `wardrobe_wear` through `resolveWearable` / `wearItem` (`wear-ops.ts` — the not-found / archived / slot-coverage sentences unchanged in words); `wardrobe_take_off` `remove` via `takeOffItem`; list / read component titles resolve from the WHOLE pool (an archived or filtered-out part still names itself). **(b) ROUTES (wire-visible):** all four tiers' item endpoints through ONE `lib/wardrobe/routes/wardrobe-route-factory.ts` (`mount-wardrobe-route-factory.ts` DELETED; the character / General / project / group route files shrink to registrations): POST and PUT answer `{ wardrobeItem }` serialized as GET does, WITH `origin` + `wear` (the project POST no longer returns the refreshed `wardrobeItems` list); collection GETs add the tier's `mountPointId`; a component cycle is **400** on every tier (`WardrobeComponentCycleError`); PUT answers 404 for a missing item BEFORE it checks `imageFileId`; a character's endpoints require the character to belong to the requester; a write to an archived character's vault is **409** `An archived character's wardrobe cannot be changed`; General unprovisioned → GET `{ wardrobeItems: [] }`, write 500 `Quilltap General store is not provisioned yet`; the dressing-instructions arms ride the factory (409 `Character is archived; dressing instructions cannot be edited`); log lines `<tag> Listed / Created / Updated / Deleted wardrobe item`. `chats/[id]/actions/outfit.ts` (−264 / +119): the three wear modes through `wear-ops` — a not-found reference answers **404** via `resolved.reason` (was 404 already for `findByIdForCharacter` misses; the slot-coverage refusal's sentence becomes wear-ops' `Item "<title>" (types: …) cannot be added to the "<slot>" slot`, was `Wardrobe item "<title>" does not cover the <slot> slot`), log lines renamed (`[Chats v1] Wardrobe item put on`, `Wardrobe slot changed`, `Refused to put on a wardrobe item`; `worn` / `force-replaced` / `layered into slot` / `removed from slot` / `slot cleared` / `Failed to schedule outfit announcement` gone); `?action=outfit-summary` through `loadCastPools` (one pool per cast member, project + General read once) + the canonical `resolveEquippedOutfitForCharacter` (each leaf under EVERY slot its own `types` cover; debug `Built outfit summary`). `wardrobe/transfers`: `resolveSourceItem` probes character → **GROUP → project** → General (was project before group — bug 192) and provisions a store only at the destination write (`ensure: true`), `locationKey` over locations. `wardrobe/preview-avatar` rewritten through `generateImageWithConciergeFailover` + `makeLoggedImageAttempt`: 400 `No image profile is configured. Set one in Settings → Images before generating avatars.` (was `No image profile available for avatar generation` / `Selected image profile has no API key configured`), a refusal answers `The image provider declined to draw this portrait`, other failures `Image generation failed: <message>` (WARN `[Avatar Preview] Image generation failed`). **PORT-NEW:** `wardrobe/[itemId]/images` gains `POST ?action=link-image` `{ fileId }` → `linkWardrobeItemImage` (a copy-by-link of an already-stored picture — Import from image's photograph, uploaded once; 400 `That picture cannot be linked` on `UnlinkableWardrobeImageError`; INFO `Linked an existing picture to a wardrobe item`, `Refused to link a picture with no readable blob`). `chats` POST resolves the initial avatar profile through `resolveAvatarImageProfile(user.id, repos, { chat })` (debug `No usable image profile; skipping initial avatar generation`; was the chat's profile else the default); participant-join and chat-open refresh through `refreshAvatarForOutfit` (no announcement, as before). **(c) DATA / CARRIERS:** the vault reader NO LONGER seeds General archetypes into the slug / id maps (`seedArchetypes` gone — a hand-written slug naming a General item is now dropped at parse; a UUID survives per `7a8d6dacc`); `vault-overlay/wardrobe-writes.ts` is the one folder writer for all four tiers (`*ProjectWardrobeItem` → `*MountWardrobeItem`, `'group'` scope), its cycle check loads peer tiers (WARN `Wardrobe cycle check could not load peer tiers; checking the folder alone`), the write lines retitled to DEBUG `Wardrobe item created / updated / deleted in folder` (were INFO `… in vault`); `WardrobeRepository` shrinks to `findByCharacterId` / `readSharedTiers` + owner-hint write adapters (`findAll`, `findByCharacterIdRaw` DELETED; one `Cannot <op> wardrobe item: no Character Vault or Quilltap General mount is available.` line); `ensure-owner-store.ts` replaces `ensure-project-store` / `ensure-group-store`; `project-wardrobe.ts` / `group-wardrobe.ts` / `resolve-container.ts` / `dissolve-bundles.ts` / `hydrate-components.ts` DELETED; `store-file.ts` retries a unique path claimed mid-resolve. **Backup carrier:** `backup-service.ts` no longer reads `wardrobe.findAll()` — the archive's `wardrobeItems` is ALWAYS `[]`; `restore/delete-service.ts` no longer deletes `wardrobe_items` rows nor reports a `wardrobeItems` count; `restore.ts` rename only. `llm-logs.repository.getStatsByProfile` takes a type LIST (`"type" IN (?, ?)`) and the Almanack's per-image-profile roll-up (`phase6-wire-records.ts`) filters `IMAGE_SPEND_LOG_TYPES` (bug 196). **(d) PROMPTS:** the avatar and item-image prompt TEXT is byte-identical (`lib/wardrobe/avatar-{prompt,cache}.ts` → `lib/image-gen/`; `soloFigureIntro` / `withArtDirection` / `FIGURE_AESTHETIC_MAX_CHARS` factored; the three avatar tests move with them); NEW `lib/characters/physical-description.ts` `pickPhysicalDescription(desc, profile)` with five fallback orders — `appearance-resolution.ts`'s full-length pick now falls through to `fullDescription` then `headAndShouldersPrompt` BEFORE the character name (was `complete ‖ long ‖ medium ‖ short ‖ name` — model-visible in story-background / scene prompts when only those variants exist); the system prompt's `self-image` order is unchanged. **(e) IMAGES:** `character-avatar`, `story-background`, `generate_image` and wardrobe item pictures share `makeLoggedImageAttempt` + `decodeProviderImage` (`lib/image-gen/image-attempt.ts`) and `createGeneratedFileRow` (`lib/files/generated-file-row.ts`) — the `llm_logs` row shape and `Generated N image(s) (Concierge reroute)` content unchanged, log lines retitled (`No image data returned from provider`, `[ImageAttempt] …`); import-from-image's `image-analysis.ts` through NEW shared `lib/llm/vision-request.ts` — `profileCanReceiveAttachment` (bug 91's predicate) replaces `profileSupportsMimeType`, `shrinkImageForLlmTransport` before send, and the answered-without-the-image guard (`The AI returned an answer without seeing the image (…). Pick a vision profile on a model that genuinely reads images.`) — bug 197; `chat/file-attachment-fallback.ts` onto the same request (−313 lines). **(f) SPA (Phase F):** `wardrobe-control-dialog.tsx` (−1,704) split into `hooks/{useFittingRoom,useStagedLiveOutfits,useWardrobeItemActions,useWardrobeListData,useComposerHandlers}` + `AvatarGenerationPane` + `WardrobeBrowser` + `slot-ui`; TanStack tier hooks (`lib/hooks/use-wardrobe-tier.ts`; `lib/query/keys.ts` gains `wardrobe.lists / list / outfit`, `chats.project`); `ProjectWardrobeManager.tsx` + `useProjectWardrobe.ts` DELETED — the Prospero `WardrobeCard` mounts the shared `WardrobeBrowser`; `use-outfit.ts` (−471) becomes the TanStack `useChatOutfit`; the item editor's composite slots through `buildCompositeTypes` with group + archived parts (bug 190); the full-screen viewer's missing-picture cleanup through the wardrobe image routes (bug 194); `DeletedImagePlaceholder` / `FullScreenImageViewer` / `wardrobe-image-viewer` adjusted. **(g) HELP / DOCS:** `help/wardrobe.md` (two NEW paragraphs — the analyst profile for Import from image, "Slots this outfit covers"; the two archive paragraphs), `help/project-wardrobe.md` (the card IS the shared browser); `docs/developer/API.md` (the factory's contract), `DDL.md`, `IMAGE_GENERATION.md`, `features/avatar-configuration-cache.md`, `features/{ => complete}/wardrobe-refactor.md`, `bugs.md` + seven `bugs/{ => fixed}/`. ⚠ **v5's `analyze-image` is REFUSAL-ARMED** (`api/wardrobe.rs:1348`, tier 3 §4) — bug 197's vision half has NO v5 counterpart until that arm is ported (NO-PORT half, by the standing deferral) | Everything the wardrobe rounds landed: `tools/wardrobe_{list,read,create,update,archive,wear,take_off,shared}.rs` + `tools/definitions/data.rs:277` (`include_presets` accepted-and-ignored at `wardrobe_list.rs:93`; P4.9f1 / P4.D262), `api/chat_outfits.rs` (:778 the old slot sentence; P4.6bh / P4.9f1 / P4.D112), `api/wardrobe.rs` + `api/wardrobe_item_images.rs` + `services/wardrobe_item_route_steps.rs` + `services/wardrobe_container.rs` + `services/wardrobe_transfers.rs` (P4.D112, P4.D256, P4.D263), `db/vault_wardrobe_write.rs` + `db/vault_wardrobe_public.rs` + `db/archetype_wardrobe.rs` + `db/wardrobe*.rs` + `vault_overlay.rs` (Phase 2 / P4.D71), `services/outfit_selections.rs` + `services/avatar_prompt.rs` + `services/appearance_resolution.rs` + `services/avatar_generation.rs` + `services/character_avatar_job.rs` + `services/story_background_job.rs` + `tools/generate_image.rs` (P4.6k / P4.6aq / P4.123 / P4.D146), `system_prompt.rs` / `services/system_prompt_compiler.rs` (the physical-description pick — unchanged order), `services/file_fallback.rs` + `files/image_transport.rs` (the attachment fallback), `almanack/phase6_wire_records.rs:423` (`Some("IMAGE_GENERATION")`), `services/backup/collect.rs:733` + `manifest.rs:57` + `restore/orchestrator.rs:2477` (the `wardrobeItems` carrier; P4.9G / P4.D264), `services/llm_logging.rs`, the chat-create route (P4.4 unit 2); SPA `apps/web/src/app/wardrobe/*` (P4.9f2 / P4.D72 / P4.D113 / P4.D130 / P4.D188 / P4.D261) + `screens/prospero/wardrobe/project-wardrobe-manager.ts` + `images/` (P4.D257); `help/` (p4.9i2 / P4.D260 / P4.D276) + `docs/v4/`. **Lands on top of this round's two unmerged wardrobe lanes** — P4.D275 (`claude/p4-wardrobe-wear-readout-save-60d2c2`, 164 files: the tools' wear readout, the 183 / 184 convergence, `save-targets` / `save-to-store`) and P4.D277 (`claude/wardrobe-images-spa-viewer-4a334b`, 41 files: the wardrobe / images SPA) — which port `01a83539d` onto exactly these files. Reddens (predicted, on a HEAD regen): the `wardrobe_*` tool families (the definitions snapshot, the create sentences), `chat_outfit` routes (`add_to_slot`'s sentence, the log lines, `set_all`), every tier's item-routes family (POST / PUT bodies, the cycle 400, the 409, `mountPointId`), `wardrobe_transfers`, `preview_avatar`, `outfit_summary`, `system_backup_equivalence` (`wardrobeItems: []`), the Almanack phase-6 family, the vault-overlay parser pins, `help_tree_equivalence`, the chat-create route's initial avatar. **Order as ONE wardrobe catch-up with `7a8d6dacc` and `e23fb095b`, after this round unifies — never commit-by-commit** | UNPROCESSED |
| `e23fb095b` | 2026-10-10 | fix(wardrobe): bug 198 — dialog showed a stale cached outfit after a tool-driven change | **PORT (SPA only)** (classified 2026-10-12 from the hunks; 11 files, +165 / −10 — TWO code files: `lib/hooks/use-outfit.ts` +15 / −3, `lib/realtime/topic-map.ts` +5; two tests, the bug file, `bugs.md`, `CHANGELOG.md`, `README.md`; no dependency move — stamps `4.10.0-dev.146`). **NOT a convergence:** bug 198 is "Original to v4", found 2026-10-10 LIVE on Friday by the human ("The Master Bedroom Tour": Laura undressed by two `wardrobe_take_off` calls, Aurora announced it, the dialog reopened from the participants sidebar still showed her dressed; a Done would have re-dressed her by `set_all`) — not this port's. The fix, both halves: `useChatOutfit` gains `refetchOnMount: 'always'` and `slotsFor` answers `undefined` until `query.isFetchedAfterMount`, so a cached snapshot from an earlier visit can never seed the ref-gated Live tab / Outfit Builder staging (bug 61's "in-progress edits win" contract unchanged); `queryKeysForTopic('chats', id)` adds `queryKeys.wardrobe.outfit(id)`, so a tool-driven outfit change — whose tool rows and Aurora's announcement land as messages and publish `chats` — invalidates the dialog's cached outfit. Sits on `9ecebf986`'s TanStack `useChatOutfit` + `queryKeys.wardrobe.outfit` — order after it | SPA: `apps/web/src/app/wardrobe/outfit-store.ts` (a signal port of the PRE-TanStack `use-outfit.ts` — whether it re-reads on every dialog open or serves a snapshot across visits is to be MEASURED at the port, not assumed; P4.9f2 unit 3), `wardrobe-control-dialog.ts` + `staged-live-outfits.ts` (P4.9f2 / P4.D72 / P4.D261), `core/realtime-topic-map.ts` (the `chats` arm invalidates `chatKeys.detail(id)` only — the realtime invalidation subsystem). No server surface; no harness family | UNPROCESSED |
| `0841000c7` | 2026-10-12 | docs: specs for scenarios from anywhere and wardrobe folders | **NO-PORT?** (docs only — classified 2026-10-12 from the hunks: 4 files, +444 / −0 — NEW `docs/developer/features/scenarios-from-anywhere.md` (229 lines) and `features/wardrobe-folders.md` (200 lines), two lines in `features/ROADMAP.md`, `docs/CHANGELOG.md`; no code, no help page, no version stamp — still `4.10.0-dev.146`). Both specs are the planning input for the two code commits below and were MOVED to `features/complete/` by them (`4a38222e6`, `545e299bb`) with §7 "As built" sections — read THOSE copies, not these. **CONVERGENCE check: NONE** (no bug; nothing filed by this port). Ratify with the round that absorbs the two code commits; the `docs/v4/` re-vendor rides it | — (planning input for `4a38222e6` and `545e299bb`; `docs/v4/` re-vendor) | UNPROCESSED |
| `c19e5d0a9` | 2026-10-12 | docs: memory system refactor spec | **NO-PORT?** (docs only — classified 2026-10-12 from the hunks: 3 files, +865 / −0 — NEW `docs/developer/features/memory-system-refactor.md` (847 lines), one ROADMAP line, `CHANGELOG.md`; no code, no version stamp). **What it plans (nothing shipped by this commit):** "one Commonplace Book, not nine" — six abstractions (one candidate shape; one extraction context with four passes and cursors; a pure admission decision applied in the PARENT over host-RPC; three named scores with one function each; one instance-wide clock for event time; a staged recall pipeline whose delivery step is the only record of what the model saw; one maintenance job with a parent-side ledger; one `retire.ts` deletion chokepoint; every memory knob behind one `resolveMemorySettings`), 28 defects to file as bugs 199–226, seven phases (0, A–G) one PR each, fifteen decisions locked by the human 2026-10-12. **Schema moves it names — none shipped yet, each "a D23 follow for quilltap-v5" by the spec's own §8:** retire `vector_entries` for memories (`retire-memory-vector-entries-v1`: verify every row has a `memories.embedding`, copy where null, then `DROP TABLE`; backup writes no `vector_entries.json`, restore ignores one), `memories.supersededAt` (bug 225), extraction cursors (`add-memory-extraction-cursors-v1`), housekeeping settings to `instance_settings.memoryMaintenance` with `chat_settings.autoHousekeepingSettings` DROPPED (`move-memory-maintenance-settings-v1`, which also drops the legacy `chat_settings.memoryExtractionLimits`), `recompute-reinforced-importance-v1` over every row (`min(1, i + log2(n) × 0.05)`). §6 "What disappears" retires `VectorIndicesRepository` / `vector-indices.types.ts`. **CONVERGENCE check: NONE** — v4's own 2026-10-12 audit (the port's filings are 180, 183, 184, 185). Ratify with the round that absorbs `124c8aca3` (its Phase 0); watch v4 for Phases A–G — each is a PORT round of its own, two with D23 moves | — (planning input for `124c8aca3` and for v4's coming Phases A–G; `docs/v4/` re-vendor). Names v5's P4.D265–P4.D276 surfaces as its target: `services/memory_gate.rs`, `memory_service.rs`, `memory_processor.rs`, `housekeeping.rs`, `memory_consolidation_job.rs`, `db/vector_store.rs` + `vector_indices.rs`, `memory_recap/`, the settings reads | UNPROCESSED |
| `0fc8ce709` | 2026-10-12 | docs: append the four audit reports to the memory refactor spec | **NO-PORT?** (docs only — classified 2026-10-12 from the hunks: 2 files, +712 / −3 — Appendices A–D appended to `features/memory-system-refactor.md` (creation / extraction; gate, merge, dedup and linking; scoring and recall; maintenance, settings and operator surfaces — the audits the diagnosis was drawn from, with v4 line numbers "as of `0841000c7`"), `CHANGELOG.md`; no code, no version stamp). Appendix D.6 lists every migration touching `memories` in registry order — a useful cross-check for the EIGHTH-plus re-dumps. **CONVERGENCE check: NONE.** Ratify with `c19e5d0a9` | — (reference for the memory programme's reviewers; `docs/v4/` re-vendor) | UNPROCESSED |
| `4a38222e6` | 2026-10-12 | Wardrobe folders — file items into a tier's closet (#85) | **PORT-NEW + PORT** (classified 2026-10-12 from the hunks, §5.3; 66 files, +4,378 / −180 — a squashed PR of two commits, the feature and its review fixes; `__tests__/` + `components/` are roughly half). **NO DDL, NO migration, NO index** (`migrations/lib/legacy-wardrobe-rows.ts` +1 is `folder: ''` on the legacy row read; folders are the EXISTING `doc_mount_folders` rows). **No dependency move** (stamps `4.10.0-dev.148`; no lockfile hunk). **NOT a convergence** (no bug number; the in-flight local bug "199 — folder picker closes the dialog" is its review follow-up, uncommitted, colliding with origin's 199 — see §1). **The model:** `WardrobeItem.folder` (`wardrobe.types.ts`: `WardrobeFolderPathSchema.default('')` — `''` = root, ≤ 8 segments, no segment `images`, sanitised with the item-file rule, case-insensitively unique among siblings) is DERIVED FROM THE FILE PATH on read (`folderOfWardrobePath(doc.relativePath)` in `parseWardrobeItemFile`; a frontmatter `folder:` key is IGNORED with debug `Wardrobe/*.md frontmatter carries a \`folder\` key; the file path wins`) and written back AS the path on projection (`joinFolder(item.folder, '<title>.md')` in `wardrobe-sync.ts`) — never stored in frontmatter. **(a) The ONE predicate, now recursive:** `isWardrobeItemDocumentPath` is true for any `.md` under `Wardrobe/` at any depth EXCEPT under `Wardrobe/images/` (`WARDROBE_RESERVED_FOLDER_SEGMENTS`) and except `instructions.md` at any depth; `readCharacterVaultWardrobe` reads `{ recursive: true }` and filters with it (debug `Wardrobe read skipped non-item documents`), and the wear-ledger `.qtap` export (`ndjson-writer`), the ledger import (`import-wardrobe-wear`) and the restore id remap (`uuid-remapper`'s `buildWardrobeItemIdRemap`) now agree through it — nested items were dropped by all three flat side-channels before. NEW `isWardrobeItemDocument(doc)` (path + a valid frontmatter `types` list) and `hasStableWardrobeItemId`. **(b) Projection:** `projectArrayIntoVaultFolder` gains `recursive: { excludeSubfolders }` + `ownsFile` — the sweep removes ONLY files that parse as items, at every depth (a note or half-written file under `Wardrobe/` is never deleted — this NARROWS the old root-level sweep too), compares paths LOWER-CASED (the store's upsert is case-insensitive and keeps stored casing, so the old exact-case compare could delete a file it had just rewritten when only casing differed — applies to flat callers), ensures only subfolders holding no existing document. **(c) Location + mutations:** `WardrobeLocation` gains `updateMany` (one read, one projection; `updateInMount` is a one-patch call of it), `listFolders` / `createFolder` / `renameFolder` / `deleteFolder` / `resolveFolder`; NEW `lib/wardrobe/folders.ts` (rules, client-safe: `normalizeWardrobeFolderPath[OrRoot]`, `isValidWardrobeFolderPath`, `isSafeWardrobeFolderPath`, `rebaseFolder`, `folderCrumb`, `wardrobeFolderStorePath`, the `WardrobeFolderPathError` / `NotFoundError` / `ConflictError` classes), `folder-mutations.ts` (`moveItemsToFolder` checks borrowed ids inside the one write via `updateMany({ requireAll })`), `folder-view.ts` (the browser's pure view rules); `sanitizeFileName` MOVED to client-safe `lib/mount-index/sanitize-file-name.ts` (re-exported from `character-vault.ts`). Requested folders resolve against the tier's EXISTING folder rows (an existing one used exactly as stored — a hand-made `Hats: Summer` is a valid target; only new parts normalised); `updateItem` drops a `folder` equal to the current one; a create / patch / import `folder` passes `safeFolder` (kept when structurally safe, else normalised or sent to the root — covers `.qtap` `wardrobe_item` records). Rename stamps UUID ids into id-less item files (one projection) BEFORE `moveDatabaseFolder` (a path-derived id would change). Delete-folder re-roots items (archived included) to the parent in one `updateMany` batch, re-creates empty subfolder rows under the parent, removes the old rows deepest-first; REFUSED 409 up front for a folder holding non-item files or a same-named subfolder; `NOT_EMPTY` → 409. **(d) Routes (the factory, every tier):** `GET ?action=folders` (`{ folders }`; NO provisioning — `{ folders: [] }` for a project / group with no store or unprovisioned General; debug `<tag> No store yet; no wardrobe folders` / `Listed wardrobe folders`), `POST ?action=create-folder` `{ path }` (provisions like a create; INFO `Created wardrobe folder`), `rename-folder` `{ path, name }`, `delete-folder` `{ path }`, `move-items` `{ itemIds ≤ 500, folder }`; a rule-breaking path or a move naming an item filed elsewhere → 400, missing folder → 404 `Wardrobe folder`, collision → 409; `wardrobeItemFieldsSchema.folder` optional on create / update (create defaults to the root; update leaves it unchanged when omitted); `wardrobeCollectionActionUrl` on the container. `wardrobe/transfers`: every planned item carries `folder` (component and source through `normalizeWardrobeFolderPathOrRoot` — a refused hand-made folder lands at the root rather than failing the transfer); the destination folder is created as needed; the dialog gains no control. **(e) Tools — READ-ONLY, model-visible TEXT:** `wardrobe_list` / `wardrobe_read` results gain `folder` (`null` at the root) and ` [folder: …]` after the title in both renderings (`formatWardrobeListResults` line, `formatWardrobeReadResults` head line, `buildWardrobeReadFailure` `folder: null`); both descriptions gain the "filing only, not a hint about when to wear it" sentence; NO input schema changes (the OpenAI parameters snapshot does NOT move); `wardrobe_create` files at the root. **(f) Carriers:** `qtap-export.schema.json` `WardrobeItem.folder` (maxLength 900; importers file at the same path, normalising or falling back to the root); `restore/archive.ts`, `legacy-presets.ts`, `move-shared-wardrobe-to-general.ts` fill `folder: ''`. **(g) SPA (Phase C):** `WardrobeBrowser.tsx` (+611: breadcrumb, folder rows first then items, tier rows in a character's view — *Group: …* / *Project: …* / *General* with a chip, hidden by **Show shared** —, search-everywhere with folder crumbs, a **Select** toggle + sticky bar with bulk move, inline folder rename, delete through the existing confirmation, row ⋮ *Move to folder…*; *Move* / *Copy* renamed *Move to another wardrobe…* / *Copy to another wardrobe…*; view state remembered per container rather than reset by effects), NEW `WardrobeFolderPickerDialog.tsx` (folder tree + inline New folder…), `wardrobe-item-editor/WardrobeFolderField.tsx` (options from the DESTINATION wardrobe; a value not in the tree is created on save; sent only when changed), `lib/hooks/use-wardrobe-folders.ts`, `lib/query/keys.ts` (+7), `wardrobe-item-row.tsx`; drafts carry `folder` (Duplicate files beside the original). **(h) Help / docs:** `help/wardrobe.md` (+35: the folders section), `help/project-wardrobe.md`, `API.md` (+15), v4 `CLAUDE.md` (the chokepoint rule), `features/{ => complete}/wardrobe-folders.md` §7 As built. ⚠ The live walkthroughs (§6 A.4, C.4, D.2) were NOT performed by v4's build session — the human's V4test walk found the picker bug (the uncommitted fix in §1). **Built ON `9ecebf986`'s factory / location / pool / `wardrobe-writes.ts` — cannot be ported onto v5's pre-refactor shape; order INTO the wardrobe catch-up round after `9ecebf986`** | `vault_overlay.rs:1227` (`is_wardrobe_item_document_path` — DIRECTLY inside `Wardrobe/`, "not nested deeper", by design; `parse_wardrobe_item_file`; the Phase-2 vault overlay, P4.D71 / P4.D256), `db/vault_wardrobe_write.rs:46` (`project_array_into_vault_folder` — flat, `preserve_file_names`; the sweep on `find_many_by_mount_points_in_folder_or_empty`), `db/vault_wardrobe_public.rs`, `services/wardrobe_container.rs` + `wardrobe_item_route_steps.rs` + `api/wardrobe*.rs` (P4.D112 / P4.D256 — pre-factory, every tier its own route), `services/wardrobe_transfers.rs` (P4.D112), `tools/wardrobe_list.rs` + `wardrobe_read.rs` + the formatters (P4.D262 — no `folder` key, no tag), `services/qtap_export/records.rs` + `quilltap_import/wardrobe_wear.rs` + `backup/uuid_remapper.rs` (P4.D264 — the three flat side-channels), `mount_index` sanitize (`sanitize_file_name`), the SPA `apps/web/src/app/wardrobe/wardrobe-container.ts` + `wardrobe-item-row.ts` + `wardrobe-item-editor.ts` + `wardrobe-transfer-dialog.ts` + `wardrobe.api.ts` (P4.9f2 / P4.D261 / P4.D277 — no folder concept), `help/wardrobe.md` + `project-wardrobe.md` (P4.D276). Reddens (predicted, on a HEAD regen): the vault-overlay parser pins (the recursive predicate, the `folder` key on every parsed item, the `folder key` debug line), the projection-sweep pins (lower-cased compare, `ownsFile`), the wear-ledger export / import / remap families, both wardrobe tool families (the output key + the tag), every tier's item-routes family (the five actions; `folder` on POST / PUT bodies), `wardrobe_transfers`, `qtap_schema_embed_guard` / `public_schemas_vendor_guard`, `help_tree_equivalence` on the two pages. ⚠ Live-data hazard once items are filed on Friday: see §1's real-instance note (2) | UNPROCESSED **Later fact (2026-10-12 evening `/driftcheck`):** the uncommitted folder-picker fix landed as `97f0d8cb8`, renumbered to bug 229 by the merge `4ce64fa1e` — its own row below; port the picker WITH it. |
| `545e299bb` | 2026-10-12 | Scenarios from anywhere: location, browser, and source tracking (#86) | **PORT-NEW + PORT — WITH A D23 MOVE (the EIGHTH re-dump)** (classified 2026-10-12 from the hunks, §5.3; 88 files, +5,003 / −2,114 — a squashed PR of four commits: the feature, the control's review fixes, the heal / honest-count fixes, the read-only character location; `__tests__/` + `components/` are roughly half). **SCHEMA / MIGRATION:** NEW `migrations/scripts/add-chat-scenario-source-v1.ts` (registered in `index.ts`; `dependsOn: ['add-chat-scenario-text-field-v1']`; `introducedInVersion: '4.10.0'`; `shouldRun` when either column is missing; `addColumnIfMissing('chats', 'scenarioSource', 'TEXT DEFAULT NULL')` + `('chats', 'scenarioNotes', 'TEXT DEFAULT NULL')`, no backfill; INFO `Added the scenario source columns to chats` / ERROR `Failed to add the scenario source columns`, `context: 'migration.add-chat-scenario-source'`; `prettify.ts` label "Noting where each chat's scene was found"). `lib/schemas/chat.types.ts`: `ScenarioSourceSchema` `{ mountPointId: UUID, path: string.min(1).max(500) — \`Scenarios/<file>.md\`, bodySha256: string.length(64) — hex SHA-256 of the body as resolved, frontmatter stripped, before notes }`, `ScenarioRefSchema` `{ mountPointId, path }`; `scenarioSource: ScenarioSourceSchema.nullable().optional()` + `scenarioNotes: z.string().nullable().optional()` added to BOTH `ChatMetadataSchema` and `ChatMetadataBaseSchema` immediately after `scenarioText`, so v4's `generateDDL` (`schema-translator.ts`: a `ZodObject` field → `TEXT`, stored as JSON; nullable-optional → `DEFAULT NULL`) renders a fresh `chats` with `"scenarioSource" TEXT DEFAULT NULL, "scenarioNotes" TEXT DEFAULT NULL` after `"scenarioText" TEXT` — `fresh_schema.json` must be RE-DUMPED from a pin at or past `545e299bb` (D23, never by hand) and the migration re-homed as an UNSTAMPED table-gated boot ensure differentially compared against v4's REAL migration (the `add-memory-tiers-v1` shape, P4.D265). No index. `DDL.md` +2 column lines (`scenarioSource` "written only beside `scenarioText` by `resolveScenarioSelection`'s two callers; NULL = custom scene or pre-source chat"). **No dependency move** (stamps `.149`; no lockfile hunk). **NOT a convergence** (no bug number). **(a) The chokepoint:** NEW `lib/scenarios/location.ts` (566 lines) — `resolveScenarioLocation(scope: 'character' \| 'project' \| 'group' \| 'general', id, repos, userId, { ensure })` (overloaded: a store scope returns a `StoreScenarioLocation` with writes; the CHARACTER location is READ-ONLY — `list` reads the vault's `Scenarios/` documents through `parseScenarioFile` so paths are the files' real paths, the default from the row's `defaultScenarioId`; archived and other users' characters resolve null), `classifyScenarioMount` (a mount is a scenario owner when it is a live character's vault, a project's or group's OFFICIAL store, or General — NOT linked stores, NOT arbitrary Scriptorium stores), `lookupScenarioRef` → `ok \| missing \| refused` (one row read per character ref — the location is built from the row that classified the mount), `characterScenarioIdForPath`, `parseScenarioForScope`, `normalizeScenarioPath`, `ScenarioConflictError` / `NotFoundError` / `FilenameError`, `GENERAL_OWNER_NAME = 'Quilltap General'`; NEW `everywhere.ts` (`listScenariosEverywhere` — one document query, name / description / flags / a 240-char excerpt ending `…`, `scenarioId` on character rows; NO bodies) and `source-remap.ts` (client-safe). **DELETED:** `lib/mount-index/general-scenarios.ts` (−115), `project-scenarios.ts` (−91), `group-scenarios.ts` (−104), `resolveStoreScenarioTier`; `scenarios-common.ts` −147 (the test-only `listScenariosInFolder` / `readScenarioByPath` / `resolveScenarioBody` gone); the group tier's `Knowledge/` helper → `lib/mount-index/group-knowledge.ts`. NEW raw finders `characters.findByDocumentMountPointIdRaw`, `AbstractStoreBackedRepository.findByOfficialMountPointIdRaw` (projects + groups share it). `instrumentation.ts`'s General heal → `resolveScenarioLocation('general', null, repos, SINGLE_USER_ID, { ensure: true })`; `ensure-project-scenarios.ts` / `ensure-group-scenarios.ts` report `{ total, provisioned, healed, failed }` (an `ensure: true` folder failure RETHROWS — was swallowed); the project / group create hooks go through the location. **(b) Routes:** `scenario-item-route-factory.ts` (431 lines rewritten) serves General as well as projects and groups + `createScenarioCollectionBodies` (list + create); all six route files are thin configs (`scenarios/route.ts` −167 → `GET ?action=everywhere[&includeArchived=true]` `{ sources }`, `GET ?action=read&mountPointId=&path=` — 400 `mountPointId and path are required` / `Invalid scenario ref: <reason>`, 404 `Scenario`; `scenarios/[scenarioPath]/route.ts` −251; the project / group collection routes −150 / −148; `groups/scenarios` the union); item routes resolve with `ensure: true` on PUT / rename / DELETE (plus the group's `Knowledge/` via `afterEnsure`) while GET stays a probe and an owner with no store is still 404; the character scenarios route (`/characters/[id]/scenarios`) is UNCHANGED (id-addressed, through the repository); a character location update carries `defaultScenarioId` across a retitle and refuses a retitle onto an existing file. `chats` POST (`route.ts`, 66 lines changed): `scenarioRef: ScenarioRefSchema.optional()` in the body; `resolveScenarioSelection` takes `{ repos, userId, projectId, characterId, logTag }` (was `character`) and RETURNS `ResolvedScenario { scenarioText, scenarioSource, scenarioNotes }` (was a string) — precedence `scenarioRef` FIRST, then `scenarioId` > project > group > general, each failing soft; a refused ref throws `ScenarioRefRefusedError` → 400; the row is written with all three. `chats/[id]/actions/scenario.ts` (206 lines changed): `scenarioRef: ScenarioRefSchema.nullish()`, `reload: z.boolean().optional()` — WITHOUT `reload`, a request that resolves to the chat's OWN source file, revised since it was picked, keeps the stored preset body (`recoverPresetBody` from `scenarioText` minus the `\n\n<notes>` suffix, verified against the source hash) with the new notes beneath (debug `[Chats v1] Source file revised; keeping the chat's scene (no reload asked)`); `reload: true` pulls the revision; the no-op check `sameResolvedScenario` compares text + source + notes — a change of provenance or notes with byte-identical text is WRITTEN but neither recompiles nor announces; INFO `[Chats v1] Scenario changed` gains `source`. NEW `GET /chats/[id]?action=scenario-source` (`handleGetScenarioSource`: the source, `scenarioName`, `scenarioId` for a character source; debug `[Chats v1] Scenario source checked`). `lib/chat/scenario-text.ts` `recoverPresetBody`. `topic-map.ts`: the `chats` topic also invalidates `queryKeys.chats.scenarioSource(id)`. **(c) Carriers:** `qtap-export.schema.json` chat `scenarioSource` (object \| null; `mountPointId` "remapped with the document stores on import; dropped to null when that store did not come across") + `scenarioNotes` (string \| null); `quilltap-import/reconcile.ts` remaps `scenarioSource.mountPointId` in `reconcileRelationships` (mapped → new id; unmapped but present in this instance → kept; else → null; debug `Remapped chat scenario source`) — chats import BEFORE document stores; `restore/uuid-remap.ts` (new-account mode) through `remapScenarioSource` + the shared `UuidRemapper`. Backup: whole-table, nothing to do. SillyTavern export: not represented. **(d) SPA (Phases C–D):** NEW `components/scenario/ScenarioBrowserDialog.tsx` (399 lines — every character vault / project / group / General, searchable, a preview fetched for the highlighted row, `staleTime: 0`, no polling), `ScenarioSelect.tsx` gains **Browse all scenarios…** beside Custom (a pick outside the hierarchy shows under an **Elsewhere** optgroup as *Owner › Name*; one inside snaps to its native kind) and ALWAYS renders the dropdown; `ChatScenarioControl.tsx` (+201: the provenance line "From: *Owner* › *Name*", **Reload from source** when the file exists and its hash differs, a notes box under presets — the text box is the notes under a preset and the whole scene under Custom, always sent as `scenario`; a source whose file is gone is not selectable; leaving Custom for a preset clears the box); `NewChatForm.tsx` / `useNewChat.ts` / `types.ts`: ONE `scenarioSelection: ScenarioSelection` replaces the four pointer fields in form state (the wire still accepts all four + `scenarioRef`), seeded once by `seedScenarioSelection` (character default > project default > general default; continuation mode skips the character default; not re-seeded once `scenarioTouched`) — which FIXES the form showing the project default while submitting the character's; the project-default override note only when the project default is the selection and the character has one of its own; `useScenarioRefBody`; `lib/query/keys.ts` (+12). **(e) Help / docs:** NEW `help/scenario-browser.md` (52 lines — the help tree goes 131 → 132), `help/chats.md`, `general-scenarios.md`, `groups.md`, `project-scenarios.md`; `API.md` (+28), `DDL.md`, v4 `CLAUDE.md` (the chokepoint rule), `features/{ => complete}/scenarios-from-anywhere.md` §7 As built, `features/scenario-builder.md` (one line — the Builder is UNTOUCHED: its mount pool and `SaveScenarioDialog` already know all four owner kinds). **Its own lane group** — shares nothing with the wardrobe set | `db/scenarios.rs` (the chokepoint — `ParsedScenario`, P4.6n / P4.D208 / P4.D219) + `api/scenarios.rs` + `api/projects.rs` / `api/groups.rs`'s scenario arms + `services/builtin_mounts.rs` (General's folder heal) + `quilltap-host/src/host.rs` (the startup heals) — the P4.6n scenarios round (2026-07-11, the three tier modules v5 ported as they were) + P4.D129's smalls; `services/scenario_selection.rs` (P4.D115 unit 1 — returns the TEXT) + `services/chat_scenario.rs` (the `?action=scenario` verb, P4.D115 units 3–4 / P4.155 unit 3) + `scenario_text.rs`; `services/chat_create.rs` (P4.4 unit 2 — the create's scenario resolve); `db/chats.rs` + `chats_read.rs` (the row — no `scenarioSource` / `scenarioNotes` columns); `services/quilltap_import/reconcile.rs` (P4.155) + `entities.rs` + `services/backup/uuid_remap.rs` + `uuid_remapper.rs` + `restore/orchestrator.rs` (P4.9G / P4.D264); `provisioning/fresh_schema.json` (the SEVENTH re-dump — P4.D265; EIGHTH pending); the SPA `scenario/scenario-select.ts` + `scenario.api.ts` + `scenario.types.ts`, `chat/sidebar/chat-scenario-control.ts` (P4.D116 — no `reload` / `scenarioSource` / `scenarioRef` today), `screens/new-chat/new-chat.state.ts` + `new-chat.logic.ts` + `new-chat-form.ts` (the New-Chat vertical — four pointer fields), `core/realtime-topic-map.ts`, `help/` (p4.9i2 / P4.D278). Reddens (predicted, on a HEAD regen): the general / project / group scenario routes families (the modules are GONE; `?action=everywhere` / `read` NEW), the chat-create route (`scenarioRef`; the two new row keys), the `chat_scenario` action family (`reload`, the kept-scene arm, the no-op compare, the `source` field), the chat GET dispatch (`scenario-source`), the `.qtap` chat export / import / reconcile families + the new-account restore remap, `provisioning_equivalence` (two columns — the D23 tripwire firing as designed), `system_backup_equivalence` (the chat row), `help_tree_embed_guard` (132) / `help_tree_equivalence` / `host_help_docs_boot`, `qtap_schema_embed_guard` / `public_schemas_vendor_guard`, the realtime topic-map spec. ⚠ Live-data hazard once v4 runs past this on Friday: §1's real-instance note (1) | UNPROCESSED |
| `124c8aca3` | 2026-10-12 | Fix bugs 199–201: memory gate, continue turns, chat wipe (#87) | **PORT** (classified 2026-10-12 from the hunks, §5.3 — read from `origin/main`, NOT yet pulled into the human's checkout; 56 files, +2,697 / −288 — a squashed PR of three commits: Phase 0, a version bump after merging main, the review fixes; 24 NEW open bug files 203–226 + four `fixed/` 199–202 + `bugs.md` +32 are 29 of the 56; nine code files). **NO schema / DDL / migration / index move; no dependency move** (stamps `.151`; no lockfile hunk). **NOT a convergence** — bugs 199–202 are v4's own 2026-10-12 memory audit, each `Original to v4` (the port's filings are 180, 183, 184, 185); 203–226 stay OPEN, each naming the refactor phase that closes it. ⚠ **Bug 199's trailing-only fix here is REWRITTEN by `69d38dd60` an hour later** (the one list) — port the two together. **199** `context-manager.ts` `buildContext`: the no-new-user-message branch (`else if (userNarrationAnchor \|\| coreWhisperLLMContext \|\| llmRecallText \|\| suparnaMailLLMContext \|\| turnSkipInstruction \|\| progressionsLLMContext \|\| informBlock)`) now pushes `[userNarrationAnchor, coreWhisperLLMContext, llmRecallText, suparnaMailLLMContext, progressionsLLMContext, informBlock, turnSkipInstruction]` (was `[userNarrationAnchor, progressionsLLMContext, informBlock, turnSkipInstruction]` — recall, mail and the Core whisper were POSTED, the rows stamped accessed and recorded in the anti-repetition ring, the letters marked alerted, and the model never saw them on continue / chained / autonomous turns); debug `[CommonplaceWhisper] Delivering recall in the trailing-only message (no new user message)` `{ chatId, participantId, length }`. **200** `memory-gate.ts`: `GATE_MAX_SEARCH_K = GATE_TOP_K * 4`; the gate searches, loads the matched rows (`findByIds`), and if fewer than `GATE_TOP_K` are LIVE widens `k` by the shortfall and searches again until live ≥ K, the store is exhausted (`results.length < k`) or `k ≥ 4K`; `results` = live hits only, sliced to K; orphans WARN `[MemoryGate] Vector index holds entries with no memory row; ignoring them` `{ characterId, orphanIds, searchedK }` + `debugInfo` `[Gate] Ignored N orphan vector(s)`; orphans are NOT pruned there (in the job child a row this job inserted is invisible to the snapshot — bug 209 — and would look like one); `bestMemory` is non-null by construction (the `bestMemory !== undefined` / `&& bestMemory` guards on the distinct-occasion, near-duplicate and merge branches and the `.filter(r => r.memory)` DROPPED). The delete chokepoint grows `removeDeletedVectors(idsByCharacter)` (best-effort per character: `removeVector` each, `save()` when any removed; WARN `[MemoryGate] Failed to remove vectors for deleted memories`): `deleteMemoryWithUnlink` removes the vector and logs `vectorsRemoved`; NEW `deleteMemoriesWithUnlinkBatchReport(ids, options) → { deleted, vectorsRemoved }` with `deleteMemoriesWithUnlinkBatch` as its `.deleted` wrapper; `DeleteBatchOptions` exported. Callers simplified: `memory-service.ts` `deleteMemoryWithVector` = `deleteMemoryWithUnlink` (its own vector removal gone), `deleteMemoriesBySourceMessageWithVectors` / `…BySourceMessagesWithVectors` through the report (their per-character loops gone; `characterCount` from a Set), `housekeeping.ts` runHousekeeping's vector cleanup DELETED, `memory-dedup.ts`'s vector cleanup DELETED, `DELETE /memories/[id]` calls the chokepoint directly (no second read) and answers 404 `Memory` when it returns false. **201** NEW client-safe `lib/memory/policy-protection.ts`: `policyProtectionOf({ source, tier })` → `'manual'` (source MANUAL) \| `'digest'` (source CONSOLIDATED) \| `'cold'` (tier cold) \| null, `partitionByPolicyProtection(rows) → { actionable, spared, sparedBy: { manual, digest, cold } }`. `deleteMemoriesByChatIdWithVectors` returns `{ deleted, vectorsRemoved, characterCount, spared }`: partitions `findByChatId`, then a spared DIGEST goes too when NO member not being wiped still names it (`consolidatedFrom` minus the doomed ids → `findByIds` once → `supersededById` set; a member re-promoted or re-consolidated elsewhere no longer speaks for it); INFO `[Memory] Chat wipe spared manual, cold and digest rows` `{ chatId, spared, manual, cold, digests }`; `handleDeleteByChatId` answers `sparedCount` and its debug is retitled `[Memories API] Wiped a chat's extracted memories` `{ chatId, deleted, spared }` (was `Deleted every memory for a chat`); `memory-regenerate-chat.ts` logs `spared` at both sites. `deduplicateCharacterMemories` folds only `actionable` rows (`originalCount` = total; `sparedCount` on every result incl. the `< 2` early returns and `deduplicateAllMemories`' per-character rows; debug `[MemoryDedup] Candidate rows for folding` `{ total, candidates, sparedBy }`); the card shows "Kept aside"; `useMemoryActions.ts` / `SalonListView.tsx` / `character-conversations-tab.tsx` wording. **202** comments only (`searchMemoriesSemantic`: anchor hits widen the pool and face the floor — R5). `help/chat-participants.md`, `memory-regenerate.md`, `system-tools.md` one line each; `features/memory-system-refactor.md` (+10 — Phase 0 marked done); v4 `CLAUDE.md` | `services/build_context.rs:3937-3955` (P4.D254's trailing-only list — EXACTLY the pre-fix four-item list: scene note, progressions, inform, turn note; `core_whisper` / `llm_recall_text` / the mail are computed above it and dropped on that branch — v5 REPRODUCES bug 199), `services/memory_gate.rs:444` (`store.search(embedding, GATE_TOP_K)` once; `:471` `if let Some(best_memory) = best_mem.as_ref()` — the pre-fix optional guard; P4.D266), `db/memories.rs:1052` / `:1141` (`delete_with_unlink` / `delete_many_with_unlink` — rows + neighbour scrub, NO vector removal; P4.d12), `services/memory_service.rs:189` (`delete_memories_by_chat_id_with_vectors` — every row with the `chatId`, no partition; `cascade_delete` + `remove_vectors_grouped` per caller; P4.D126 / P4.D270), `services/housekeeping.rs` (its own vector cleanup; P4.D267), `services/memory_dedup.rs:229` + `:461` (`deduplicate_character_memories`, every tier and source folded, its own `remove_vector`; P4.43 / P4.D268), `api/memories.rs:706` (the wipe route — no `sparedCount`) + `:644` (the item DELETE), the regenerate-chat job, the SPA `memory/` dedup card + the Salon list wipe wording (P4.D271). Reddens (predicted): the memory-gate tier-3 family (the orphan widen + WARN; the dropped guards), the chat-wipe + regenerate-chat families (`spared`), the dedup family (`sparedCount` + the debug), the housekeeping family, the memories item DELETE family (404), the delete-chokepoint tier-2 (`vectorsRemoved`), the buildContext families on a continue / chained / autonomous turn (the trailing-only content). **Order TOGETHER with `69d38dd60`**; ⚠ live-data hazard (the wipe): §1's real-instance note (3) | UNPROCESSED |
| `69d38dd60` | 2026-10-12 | Fix bug 227: mail marked announced before its turn succeeds; one trailing-section list (#88) | **PORT** (classified 2026-10-12 from the hunks, §5.3 — read from `origin/main`, NOT yet pulled; 17 files, +635 / −56 — a squashed PR of three commits: 227, the 199 completion, the review follow-up; six code files, four tests, `help/post-office.md`, the 227 bug file + the 199 file's +7). **NO schema / dependency move** (stamps `.153`; no lockfile hunk). **NOT a convergence** — bug 227 is `Original to v4. Present since the Post Office's mail check was added to buildContext`, found 2026-10-12 in review of the 199 fix. **(a) The mail flip is deferred to delivery:** `buildContext` no longer imports `markAlerted`; `BuiltContext` gains `mailAlertsOwed: MailAlertsOwed \| null` (`{ vaultId, paths }` — the letters Suparṇā announced this turn, "like `informRowIds`, marked only once the turn has delivered them"); the mail block runs only when `mailVaultId && !isRegeneration` (`isRegeneration = (regenerationOfMessageIds?.length ?? 0) > 0` — **a swipe skips the mail check altogether**: a re-roll of a line already spoken must not carry a letter that arrived since, and the swipe path never reaches the finalizer that would mark it); on announce, debug `[Suparna] Mail announced; alerted flip deferred to delivery` `{ chatId, characterId, vaultId, letters }`. NEW `mailbox.ts` `markMailAlertsDelivered(owed, { chatId, via })` — `markAlerted` per path, warn-only per letter (`markMailAlertsDelivered: could not mark letter alerted`), debug `markMailAlertsDelivered` `{ chatId, vaultId, via, requested, marked }`. Call sites: `message-finalizer.service.ts` after the inform mark (`via: 'finalizer'`); `primary-stream.service.ts` `makePreservePartialOnError` takes `mailAlertsOwed` and marks `via: 'preserved-partial'`; `orchestrator.service.ts` passes it to the preserve-partial options at both sites, marks `via: 'tool-only'` after a tool-only save ("the model read this turn's mail and acted"), and `handleTurnSkip` takes `mailAlertsOwed` and marks `via: 'turn-skip'` ("a pass is an answer"). A failed or aborted turn therefore announces the letters AGAIN next attempt — at the cost of a second whisper, "the honest record of a letter re-announced". **(b) One trailing-section list:** `trailingContextSections = [coreWhisperLLMContext, llmRecallText, suparnaMailLLMContext, progressionsLLMContext, informBlock, turnSkipInstruction].filter(Boolean)` is built ONCE before the branch and used by BOTH — the new-user-message branch's hand-kept six-push copy DELETED, and the no-new-message branch becomes a plain `else` pushing `[userNarrationAnchor, ...trailingContextSections]` ONLY when non-empty (was pushed whenever any section existed; the PR's review note that "the Core whisper was also missing from turns with no new user message" describes the pre-Phase-0 list — `124c8aca3`'s seven-item line already carried it, and the one list keeps it); the comment carries the adjacent-user-message note (a chained turn's history routinely ends in a `user`-attributed line from another seat; OpenAI-compatible APIs accept adjacent user messages, Anthropic's combines them, the Google plugin merges them before sending). `help/post-office.md`: the check runs "whether the character is answering you or simply carrying on unprompted", and "the one exception is a turn that comes to nothing … Suparṇā will read it again on the character's next turn". Tests pin the order on both paths, the Core whisper case, the deferral and the swipe skip | `services/suparna_mail.rs:101` (`mark_alerted` AT COLLECT TIME inside the mail check — v4's pre-fix shape; P4.9E2A / P4.D234's Post Office, the P4.D45 string table), `services/suparna_notifications.rs:265`, `post_office/mailbox.rs:395` (`mark_alerted`; no `mark_mail_alerts_delivered`), `services/build_context.rs:3937-3955` (the trailing-only branch; the new-message branch's own list above it — two hand-kept copies, as v4 had; P4.D254), `services/message_finalizer.rs:1047-1090` (the inform mark — `inform_row_ids` empty on a swipe; the mail mark goes beside it), `services/primary_stream.rs:677` (`inform_row_ids` on the preserve-partial options), `services/orchestrator.rs` (the tool-only save + the turn skip), `tools/read_mail.rs:166` (the tool's own mark — unchanged by v4). Reddens (predicted): the buildContext tier-3 families on both branches (the one list; `mailAlertsOwed` in the built context; the deferred-flip debug; the swipe's missing mail block), the mail tier-3 family (the `alerted` flip's timing — it now happens AFTER the model call: a mocked-LLM failure must leave the letter unalerted), the finalizer / preserved-partial / turn-skip families (the four `via` lines), `help_tree_equivalence` on `post-office.md`. **Order TOGETHER with `124c8aca3`** (it rewrites that commit's `trailingOnly` line) | UNPROCESSED |
| `8050958be` | 2026-10-12 | Fix bug 228: importing a chat with "Create New" dropped every message (#89) | **PORT** (classified 2026-10-12 evening from the hunks, §5.3; 20 files, +910 / −69; `4.10.0-dev.154`, parent `69d38dd60`. `lib/` hunks ONLY in `lib/import/quilltap-import/`: NEW `message-id-remap.ts` (169 lines — `remintChatMessageIds`, `remapImportedMessageId`), `import-entities.ts` (`importChats`: the `duplicate` and fresh-id create paths made ONE; a chat created under a fresh id gets fresh message ids, each occurrence its own even when a bundle repeats a source id; `swipeGroupId` rewritten only in its `swipe-<messageId>` form; `summaryAnchorMessageIds` remapped, an unresolvable anchor CLEARING the summary + anchors + `lastSummaryTurn` together as rebuild-summary does; `otherExtractionWatermarkMessageId`; `courierCheckpoints[*].lastResolvedMessageId`, the JSON-string form read, unreadable entries dropped; the duplicate branch now maps the source chat to the REAL created id — the phantom `idMaps.chats` entry is gone), `types.ts` (NEW `IdMappingState.messages: Map<sourceChatId, Map<sourceMsgId, newId>>`, written only after an insert lands), `execute.ts` (annotation `sourceMessageId` through the remap), `reconcile.ts` (`remapChatInform`: `recordMessageId` / `consumedByMessageId` through the remap; a `null` = reminted-but-never-landed treated as missing; the refusal `reason` keeps the SOURCE id), `import-files.ts` (`remapLinkedTo` resolves a message only through a chat named in the same `linkedTo` array). The memories import `sourceMessageId` follows the same map. `help/system-import-export.md` +1 sentence (the "(imported)" copy keeps its transcript). Tests: NEW `duplicate-chat-message-ids.test.ts` (445 lines) + five touched. NO DDL, NO migration, NO dependency (stamps only). **CONVERGENCE check: NONE** — `Provenance: Original to v4`, found live on `V4test`, `v5 status: Not assessed`.) | **v5 SHARES the defect**: `services/quilltap_import/entities.rs:791` `import_chats` adds each message under its bundle id (`ChatMessagesRepository::add_message` with the decoded event) and keeps v4's ruled "Phantom-map quirk" (`entities.rs:829-834` + the module header, `DUPLICATE_MINTS`) — ported faithfully in the `.qtap` import family (P4.9G-era import EXECUTE; the bare-tail warnings P4.147/#140, P4.155 whole-entity refusal, P4.161 whole-row validation), so the phantom-map pins and every duplicate-strategy chat row flip by design. Sidecars to follow: `quilltap_import/reconcile.rs` (the inform remap, P4.155 / P4.D249's `permanent` carrier), `quilltap_import/files.rs` (`linkedTo`), `quilltap_import/memories.rs` (`sourceMessageId`; P4.161 / P4.D272's up-front id minting), the annotation import; the restore path (`backup/`) is NOT touched — restore preserves ids. Shares `reconcile.rs` + the chat import with `545e299bb`'s scenario reconcile — fold into or sequence with that lane (§1). `help/` re-vendor of one page. | UNPROCESSED |
| `97f0d8cb8` | 2026-10-12 | fix(wardrobe): a click inside the folder picker no longer closes the wardrobe dialog (bug 199 → renumbered 229 by `4ce64fa1e`) | **PORT** (SPA-only rider on `4a38222e6`; classified 2026-10-12 evening from the hunks; 10 files, +167 / −26; `4.10.0-dev.150`, parent `545e299bb` — the close probe's recorded dirt, committed: `components/wardrobe/WardrobeBrowser.tsx` drops its local `picker` state for `actions.folderPicker`; `hooks/useWardrobeItemActions.ts` owns the picker state and counts it in `overlayOpen`, so a mousedown inside the portalled listbox no longer reaches the dialog's click-outside close; the jsdom folders test +1 case; `bugs.md` + the bug file; `package-lock.json` hunk = the root `version` lines only. No `lib/` / `app/` / `packages/` / `plugins/` hunk. **CONVERGENCE check: NONE** — v4's own SPA defect in a feature v5 has not ported.) | None today — v5 has no wardrobe folder browser or Move-to-folder picker; the SPA half of `4a38222e6` lands it in `apps/web/src/app/wardrobe/*` (P4.D261 / P4.D277's browser + dialog). Port the picker with its state in the actions service and counted by the dialog's overlay gate from the start, and carry the bug-229 test as a Playwright / component beat. | UNPROCESSED |
| `4ce64fa1e` | 2026-10-12 | Merge origin/main; renumber the folder-picker bug to 229 | **NO-PORT?** (merge of `97f0d8cb8` and `8050958be`; classified from its OWN resolution, `git show --cc`: the README badge, the version stamp `.155` (both `package.json`s + the lockfile's root `version` lines), and the picker fix RENUMBERED 199 → 229 — the bug file renamed to `bugs/fixed/bug-229-folder-picker-closes-dialog.md`, `bugs.md`, two code comments and one test title. `git diff 8050958be 4ce64fa1e` equals `97f0d8cb8`'s content plus that renumber; no behaviour of its own. Resolves the close probe's predicted bug-199 collision in favour of the memory audit's 199.) | None — ratify with `97f0d8cb8` in the wardrobe round. | UNPROCESSED |

## §4 How a full drift check runs (the `/driftcheck` procedure)

1. **Read §1** for the baseline and the previously recorded state. Never
   take the baseline from memory — CLAUDE.md's Status and this file must
   agree; if they don't, that disagreement is itself a finding to report.
2. **Both branches, always.** Since v4's 4.8.0/4.8.1 releases (2026-08-12)
   v4 develops on TWO branches: `main` (next-dev) and `bugfix` (patch
   maintenance). Release flow: a `bugfix: started X.Y bug branch` commit
   forks the branch; fixes land there; `release: X.Y` squashes back onto
   main. The topology is squash-like — bugfix commits are NOT ancestors of
   main — so `git log <baseline>..main` misses nothing but shows the release
   squash, while `git log <baseline>..bugfix` shows the whole historical
   lineage and looks alarmingly long. **Measure bugfix by CONTENT, never the
   commit list:** `git log main..bugfix --oneline -- lib/ app/ packages/`
   for candidates, then `git diff main bugfix -- <paths>` to confirm what is
   genuinely unabsorbed.
3. **Record the checkout's posture:** `git branch --show-current` and
   `git status --short`. A checkout sitting on `bugfix`, or dirty in
   `lib/`/`app/`/`packages/`/`plugins/`, silently poisons regens (it has
   happened — see §5.1) and flips the §1 regen rule to pin-required.
4. **Classify every new commit** — `git show --stat` for the file list,
   then the hunks. ⚠ **Never classify (or later port) from the commit
   message.** A v4 commit message describes the bug's shape as its author
   understood it; the shipped hunks are often narrower. The spec is
   `git show <sha>:<path>` (the post-commit file) plus `git show <sha> --
   <path>` (the hunks). When the message asserts a behavior change, find
   the hunk that makes it; if there is no such hunk, the claim is about the
   bug, not the fix (P4.D110/bug 96 is the canonical case). Carry a note of
   what a commit did NOT do into the row when the prose misleads.
5. **Delineate the intersection with ported work.** For each commit's
   files, name the v5 surface and the round/lane that ported it — grep
   `status-log.md` and CLAUDE.md's round bullets for the v4 path, feature
   name, or bug number. Rules of thumb: `lib/**` and `app/api/**` are fully
   ported (Phases 2–4); `components/**`/`app/**` client code maps to the
   SPA verticals; `packages/quilltap/**` is the CLI Tier R;
   `help/**` banks to `p4.9i2`; `.github/`, `docker/`, release scripts,
   and tests-only changes are NO-PORT candidates. Check v4's
   `docs/developer/bugs.md` for the bug number: a bug this port filed
   coming back fixed is a **CONVERGENCE** row (pins will trip; §5.4).
6. **Update §1 and §3** — never touch existing ORDERED/ABSORBED
   dispositions except to append newer facts. Commit per
   `.claude/commands/commit.md` (docs-only) and report: the verdict, the
   new rows, which ported surfaces are affected, and whether the regen rule
   changed.

## §5 Standing drift machinery — recipes and traps

_Moved into the repo from the session-memory notes, 2026-08-25. This is the
durable home; the memory notes now just point here._

### §5.1 The pinned-worktree regen recipe

A fresh oracle is only as pinned as the tree it imports. Whenever v4 HEAD is
past the baseline OR the checkout is dirty or on the wrong branch, regen from
a detached worktree pinned at the baseline. v4 is the human's active repo —
**never stash or checkout-switch it.**

```bash
PIN=/tmp/qt-v4-pin-<order>-<sha>       # LANE-UNIQUE path — never share pins
git -C ~/source/quilltap-server worktree add --detach "$PIN" <baseline-sha>
# THREE symlink classes, all untracked and absent from a bare worktree:
ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
ln -sfn ~/source/quilltap-server/packages/quilltap/node_modules \
        "$PIN/packages/quilltap/node_modules"
for d in ~/source/quilltap-server/plugins/dist/*/; do
  [ -d "$d/node_modules" ] && ln -sfn "$d/node_modules" "$PIN/plugins/dist/$(basename "$d")/node_modules"
done
cd "$PIN"   # run EVERY tsx/jest oracle + fixture builder from here
# cleanup: git -C ~/source/quilltap-server worktree remove --force "$PIN"
```

- The `@/` alias resolves through the cwd's tsconfig, so cwd = the pinned
  worktree imports pinned lib code. The second symlink feeds the real-DB
  jest cases' `requireActual` of the cipher driver; the third feeds any
  oracle that loads a provider plugin (`provider-registry.ts`, the
  stream/envelope recorders, the manifest generator) — without it they die
  on `Cannot find module '@anthropic-ai/sdk'`.
- **The dependency trap (2026-10-07, the `94fbb1ae3` boot-hardness
  unification):** the root `node_modules` symlink follows the LIVE checkout,
  so once the human pulls v4 past the baseline and runs `npm install`, a
  "pinned" regen imports HEAD's dependency tree (that day: 105 package
  versions — `openai` 7.30.0, `@openrouter/sdk` 1.4.25, `next`, `sharp`,
  `babel`). `provider_sdk_version_guard` is the tripwire (it reddens on the
  root SDKs). The cure, offline from the npm cache: `rm "$PIN/node_modules"
  && (cd "$PIN" && npm ci --offline --no-audit --no-fund)` — a real tree from
  the pin's own lockfile in seconds. Check the other two classes against the
  lockfile diff (`packages/quilltap/` and the plugin dirs did not move that
  day) before trusting their symlinks.
- **The empty-file trap:** that failure is loud on stderr, but if stdout was
  redirected to the oracle file, the redirect already truncated it to ZERO
  bytes — the next diff then reads "DIFFERS against an empty file" exactly
  like real drift. Check the file is non-empty before believing a diff.
- **Verify the pin** by grepping the fresh NDJSON for a marker only one tree
  has (a sentence the drift added must be ABSENT from a baseline-pinned
  oracle).
- **Lane-unique paths:** a pin shared between lanes was deleted mid-run by
  whoever finished first (P4.d26) while v4 HEAD was past the baseline — the
  later regen would silently have used the wrong tree. Cheap guard:
  `git -C ~/source/quilltap-server worktree list` before each regen batch.
- **The checkout can go dirty MID-LANE** (P4.D105): clean at lane start,
  dirty two units later. Do not reason about whether the dirty files "could"
  have mattered — build the pin, re-run every regen the lane already did
  from it, and expect the re-run to change nothing. That is a cheap, total
  proof; the alternative is an argument.
- The pin is also what makes an end-of-lane sweep honest:
  `harness/tools/recipe_sweep.py --run-all --v4 "$PIN"` rewrites every
  recipe's `cd ~/source/quilltap-server`.

### §5.2 The silent-stale-pass trap (regen verification)

A fixture builder that ERRORS leaves the old fixture in place, and the
differential then passes **green on stale data** — the green comes from the
old fixture + old oracle agreeing with each other as they always had.
Discipline for every regen:

```bash
rm -f /tmp/qt-<x>.db                      # a failed build can't hide behind a stale file
... build ... 2>&1 | tail -1              # read it; a stack trace invalidates the run
grep -c '<new-field-or-id>' /tmp/oracle-<x>.ndjson   # MUST be > 0
```

Corollaries: a green regen is not coverage — grep every regenerated NDJSON
for the CHANGED bytes; when appending to a pinned-id corpus, check the WHOLE
id set for collisions, not the tail; anchor jest `--` filters
(`"case\.test\.ts$"`) because substring matches let a sibling case clobber
the same `QT_ORACLE_OUT`.

### §5.3 Commit prose misdescribes diffs

(Also in §4 step 4, because it bites at classification AND at porting.) Port
from the shipped hunks, never the message. Re-verify even when the work
order already flagged the trap — the order's paragraph is the same kind of
prose. Carry the finding into a v5-side code comment naming the sha and what
it did NOT do.

### §5.4 Convergence rows: measure, don't assume

When v4 adopts a fix this port made first, the both-directions divergence
pins trip at the baseline move **by design** — that tells you v4 MOVED, not
HOW. Regenerate the oracle and dump v4's ACTUAL post-fix output before
retiring a pin to a plain equality: v4's adoption is often partial or
differs in wording/details the planner never checked (P4.D51's bug-8 message
heads and bug-12 phantom-row surprises are the canonical cases). A partial
convergence splits one carve-out into a converged half + a still-divergent
half — expect to reshape pins, not just delete them. When a converging arm
reddens, instrument the harness to dump the actual rows (a `QT_DEBUG_*`
-gated `eprintln!`), and for restore/import read the archive's own manifest
as ground truth.

### §5.5 Drift heals real data — 💸 proofs expire

A banked live proof that depends on **damaged rows existing** can expire
because v4 (running daily on the real instance) lands its own fix and heals
the data — the orphan-reaper and poisoned-base-URL proofs both died this
way. When a round banks such a proof, record the measurement date and count;
before building a walk step around it, **measure the population first** (one
read-only query; a zero is a finding, not a failure); when it has expired,
retire it explicitly rather than carrying it forward. Planting the damage on
the disposable copy proves the mechanism but is a weaker claim — offer it,
don't silently swap it in.

## §6 History

- **The `01a83539d` memory-programme + bugs-183–186 + wardrobe-viewer drift
  catch-up round (2026-10-12, baseline `f5e953a3f` → `01a83539d`; P4.D265 →
  {P4.D266 ∥ P4.D267 ∥ P4.D268 ∥ P4.D269 ∥ P4.D270 ∥ P4.D271 ∥ P4.D272 ∥
  P4.D273 ∥ P4.D274 ∥ P4.D275} ∥ P4.D276 ∥ P4.D277 ∥ P4.D278):** the thirteen
  rows retired — `1825bfd53` (the memory specs), `da98ca58b` (the F2 probe
  result + retuning spec), `7e9eaf42c` (the 4.10 regression tests + the
  All-LLM pause refactor v5 already matches — verified at
  `salon-conversation.ts`'s `onAllLLMContinue`) and `783385873` (the 4.10.0
  release notes, mirrored) NO-PORT-RATIFIED(P4.D278, on the file lists at the
  pin); `5abcd01ea` and `3c56a41e7` (this port's own bug 183/184/185 filings)
  NO-PORT-RATIFIED(P4.D278); `3f7320138` (F1–F9) ABSORBED(P4.D265, P4.D266,
  P4.D267, P4.D268, P4.D269); `f7d8064be` (the recall retuning)
  ABSORBED(P4.D267, P4.D268, P4.D269); `d58548051` (hot/cold tiers +
  consolidation, the SEVENTH D23 re-dump) ABSORBED(P4.D265, P4.D266, P4.D267,
  P4.D268, P4.D269, P4.D270, P4.D271, P4.D272, P4.D273, P4.D276, P4.D278) with
  the `MEMORY_REGENERATE_CHAT` fold-grain rebuild DEFERRED by name;
  `70f9b495c` (bug 182's atomic reinforcement) ABSORBED(P4.D265, P4.D266);
  `197104649` (the `[Sync] Reading disk file` line) ABSORBED(P4.D274);
  `ed8b15b50` (bugs 183/184/185 CONVERGED — the convergence pins retired by
  measurement, `FRESH_STORE_RESIDUAL` on 30 of 32 cases, the two exceptions
  re-pinned as `DUP_STORE_ID_FIRST_ROW` and `ARCHIVED_CHARACTER_FALLBACK`;
  bug 186 PORT-NEW with the NOCASE index riding the re-dump + the boot /
  restore / import / character-path reconciles) ABSORBED(P4.D265, P4.D273,
  P4.D274, P4.D275, P4.D277, P4.D278); `01a83539d` (the wardrobe picture
  viewer + save-to-store + the message Save Image 409) ABSORBED(P4.D275,
  P4.D277, P4.D278). Four commits arrived past the pin DURING the round
  (`3e6e2f590`, `7a8d6dacc`, `9ecebf986`, `e23fb095b` — v4's wardrobe audit,
  fixes and refactor) — §3, UNPROCESSED, classified by the 2026-10-12
  `/driftcheck`; every regen stayed pinned at `01a83539d`. Round record:
  `status-log.md` → "The `01a83539d` memory-programme + bugs-183–186 +
  wardrobe-viewer drift catch-up round — UNIFICATION record".
- **The `f5e953a3f` wardrobe-programme + convergence drift catch-up round
  (2026-10-08, baseline `94fbb1ae3` → `f5e953a3f`; P4.D255 → {P4.D256 ∥
  P4.D262 ∥ P4.D263 ∥ P4.D264} ∥ P4.D257 ∥ P4.D258 ∥ P4.D259 ∥ P4.D260 ∥
  P4.D261):** the eleven rows retired — `938144eb4` (4.10.0 release-notes
  draft) and `7c78abd49` (the wardrobe programme's three specs)
  NO-PORT-RATIFIED(P4.D260, docs-only, on the file lists; `releases/4.10.0.md`
  + the specs mirrored); `cc80dc89d` (#80 lists / read-time `origin`)
  ABSORBED(P4.D256, P4.D261); `3ee3b1342` (#81 the wear ledger)
  ABSORBED(P4.D255, P4.D256, P4.D262, P4.D264, P4.D261); `f9f1ba177` (the chat
  gallery's backdrop + current-avatars passes) ABSORBED(P4.D257);
  `7c8572869` (#82 item images) ABSORBED(P4.D255, P4.D256, P4.D263, P4.D264,
  P4.D261); `a9c99a4a0` (the merge — the root dependency move) ABSORBED(P4.D260
  — measured, the SDK guard re-scoped, google-wire re-recorded, no recorded
  byte moved); `b3f937076` (the wardrobe tools' pictures)
  ABSORBED(P4.D255, P4.D262, P4.D263, P4.D261; the plugin bundles P4.D260);
  `06a70a76f` (this port's own bug-180/181 filing) NO-PORT-RATIFIED(P4.D258);
  `039f7017c` ABSORBED(P4.D258) for bugs 180/181 (CONVERGED — the LLM-logs
  cold-open ladder, the `number[]` embedding writer, the index-keyed decode +
  the written-rows count; the `INDEX_KEYED_EMBEDDING` carve retired by
  measurement) + NO-PORT-RATIFIED(P4.D258) for bug 179 (no child process / no
  buffered overlay in v5 — pinned `bug_179_no_port`); `f5e953a3f` (the daily
  optimize + physical backups) ABSORBED(P4.D259 — the startup trio moved to a
  JOINED pass before the pumps at the unification). The SIXTH D23 re-dump
  landed (no `migration_tables.json` — R-A overruled). Eight commits arrived
  DURING the round (`1825bfd53` … `7e9eaf42c`, v4's memory programme) — §3,
  UNPROCESSED; every regen stayed pinned at `f5e953a3f`. Round record:
  `status-log.md` → "The `f5e953a3f` wardrobe-programme + convergence drift
  catch-up round — UNIFICATION record".

- **The `94fbb1ae3` boot-hardness + validation + follow-ups round
  (2026-10-07, baseline STAYS `94fbb1ae3`; P4.159 ∥ P4.160 ∥ P4.161 ∥ P4.162
  ∥ P4.163 ∥ P4.164):** no row absorbed (the round's §3 was EMPTY at
  planning); v4 drifted SEVEN commits DURING the round (§3, UNPROCESSED), and
  by the human's ruling every regen stayed pinned at `94fbb1ae3`. Round
  record: `status-log.md` → "The `94fbb1ae3` boot-hardness + validation +
  follow-ups round — UNIFICATION record".
- **The `94fbb1ae3` fresh-instance-indexes + follow-ups smalls round
  (2026-10-06, baseline `07b8f0209` → `94fbb1ae3`; P4.D254 ∥ P4.153 ∥ P4.154
  ∥ P4.155 ∥ P4.156 ∥ P4.157 ∥ P4.158):** `94fbb1ae3` (the inform block as a
  trailing context section under ONE vouching header + a new debug line)
  ABSORBED(P4.D254 — `INFORM_BLOCK_HEADER` byte-exact against v4's exported
  constant; the system push deleted, the block pushed after recall / mail /
  progressions and before the turn-skip note, the trailing-only branch's
  condition and list grown; the `[Inform] Delivering …` debug line
  capture-pinned per op against v4's patched logger; the two existing
  `[Inform]` lines onto camelCase; `build_context_tier3` grown by six ops,
  mutation-proven six ways; `help/inform.md` + three `docs/v4/` paths
  re-vendored; the SPA dialog's doc comment). Round record: `status-log.md`
  → "The `94fbb1ae3` fresh-instance-indexes + follow-ups smalls round —
  UNIFICATION record".
- **The `07b8f0209` two-commit drift catch-up round (2026-10-05, baseline
  `52d6e7ecd` → `07b8f0209`; P4.D251 ∥ P4.D252 ∥ P4.D253):** `a434c715b`
  (bugs 177/178, PDF text extraction) ABSORBED(P4.D253) for bug 177 — the
  PDF arm reads through the `DocumentTextExtractor` seam first with v4's
  three new lines, under the first tier-1 family over `extractFileContent`
  (the old header's premise was false: before the fix v4 failed EVERY PDF
  with `pdfParse is not a function`, 13 of 13 rows at `52d6e7ecd`) — and
  NO-PORT-RATIFIED(P4.D253) for bug 178 (`next.config.js` only; v5 has no
  bundler). `07b8f0209` (the impersonated-line voice as three modes, a
  schema move) ABSORBED(P4.D251, P4.D252 — the FIFTH D23 re-dump, the
  re-homed `impersonation-voice-mode-v1` boot ensure REPLACING the P4.D179
  one and differentially compared in three shapes, the TEXT enum at the six
  data-layer sites, the PUT's enum arm with the retired key ignored, the
  Almanack's three labels, the legacy translation under the restore with
  its own tier-1 family, 17 committed pairs narrowed, `help/` + `docs/v4/`
  re-vendored; the SPA's three-way gate, the draft stage with no model call
  under `ask`, Restate, the two footers, the three-radio card, the two beats
  LIVE). Round record: `status-log.md` → "The `07b8f0209` two-commit drift
  catch-up round".
- **The `52d6e7ecd` standing-informs drift catch-up round (2026-10-04,
  baseline `e5c6bd0c0` → `52d6e7ecd`; P4.D249 ∥ P4.D250 ∥ P4.146 as the
  rider):** `52d6e7ecd` (Inform: standing per-chat informs — a NEW
  migration) ABSORBED(P4.D249, P4.D250 — the FOURTH D23 re-dump + the
  re-homed boot ensure with BOTH v4 shapes carried and the ensure itself
  differentially compared against v4's real migration; ONE `is_inform_in_
  force` predicate under every read and delete with a source census; the
  two comparators; the block's swipe merge, stamped-row exclusion and
  `standing` debug counts; the `permanent` body key as a tri-state refusing
  `null` on both transports; the 201 / list / cancel bytes; import / export
  key order / backup / restore carriers; `help/inform.md`, five `docs/v4/`
  paths and `qtap-export.schema.json` re-vendored; the SPA's checkbox, help
  tail, three-way toast, chip strings and the gated beat run LIVE at the
  unification). Round record: `status-log.md`.
- **The `e5c6bd0c0` drift catch-up round (2026-10-03, baseline
  `f6426e196` → `e5c6bd0c0`; P4.D245 ∥ P4.D246 ∥ P4.D247 ∥ P4.D248):**
  `9753d0eb2` (the project roster as a tool-access gate)
  ABSORBED(P4.D245, P4.D246, P4.D247 — the `project_roster_access`
  chokepoint at v4's seven sites with the enumeration twin folded, the new
  real-DB family; the create default `true` at both sites with the READ
  default kept, the enriched PUT through the GET's helper, the chat-create
  auto-add deleted and the chat-PUT half a convergence; the Characters card
  whole — and the phantom `roster` key, a v5 defect since P4.6l, fixed);
  `e5c6bd0c0` (bugs 175/176, this port's own filings) ABSORBED(P4.D248 —
  the collapse defers and boots, the PHASE 3.1 structural pass + the
  `/health` `structure` service, and by the human's mid-lane ruling an
  absent dedicated table CREATED at boot from v4's DDL dump; the SPA's
  degraded-503 carve-out in P4.D247). `help/**` + `docs/v4/**` whole at
  `e5c6bd0c0` (P4.D246). Round record: `status-log.md`.
- **The `f6426e196` bug-174 drift catch-up + review-follow-ups round
  (2026-10-02, baseline `ca363178d` → `f6426e196`; P4.D244 ∥ P4.135 ∥
  P4.136 ∥ P4.137 ∥ P4.138):** `f6426e196` (bug 174: a vault image's
  server-relative `url` sent to Z.AI and NanoGPT) ABSORBED(P4.D244 — the
  loader's `url` key deleted on both `chat_files.rs` branches; the Z.AI +
  NanoGPT builders bytes-first with an absolute-`http(s)` url forwarded
  through one ASCII `is_absolute_http`; OpenRouter untouched and pinned as a
  non-change; `file_attachment_tier3` red-first + a native-text arm; the
  request corpus re-recorded at the pin, 385 → 393 rows, and the
  unification's six absolute-url rows incl. an upper-case `HTTP://` pinning
  v4's `/i` regex, 393 → 399; the `docs/v4/` mirror's three paths). The
  plugin/`package.json` stamps NO-PORT (the standing class). Round record:
  `status-log.md`.

- **The `ca363178d` four-commit drift catch-up + dogfood-orders round
  (2026-10-01, baseline `97b25fc53` → `ca363178d`; P4.D240 ∥ P4.D241 ∥
  P4.D242 ∥ P4.D243 ∥ P4.133 ∥ P4.134):** `aa92cf91c` (files bug 173) and
  `a67a282c6` (the prompt-trust / anti-committee spec) NO-PORT-RATIFIED
  (P4.D240 — on the file lists: `docs/CHANGELOG.md`, `docs/developer/bugs.md`
  + the new bug file for the first; `.claude/commands/update-documentation.md`,
  `docs/CHANGELOG.md` + the new spec for the second; neither touches `lib/`,
  `app/`, `packages/`, `plugins/`, `components/`, `help/` or a test; both
  mirrored into `docs/v4/` at `ca363178d` bytes). `ddf942635` (bug 173: raw
  CLI SQL decodes compressed text) ABSORBED(P4.D240 — the scoped
  `nodefmt::raw_sql_cell_to_js_value` at the ONE v5 site, the four other
  `cell_to_js_value` callers untouched, Tier R 266 → 271 with v4's
  integration-test shapes, option (b) for the table-mode seam with four named
  divergences pinned; `--repl` stays the refusal with its decode site
  banked). `ca363178d` (the anti-committee phases) ABSORBED(P4.D241 the 21
  prompts re-vendored + the five trust directions at eight generator sites,
  `META_SYSTEM_PROMPT` a `LazyLock<String>`; P4.D242 the memory-consent
  prompts through the extended generator + the two-arm transcript heading;
  P4.D243 the chained-turn scene note as a pure module + its `build_context`
  wiring; help P4.D240 — the whole tree at `ca363178d`). P4.133 (the
  profile's own API key on every model call) and P4.134 (boot hardness)
  landed beside them as the two dogfood orders. Round record: `status-log.md`.

- **The `97b25fc53` seven-commit drift catch-up + refusal-seam round
  (2026-09-29, baseline `acadcc7cd` → `97b25fc53`; P4.D234 ∥ P4.D235 ∥
  P4.D236 ∥ P4.D237 ∥ P4.D238 ∥ P4.D239 ∥ P4.118):** `39bc98ffc` (`read_mail`
  + the `list_email` → `list_mail` rename) and `12c336fad` (`discard_mail`)
  ABSORBED(P4.D234 — ONE `resolve_mail_path` parser + a NEW tier-1 family,
  the rename with no alias, both tools through the existing chokepoints, the
  catalogue 59 → 61, the destructive list, letters by file name; help
  P4.D238). `f7f3d7bf0` (warm embeddings + the `chats.renderedMarkdown` DROP
  + the on-demand render) ABSORBED(P4.D235 the server — the tolerate-both-
  shapes read, the third D23 re-dump, 41 pairs narrowed through v4's real
  migration, ONE render + ONE status derivation, the cold-tier reversal —
  and P4.D236 the client; help P4.D238). `9ff4bbd8e` (the wardrobe outfit
  proposal) NO-PORT-RATIFIED(P4.D238 — every code hunk lands on the vision
  image-analysis vertical v5 never ported: `api/wardrobe.rs:1191-1205` is the
  P4.9f1 refusal arm, the SPA ships no import-from-image entry, no v5 file
  holds the vision prompt, `activity_span_sites_guard` holds the refusal row;
  `help/wardrobe.md` byte-copied; the vertical stays BANKED by pointer in
  `work-orders/surveys/2026-09-28-97b25fc53/survey-wardrobe-9ff4bbd8e-no-port.md`).
  `c3eefa752` (the 21 built-in prompts + the seeder's lazy refresh + the
  voice direction) ABSORBED(P4.D237; help P4.D238). `04d6c9d52` (the
  join-avatar refresh) ABSORBED(P4.D238 — plus the whole `help/**` tree at
  `97b25fc53`, the `caller_context` rider). `97b25fc53` (the story-background
  drape) ABSORBED(P4.D239; help P4.D238). P4.118 closed the `acadcc7cd`
  round's escalated text-side refusal seam. Round record: `status-log.md`.

- **The `acadcc7cd` Concierge-overhaul drift catch-up round (2026-09-28,
  baseline `b0b6656b5` → `acadcc7cd`; P4.D225 → P4.D226 → P4.D227 → P4.D228
  ∥ P4.D229 ∥ P4.D230 ∥ P4.D231 ∥ P4.D232 ∥ P4.D233):** `8bd080267` (#73,
  refusal-driven failover) + `49059fb14` (#74, the refusal ledger +
  auto-switch) ABSORBED(P4.D225 — the classifier, the understudies, the
  image-failover chokepoint, the text failover's refusal branch, the ledger
  as a boot ensure; ⚠ the TEXT-side structured refusal stays an unplugged
  seam, named in the order's header; the client half P4.D229). `4d370a90f`
  (#75, the three states) ABSORBED(P4.D226 — the trio, the flip, the
  switches, the first D23 re-dump, 41 + 1 pairs widened through v4's real
  migrations; the client half P4.D229). `3b463d6b1` (#76, the Concierge's
  own settings + the `conciergeOverride` DROP) ABSORBED(P4.D227 — the policy
  resolver + the consumer sweep, the second re-dump; the client half
  P4.D230). `ce2f1dabf` (#77, "Try uncensored") ABSORBED(P4.D228 — the two
  retry verbs, the Lantern's refusal bubble; the client half P4.D229).
  `08c49319d` (the Scenario Builder on the shelves) ABSORBED(P4.D231).
  `acadcc7cd` (bugs 171/172) ABSORBED(P4.D233 the server; P4.D228 the help
  pages). `6d0f88d65` (the dependency sweep) NO-PORT-RATIFIED for code +
  ABSORBED as a REGEN EVENT (P4.D232 — the guard's constants, `request-
  envelopes` re-recorded stamp-only, re-verified at unification; P4.D225's
  `image-dialects`). `83d0c969b` (bug 170) NO-PORT-RATIFIED(P4.D232 — v5 not
  affected, measured: the speaker pick is a dispatch verb). `a8292547a` (the
  overhaul specs) NO-PORT-RATIFIED(P4.D232 — docs; mirrored at `acadcc7cd`'s
  bytes). Round record: `status-log.md`.

- **The `b0b6656b5` ten-commit drift catch-up round (2026-09-25, baseline
  `d1c06cd9d` → `b0b6656b5`; P4.D220 ∥ P4.D221 ∥ P4.D222 ∥ P4.D223 ∥
  P4.D224):** `ad1c4c37f` (the ONE `dispatchAction` primitive) ABSORBED(P4.D220
  the web edges + core dispatch — `query::dispatch_action` /
  `dispatch_required_action`, the folding `query::action()` DELETED, every
  consumer's FULL v4 list in v4's order, the four data-affecting arms
  red-first [a bare `?action=` had DELETED a chat, RUN a restore, UPLOADED a
  file, CREATED a rule], the unlock edge's awaited catch, the two WARNs at
  v4's field names, `query_param_semantics` re-recorded with `fold` false
  everywhere; P4.D221 the `lib/` riders — the two mount-index partition keys
  red-first, the Brahma `qt_text()` sentence regenerated, `escapeLikeLiteral`
  NEUTRAL, six dead v5 twins retired with their oracle rows). `68da64d9b` (the
  chat GET's fourteen keys) ABSORBED(P4.D220 — gated BEFORE any chat read;
  the absent arm's pointer pinned both ways). `944127d9a` (the
  `?action=has-dangerous` removal) ABSORBED(P4.D220 the server half —
  `Request::ChatsHasDangerous` retired end to end; P4.D223 the client half —
  the probe, the three-way gate and the contract member deleted, the section
  ALWAYS offered). `492771aff` (bugs 167/168) ABSORBED(P4.D222 — the HELP_DOC
  job by SECTION with `average_embeddings` [a NEW tier-1 family, f32
  bit-exact], `count_by_doc`, the sync shape by id, `reconcile_help_docs` +
  the once-per-boot `HelpDocReconcileGate`, the boot order 3.66 before 3.7,
  `help/` 127 → 129 whole; **bug 168 IS dogfood #120 — CONVERGED by v4's
  mechanism**). `e3937d7aa` (the Salon Images quick-hide) ABSORBED(P4.D223 —
  `hideSalonImages` + the Salon-scoped `IMAGES_HIDDEN` token + v4's two
  stand-ins at every Salon image site; a v5-only XSS in the inline stand-in's
  string swap found and fixed at unification). `3376b3dfa` (the `@` mention
  typeahead) ABSORBED(P4.D224 — the pure module recorded against v4's real
  one [116 rows], the ProseMirror plugin, the composer wiring + §S.2; the char
  typeahead's soft-break defect found and fixed with it; P4.D222 the `help/`
  half; P4.D221 the Rust Carina neutrality re-run, 67/67 byte-identical).
  `b0b6656b5` (bug 169) CONVERGENCE-RETIRED(P4.D223 — v5's predicate made
  v4's exact twin, every "deliberate divergence" wording retired).
  `7ebb74143` (the release script) and `8aafd595d` (the dedicated-db
  repository base — its item (1) taken as P4.D221's Tier-2 small, the joined
  read's inner ERROR) NO-PORT-RATIFIED(P4.D221, on the file lists).
  `94e946728` (this port's bug-169 filing) NO-PORT-RATIFIED(P4.D223, docs
  only). Round record: `status-log.md` → "Round record — the `b0b6656b5`
  ten-commit drift catch-up round unification".
- **The `d1c06cd9d` Scenario Builder drift catch-up + maintenance round
  (2026-09-23, baseline `00c290c9a` → `d1c06cd9d`):** `d1c06cd9d` (the
  Scenario Builder) ABSORBED(P4.D216 the substrate — the shared
  `run_one_shot_tool_loop` with v4's five new debug lines, `DocToolsMode` +
  the extras bag + the third `search` variant, the stream call's `log_type` +
  `SCENARIO_BUILDER`, the pre-built mount pool through the executor / search /
  doc-edit / path resolver, `groupList { characterIds }`; P4.D217 the service,
  the three verbs + `scenarioBuilderProgress` frames, the host driver, the SSE
  edge, `help/` 126 → 127; P4.D218 the SPA dialog + save dialog + both entry
  points + the log labels, beats LIVE at unification; P4.D219 bug 165 — v5 had
  it verbatim; bug 166 NO-COUNTERPART, the modal never ported). `dff00e98d`
  (the handoff spec) NO-PORT-RATIFIED(the same round — docs-only by `git show
  --stat`; its file is mirrored in `d1c06cd9d`'s final text). Recorded
  divergence: `curlConfigured` always false (v5 builds no plugin tools).
  Round record: `status-log.md`.
- **The `00c290c9a` bug-163/164 drift catch-up + maintenance round
  (2026-09-23, baseline `a2db63da7` → `00c290c9a`):** `00c290c9a` (bugs
  163/164, the auto-title chokepoint) ABSORBED(P4.D215 — NEW
  `services/auto_title.rs` with v4's four outcomes, the post-LLM re-read, the
  refused arms' `extraPatch`-only write, the enqueue MOVED in with its lines
  re-prefixed; under the fold [its hand-renamed early return + its catch], the
  title-update job [the removed `Updated title` line; an unchanged title queues
  nothing] and regenerate-title [`clearManualRename`; v4's one outer catch over
  every read, fixed at unification]; `help/story-backgrounds.md`;
  `chat-admin-main.db` widened; the two bug mirrors + `bugs.md` and the
  bug-146–154 mirror lag by the unifier). Round record: `status-log.md` →
  "Round record — the `00c290c9a` bug-163/164 drift catch-up + maintenance
  round unification".

- **The `a2db63da7` bug-161/162 drift catch-up + maintenance round
  (2026-09-23, baseline `f45a517a9` → `a2db63da7`):** `4e1a8e061` (docs:
  bugs 161/162 filed + the summarizer-name design) NO-PORT-RATIFIED(P4.D212,
  mirrors — `context-summary-speaker-names.md` at its FINAL path, both bug
  files, `API.md`, `bugs.md`); `e7821606f` (bug 161 + the PORT-NEW
  rebuild-summary) ABSORBED(P4.D212 ∥ P4.D213 — ONE `speaker_names`
  resolver shared by the fold and the episode pass, the fold turn's required
  `speaker`, `FOLD_SUMMARY_PROMPT` regenerated mechanically, the debug line,
  `Request::ChatRebuildSummary` with v4's 409/400 order, the single update,
  the enqueue at priority 0, the `chats` publish, `help/chats.md`; the SPA's
  Organize entry, confirm and toasts, the beat run LIVE at unification —
  `orchestrator_tier3`'s `lastTurnParticipantId` CONVERGENCE pin retired:
  v4 persists the write from this commit on); `a2db63da7` (bug 162)
  ABSORBED(P4.D214 — the ONE CLI opener with v4's per-target strings, Tier R
  244 → 266 at both pins, P4.D210's live rows as canned-stub rows, the
  `packages/quilltap` README mirror). Round record: `status-log.md` →
  "Round record — the `a2db63da7` bug-161/162 drift catch-up + maintenance
  round unification".

- **The `f45a517a9` thirteen-commit drift catch-up round (2026-09-22,
  baseline `baa85e19b` → `f45a517a9`):** `186eb09cb` (bugs 159/160, the
  codec) ABSORBED(P4.D203 ∥ P4.D209 — the brotli codec + `qt_text` UDF on
  every connection, byte-parity with v4's real module; bug 160's third
  column; bug 159's image half as a seam wired ONLY into the sync applier —
  the un-wired sites are OPEN by name); `f45a517a9` (FTS5 + the four
  compressed columns) ABSORBED(P4.D203 → P4.D204 — the five FTS objects
  verbatim, the boot reconciler, `fts_query`, the two SQL shapes, the
  snippet fold, the `.`-defect retired); `80a05a4c8` NO-PORT-RATIFIED(P4.D203,
  mirror); `781e3b499` NO-PORT-RATIFIED(P4.D205, mirror); `e7d77bb60` (Inform)
  ABSORBED(P4.D205 ∥ P4.D206 — the table + D23 + repository + prompt block +
  consumption + the three verbs + export/import/backup/restore; the SPA
  dialog/chips/gutter; seven server items OPEN by name in the order header);
  `f564b0de3` (the streamed swipe) ABSORBED(P4.D207 ∥ P4.D206 — the watched
  stream, four beats, the frames verbatim incl. `kind`, the SSE edge; the
  SPA plate and bug-(b)/(c) fixes, the (b) fix repaired at unification);
  `0ecc6067c` NO-PORT-RATIFIED(P4.D210, mirrors); `23da0b322` (`quilltap
  sync` + bugs 155/156) ABSORBED(P4.D209 → P4.D210 — the repo-layer
  contract T; the nine engine modules, `Request::MountSync`, the CLI verb,
  Tier R 223 → 244; the live Tier R rows OPEN by name); `0c14fd61f` (bug
  157) ABSORBED(P4.D209 — `link_id` required, every caller); `5e4252898` +
  `e11a51f44` NO-PORT-RATIFIED(P4.D211, mirrors incl. the widened
  `docs/v4/packages-quilltap-README.md`); `6b0615807` (the npm update)
  ABSORBED(P4.D211 — Zod 4.6.5 measured neutral, `is_zod_email` converged
  at tier 2, the nine recorders re-recorded: two SDK stamps moved and
  nothing else); `da9c4f34f` (bug 158) ABSORBED(P4.D208 — the seed
  deleted, ONE predicate, the greeting block, the Concierge arms, the boot
  heal). Round record: `status-log.md` → "Round record — the `f45a517a9`
  thirteen-commit drift catch-up round unification".

- **The `baa85e19b` bug-154 default-system-prompt drift catch-up +
  maintenance round (2026-09-18, baseline `89fcc3c0d` → `baa85e19b`):**
  `baa85e19b` (bug 154) ABSORBED(P4.D201 ∥ P4.D202 — server: the
  `quilltap_core::default_system_prompt` resolver home (tier-1 exact over a
  22-case committed corpus against v4's REAL module) folded into the chat
  initializer (the `??`-on-content change) and the impersonation voice
  preview (which had reproduced v4's PRE-fix stale-column chain verbatim);
  `systemPromptsPatch`'s twin `project_system_prompts` so all four
  system-prompt writers move the `isDefault` flags AND the
  `defaultSystemPromptId` column in ONE patch, with v4's transient-id null
  rule; `set_default_system_prompt(Option<&str>)`'s clear arm; the
  `characterUpdate` chokepoint route (the pull-out, the empty-payload rule,
  v4's 400 after the generic write — and, at unification, the uuid half of
  v4's `z.uuid()` gate, red-first); the arrays family's per-op
  `defaultColumnTrail`; seven `characters_mutations` arms; the widened
  `characters-{main,mount}.db`; `help/character-system-prompts.md` at 124
  files. SPA: the client-safe twin over v4's five vectors verbatim, the two
  broken New-Chat seeds red-first, the announcement dialog's neutral fold,
  the star's optimistic cache write with rollback-then-refetch, the 4xl
  dialog, the live star beat (`P4D201_SERVER_LANDED` flipped at
  unification). The non-lib files: the two new test files + the PUT route
  test → the corpus; the help page re-vendored; README/CLAUDE/bugs/stamps
  NO-PORT.) Round record: `status-log.md` → "Round record — the
  `baa85e19b` bug-154 default-system-prompt drift catch-up + maintenance
  round unification".

- **The `89fcc3c0d` opacity-covenant drift catch-up + follow-ups round
  (2026-09-18, baseline `bcd7e4852` → `89fcc3c0d`):** `1065a1f53` (bug 152)
  + `89fcc3c0d` (bug 153) ABSORBED(P4.D200 — the `hide_character_vaults`
  flag on `PathResolutionContext` set by both builders which keep
  `character_id`; the tiered pool's `FlattenOptions { include_character_tier }`;
  the collector's two `vaults_visible` uses; the self-token gate's new
  conjunct; `find_enabled_mount_point_by_ref` + the NOT_FOUND → ACCESS_DENIED
  split with v4's three-sentence message and the two `vaultsHidden` warns +
  `describe_characters`; `AccessibleMountPointsQuery` on
  `get_accessible_mount_points` derived at all four enumeration call sites;
  the five pre-existing absent path-resolver/opacity-helper warn lines; the
  NEW real-DB `doc_opacity_equivalence` family over `build-doc-opacity-
  fixture.ts` mirroring v4's 13 + 15 regression cases, red-first 21/44;
  `help/character-system-transparency.md` re-vendored; the non-lib files
  ratified on P4.D200's list — `lib/doc-edit/index.ts`'s two type re-exports
  NO-PORT (v5's items are `pub`), the two `__tests__` files → the corpus,
  README/package stamps NO-PORT, the four docs mirrored under `docs/v4/`).
  Round record: `status-log.md` → "Round record — the `89fcc3c0d`
  opacity-covenant drift catch-up + follow-ups round unification".

- **The `bcd7e4852` bug-151 drift catch-up + follow-ups round (2026-09-17,
  baseline `5f0a57dc4` → `bcd7e4852`):** `bcd7e4852` ABSORBED(P4.D198 — the
  loader half: `files/llm_image_budget.rs`, the fallible `ImageTranscoder::
  shrink_to_webp` + `HostImageCodec`'s impl, the shrink ahead of the backstop
  at both loaders in `services/chat_files.rs`, the NEW `llm_image_budget_
  equivalence` tier-1 family with `sharp` scripted below v4's real function,
  `file_attachment_tier3` grown; P4.D199 — the walk half: the per-turn budget
  in `services/message_context.rs` section K over the seam's `LanternLoad`,
  five `orchestrator_tier3` arms, `help/connection-profiles.md` re-vendored,
  the commit's non-lib files ratified on P4.D199's list — v4's CLAUDE.md rule
  line NO-PORT, the README/package stamps NO-PORT with Tier R at the pin, the
  three docs mirrored under `docs/v4/`, the unit test → P4.D198's corpus).
  Round record: `status-log.md` → "Round record — the `bcd7e4852` bug-151
  drift catch-up + follow-ups round unification".

- **The `53294163f` GPT-Image-2.5 drift catch-up + maintenance round
  (2026-09-17, baseline `1fefadb9a` → `5f0a57dc4`):** `d8d2890ee`
  ABSORBED(P4.D196 — the OpenAI image capability table as one module
  [`model/openai_image_models.rs`, eight families, exact-then-longest-prefix],
  the OPENAI dialect rewritten over it red-first against the `image-dialects`
  corpus re-recorded at the target [97 → 150 rows; exactly the seven predicted
  pre-existing rows moved], the per-model options schema [`model/
  openai_image_options.rs`] through the existing `options-schema` action, v4
  bugs 148 + 149 in `generate_image.rs` [v5 measurably had both], the tool
  definition's bytes, the ONE quality list [`image_gen/quality.rs`] at the tool
  schema and the `?action=generate` arm, the eight-model manifest, the
  openai-SDK wire re-check [every other recorded corpus byte-identical] ∥
  P4.D197 — the offline fallback list in v4's CLIENT order, the modal header's
  recorded divergence, a recorded copy of v4's REAL `getOpenAIImageOptionsSchema`
  rendered in a 28-test spec + the live beat, the two `help/` pages
  byte-copied [124 stays 124]); `53294163f` and `5f0a57dc4`
  NO-PORT-RATIFIED(P4.D197 — both `--name-status` lists in the lane record;
  bug 150's client fix lands on the image-UPLOAD dialog v5 never ported, and
  v5's two in-chat generate dialogs post through the dispatch client, never a
  REST path, so the class is unreachable by construction). Round record:
  `status-log.md` → "Round record — the `53294163f` GPT-Image-2.5 drift
  catch-up + maintenance round unification".
- **The `1fefadb9a` bug-147 drift catch-up + maintenance round (2026-09-16,
  baseline `2075242f9` → `1fefadb9a`):** `1fefadb9a` ABSORBED(P4.D195 — the
  chat GET projects `spokenThisCycleParticipantIds` + `cycleOrderParticipantIds`
  as RAW JSON strings at v4's position with v4's `?? '[]'` shape, the P4.D171
  both-directions omission pin tripped at the target on all five `get_*`
  cases and retired, the corpus grown with the raw-`''` and raw-NULL arms;
  the SPA's dormant chat-GET seed made LIVE and grown its spoken half [the
  sidebar's `spoken` status was unreachable in v5 exactly as v4's filing says
  of v4], the P4.D187 refetch interplay and the busy-flip re-seed both pinned;
  the `state.cycleOrder` client read MEASURED as a convergence over a
  four-row table [v5 has read it since P4.D177]; v4's client/server agreement
  room as tier-1 corpus rows over the REAL `selectNextSpeaker` incl. the
  post-post half, which also opened and closed a blind spot — nothing had
  ever driven the rotation argument of `selectNextSpeaker` /
  `calculateTurnStateFromHistory` before; `help/chat-turn-manager.md`
  byte-copied, 124 stays 124; the non-lib files ratified NO-PORT on the
  `--name-status` list in the lane record). Round record: `status-log.md` →
  "Round record — the `1fefadb9a` bug-147 drift catch-up + maintenance round
  unification".
- **The `2075242f9` bug-145/146 drift catch-up + maintenance round
  (2026-09-16, baseline `ffb6b3119` → `2075242f9`):** `23abc1ba1`
  ABSORBED(P4.D192 — bug 145's migration hunk: `drop_victim_roll_link` over
  the three existing chokepoints, `gc_orphaned_file_row` widened to v4's
  per-table counts, the in-pass `protectedKept` census + `keptClause`, the
  family 17 → 24 scenarios with the album cases RED-FIRST [v5 measurably had
  the bug] ∥ P4.D194 — the bug-144 CONVERGENCE measured then retired
  [FOUR Tier R cases moved, not five; BOTH lines moved], the
  `help/database-protection.md` re-vendor, the non-lib files ratified
  NO-PORT on `--name-status` lists), `2075242f9` ABSORBED(P4.D193 —
  `resolveFloorSeatId` on both sides under the NEW tier-1
  `floor_seat_equivalence` [52 → 54 rows], the Salon banner re-keyed on the
  floor with v4's fourth sentence, a live two-user-seat beat ∥ P4.D194 —
  `help/chat-turn-manager.md`), `064ba85df` NO-PORT-RATIFIED(P4.D194 — bug
  143 filed, docs-only), `81e02f7a2` NO-PORT-RATIFIED(P4.D194 — bug 144
  filed, docs-only). Round record: `status-log.md` → "Round record — the
  `2075242f9` bug-145/146 drift catch-up + maintenance round unification".
- **The `ffb6b3119` bug-141 + bug-142 drift catch-up round (2026-09-15,
  baseline `31436bae4` → `ffb6b3119`):** `f90144ac4` ABSORBED(P4.D189 — the
  stream watchdog as a substrate commit [`model/stream_watchdog.rs` +
  `StreamError`'s `Stalled` kind with v4's message bytes], the TEN Salon-side
  wrap sites held by a per-file census [v4 has one funnel, v5 none], the
  `LLMStreamStalledError` → `network` classifier arm through all four classify
  sites with red-first `fallback_engine` rows + five `primary_stream_tier3`
  stall arms, the neutrality sweep at both pins ∥ P4.D190 — the greeting
  wrapped at 90 s / 60 s, the ladder's own-profile gate at v4's three
  positions with the desk scoped out, v4's three attempt warns + the
  exhaustion line the v5 arms never carried, `initial_greeting` +
  `chat_create_capstone` red-first over the ordered `stream_calls` comparand
  [the jest `instanceof`-across-`resetModules` trap found and fixed on both
  oracles] ∥ P4.D191 — the two `help/` files byte-copied, 124 stays 124);
  `ffb6b3119` ABSORBED(P4.D191 — the CONVERGENCE measured TOTAL [1 of 1,513
  cells moved], `DELETE_MISS_DIVERGENCE` retired to a plain equality
  red-first in both directions with zero v5 source change; the §3 review
  added a presence pin for the converged chat); `85813ddd2` and `364b04ac4`
  NO-PORT-RATIFIED(P4.D191 — file lists in the lane record; Tier R 223/0 at
  the pin; the main checkout's binding measured ABI-matched to Node 24).
  Round record: `status-log.md` → "Round record — the `ffb6b3119` bug-141 +
  bug-142 drift catch-up round unification".
- **The `31436bae4` drift catch-up round (2026-09-15, baseline `f4ad2c8d1`
  → `31436bae4`):** `5029075bb` ABSORBED(P4.D182 substrate — the
  `chats.transcriptVersion` boot ensure, the eight-file `help/**` re-vendor ∥
  P4.D183 server — the funnel's one announce point at v4's six conditions,
  the projection extraction proven neutral, the `chatTranscript` verb + the
  `/api/v1/messages` GET edge, the chat-GET key; a v4 bug found and pinned
  both directions, since filed and fixed as bug 142 ∥ P4.D187 SPA —
  `reconcileTranscript` over a recorded corpus, the subscribed read, the
  bubble inside the array [dogfood #106's ground]); `7fbf8a55b`
  ABSORBED(P4.D182 substrate — `files.generationKey` through the D23 re-dump +
  the column/index ensure, the carry through every read/write/export/import
  surface, the export-schema re-vendor ∥ P4.D184 server — the key-derivation +
  lookup chokepoint over a new tier-1 corpus, lookup-before-spend, `force`,
  vault-always, the collapse heal under v4's ledger id over a new tier-2
  family; the §3 review landed the five unpinned log lines); `4dcbe0d21`
  ABSORBED(P4.D185 server — the album predicate, the rolls service, three
  verbs + two REST sub-routes, a new committed `avatar-rolls-*` pair; the §3
  review caught a swap-remove reaching `chats.characterAvatars` ∥ P4.D188 SPA
  — the Avatar Rolls section in the gallery tab); `055cac45a`
  ABSORBED(P4.D188 — the "Show shared" tickbox); `8275b3642`
  ABSORBED(P4.D187 — bug 136's two sentences at v5's measured sites; bug 135
  NO-COUNTERPART, measured); `31436bae4` ABSORBED(P4.D186 server — the
  `paused_hold` predicate consulted once at the record → prepare-turn seam,
  the three `!hold` conjuncts each pinned with the others open,
  `finish_held_user_turn`, `heldUserTurn` on the frame ∥ P4.D187 client — the
  unpause-first legs deleted, the once-per-pause toast, bug 139's resume-then-
  ask, bugs 138/140 measured); `4dc48283d` and `aecf9de0b`
  NO-PORT-RATIFIED(P4.D182 — two `docs/` files; four version markers; the
  evidence in the lane record). Round record: `status-log.md` → "Round record
  — the `31436bae4` drift catch-up round unification".
- **The `f4ad2c8d1` In-Their-Own-Words drift catch-up round (2026-09-11,
  baseline `cc65d6bfc` → `f4ad2c8d1`):** `686954937` ABSORBED(P4.D179 the
  `chat_settings."impersonationVoiceRewrite"` column through the D23 re-dump
  + a boot ensure + the route arm, the `VOICE_REWRITE` log type + both
  `mapTaskTypeToLogType` arms — v5 had filed `announcement-rewrite` as
  `SUMMARIZATION` since it was ported — the Almanack row, `help/**` 123 →
  124 ∥ P4.D180 the `voice_rewrite_core` extraction proven neutral at both
  pins, the new `in_scene_voiced` service, the `chatImpersonationVoicePreview`
  verb, the LIVE host wire, a new committed pair + 27-case tier-3 family ∥
  P4.D181 the whole SPA half); `f4ad2c8d1` ABSORBED(P4.D181 — bug 134
  measured then ported: v5 never had the mount-only snapshot, the live-read
  facts pinned structurally, the memory-cascade remember arm + its
  invalidation landed, the `types.ts` consolidation NO-COUNTERPART; its
  `help/settings.md` hunk rode P4.D179's re-vendor). Round record:
  `status-log.md` → "Round record — the `f4ad2c8d1` In-Their-Own-Words drift
  catch-up round unification".
- **The `cc65d6bfc` bug-133 catch-up + `78b381a96`-round remainders round
  (2026-09-10, baseline `78b381a96` → `cc65d6bfc`):** `cc65d6bfc`
  ABSORBED(P4.D178 — bug 133 whole: the sanitizer's fourth parameter
  re-meant as "does THIS scene route uncensored", the story reroute barred
  for a moderated chat with the candid re-craft and v5's `RerouteRecraft`
  seam deleted, the six reroute-path log lines both handlers were missing,
  the story corpus's two moderated reroute rows red-first + two new arms, the
  NEW `appearance_sanitize_gate_tier3_equivalence` family over v4's real
  sanitizer, `image_generation_tier3` widened with a DETECT_ONLY case,
  `help/dangerous-content.md` re-vendored). Round record: `status-log.md` →
  "Round record — the `cc65d6bfc` bug-133 catch-up + `78b381a96`-round
  remainders round unification".
- **The `78b381a96` twelve-commit drift catch-up round (2026-09-10, baseline
  `25f534c0b` → `78b381a96`):** `5841a8c62` ABSORBED(P4.D171 substrate →
  P4.D173 server ∥ P4.D177 SPA — the message route trail: the column through
  every surface, the ONE recording chokepoint + twelve record sites + the
  three empty-response arms v5 lacked, persistence + the `done` frame, the
  64-row compose family, the badge under the avatar); `2aca73ad6` +
  `d14da3a56` ABSORBED(P4.D171 substrate → P4.D172 server ∥ P4.D177 SPA — the
  cycle's drawn rotation as an ORDERED draw source, the six selection sites
  over the whole-room batched map [bug 131 — v5 measurably had it], the
  strike, `?action=turn`'s `state.cycleOrder`, the participants-list
  rotation; the §3 review fixed the finalizer's `{id,name}` preloaded stub);
  `86d59660c` ABSORBED(P4.D174 server ∥ P4.D176 SPA — the Salon chat gallery
  whole, `?download=1`, bugs 129/130; the §3 review flattened the 409's
  riders); `4a9be9878` ABSORBED(P4.D175 server ∥ P4.D177 client — bug 128's
  `memories` topic; bug 127 a convergence record); `78b381a96`
  ABSORBED(P4.D175 — bug 132's writers, the prompt-first ladder, the boot
  heal + ledger row, `help/**` at 123); `07eee4f4c`, `9fc664c94`,
  `5fb6bedd6`, `df1a075e8`, `c0f9232af`, `d3f0ed133` NO-PORT-RATIFIED(P4.D175
  — docs/version-only, with the evidence in the lane record; the twelve
  retired specs MOVED in the `docs/v4/` mirror at unification). Round
  record: `status-log.md` → "Round record — the `78b381a96` twelve-commit
  drift catch-up round unification". The mid-round `cc65d6bfc` (bug 133)
  stays in §3 UNPROCESSED.
- **The `25f534c0b` progressions + bug-126 drift catch-up round (2026-09-09,
  baseline `2f4254b42` → `25f534c0b`):** `0587d1e96` ABSORBED(p4.d167 +
  p4.d168 + p4.d169 + p4.d170 — the whole character-progressions feature:
  the pure engine tier-1 exact over a 555-row committed corpus [the U+202F
  prediction refuted, `toFixed` half-up pinned, the abort-suppresses-refine
  rule measured and fixed at unification]; the prompt path — the chokepoint,
  `find_last_own_turn_ms`, the ONE memoised cadence read, the trailing section
  after Suparṇā's mail and before the turn-skip note, the forced greeting and
  Carina reports, the negative cache guarantee, the seven `help/` files
  re-vendored [121 → 122]; the Pascal `progress` family end to end — read
  subject, `{{now}}`, the effect target with create-on-write / normalise /
  post-validation rollback, the vocabulary's three keys, one clock per run,
  the NEW `pascal_side_effects_equivalence` family, six corpora widened from
  zero, the committed `pascal-run-custom-*` pair rebuilt [`shift_remove` at
  three applier sites landed at unification — key order reaches disk]; the
  SPA half whole — the client-safe twins over an extracted Zod shim, the
  Progressions card + editor modal, the Workbench affordances, the run popup,
  both `public/schemas/` vendors GUARDED, both gated beats flipped LIVE);
  `25f534c0b` ABSORBED(p4.d166 — bug 126: the ownership snapshot [PID +
  `startedAt`, keyed by lock path], the heartbeat-freshness cascade for every
  environment, the renamed-process release, the loss teardown made ordered,
  the CLI's shared `assess_lock` with Tier R 216 → 223/0; `lock-helpers.js`
  UNTOUCHED by v4, so the write lock and the launcher's classifier keep the
  hostname comparison); `d307a4164` NO-PORT-RATIFIED(p4.d167 — the design of
  record, mirrored to `docs/v4/developer/features/character-progressions.md`
  at unification) and `4097626c6` NO-PORT-RATIFIED(p4.d166 — version bump,
  four files, no comparand). Round record: `status-log.md` → "Round record —
  the `25f534c0b` progressions + bug-126 drift catch-up round unification".
- **The `2f4254b42` character-subprompts round (2026-09-07, baseline
  `f699da6f6` → `2f4254b42`):** `2f4254b42` ABSORBED(p4.d163 + p4.d164 +
  p4.d165 — the whole feature: the participant `selectedSubpromptIds` carry
  [a measured v5 data-loss fix landed first], the vault-backed `subprompts`
  module + fan-out, the five verbs + REST edges + realtime, the four `help/`
  files re-vendored, the `## Additional Instructions` block in the identity
  stack with NO builder-version bump, the compiler bake, the `build_context`
  fallback, the greeting, the green room at both entrances, and the whole SPA
  half with its walk live); `15573c3a1` ABSORBED(p4.9k1-resumed — bug 119's
  runner half: `run_sub_step` / `run_sub_step_core` containment + both log
  lines, capture-pinned). Round record: `status-log.md` → "Round record — the
  `2f4254b42` character-subprompts round unification".
- **The `f699da6f6` 4.9.x drift catch-up round (2026-09-06, baseline
  `c2232cd9a` → `f699da6f6`):** `fef7ce4f7` ABSORBED(p4.d160 + p4.d161 — bug
  123: the per-emit optional `paused` chain-complete key, the paused early-
  return with v4's info line, the two re-vendored help pages; the SPA's
  seat-keyed Skip banner, overlay-aware Skip with a silent unpause-first, the
  pause-you-did-not-cause toasts, v4's pause-sync drift a mutation-proven
  NO-COUNTERPART), `20913d2aa` ABSORBED(p4.d162 — bugs 124/125, on `main` by
  content through the 4.9.2 squash: the help loop through the tool-call
  threading primitive with the family's FULL-slate comparand and an id-less
  case; `additionalProperties` at the head of Google's strip list with the
  real wardrobe schemas in the recorded corpus; a GOOGLE seat in the help-chat
  fixture), `d40497411` / `5eaf98cf1` / `ba34fa367` / `02b77ab0f` /
  `8fbf2afe0` / `d489b04a3` / `f699da6f6` / `1a2b2164c` NO-PORT-RATIFIED(p4.d160
  — the two release cycles' branch starts, squashes, merge-backs, the bug-filing
  docs commit and the CHANGELOG → `CHANGELOG_V4.md` move; file lists in the
  P4.D160 lane record's "NO-PORT ratification evidence"; `docs/v4/` refreshed
  to byte-identity with `f699da6f6:docs/` on every shared path). Round record:
  `status-log.md` → "Round record — the `f699da6f6` 4.9.x drift catch-up round
  unification". `15573c3a1` (bug 119) stays in §3 for the unported `p4.9k`.
- **The `p4.9i2` help/HelpChat round (2026-09-05, baseline `d883a5ee1` →
  `c2232cd9a`):** `6cbe2b027` NO-PORT-RATIFIED(p4.77 — the final 4.9.0
  release notes; `docs/v4/releases/4.9.0.md` refreshed from it), `b0eea4642`
  NO-PORT-RATIFIED(p4.77 — the squash onto `release`; content diff against
  the baseline EMPTY under every ported path), `f6794c840`
  NO-PORT-RATIFIED(p4.77 — the merge back; four version/doc files),
  `c2232cd9a` NO-PORT-RATIFIED(p4.77 — the 4.10.0 dev bump; the
  `package-lock.json` hunk is the two version lines). Evidence: the P4.D159
  block in `status-log.md` → "Lane record — P4.77 unit 2". `15573c3a1`
  (bug 119) stays in §3 for the unported `p4.9k`.
- **The `d883a5ee1` drift catch-up round (2026-09-05, baseline `0b0617fee` →
  `d883a5ee1`):** `d883a5ee1` ABSORBED(p4.d153 — bug 122: the memory-subject
  prefix through the three self-facing formatters at v4's template positions
  and inside the token estimate, `find_names_by_ids` on the RAW path, the
  `memory_subject` resolver with the zero-query early return, the three call
  sites; the oracle case's positional arity fixed FIRST; corpus + tier-3
  fixtures widened with targeted memories), `e288ae2ec` ABSORBED(p4.d154 —
  bug 121: the USER-side attachment walk as a fourth `message_context_leaves`
  leaf with v4's ten cases, the re-hydration before `build_context` with the
  skip-whole budget and the `unsupported`-with-error drop, the
  `load_user_attachments` seam, the orchestrator corpus widened to SEE the
  splice), `0506517d3` ABSORBED(p4.d155 corrections (a)–(e),(g) server-side +
  the Pascal classifier on both sides ∥ p4.d156 client corrections (f) ∥
  p4.d158's neutrality sweep for the other 249 files; the two
  `screens/custom-tools/**` readers landed at the unification wire),
  `bbcb318c6` ABSORBED(p4.d156 — the two `qt-checkbox` attributes),
  `48f4b42ec` ABSORBED(p4.d158 — `^claude-opus-5(-|$)` in v4's position, two
  corpus rows red-first), `af2023c9a` ABSORBED(p4.d156 — bug 120, Tier R
  214 → 216/0 vs v4's REAL launcher), `e9a9c538e` ABSORBED(p4.d156 About hunk
  ∥ p4.d158 docs half — the `docs/v4/` mirror refreshed at the pin, the
  `?action=` rows read against v5, the §G help bank), `d4138b96b`
  ABSORBED(p4.d157 — thirteen symbols, all option (ii) DELETE — not one twin
  had a production caller; seven families SPLIT, none frozen; the LoRA
  bounds pinned against their new home), `b52b996c1` + `6e1a64ea6` +
  `06658535f` NO-PORT-RATIFIED(p4.d158 — every provider corpus regenerated
  at the pin: only `x-stainless-package-version` 7.4.0 → 7.10.0 and the
  openrouter user-agent moved; manifests byte-identical) **with one
  correction at the unification: `6e1a64ea6`'s `zod` bump DID move v5 bytes**
  — Zod 4.5.4 makes a strict object's `unrecognized_keys` issue continuable
  (core `schemas.js`, invisible to the locale diff both lanes took), so a
  `when: true | {…}` union with a stray key reports the object branch alone
  and its refines fire; both engine twins (`custom_tool_types.rs` /
  `custom-tool-types.ts`) fixed at the wire, `pascal_custom_tool_definition_
  equivalence` red-first then green (260
  definitions with two new astral-title rows), the SPA's committed corpus
  refreshed (301 rows) + nine hand-captured rows re-captured — **and the
  neutrality sweep then found the bump's SECOND rule, code-point string
  lengths, in three families' astral pins (fixed as one helper per check on
  both sides; the sweep's other reds were a fixture-vintage artifact on the
  characters pair, an oracle mock lagging the collapse, and a moved LoRA
  import — none `0506517d3`'s, which measured NEUTRAL over 402 families),
  `49f66f571` + `a0e6fb42a` + `2edd823c0` NO-PORT-RATIFIED(p4.d158 — hunk
  evidence; `2edd823c0`'s four bag-key blind spots landed as restore corpus
  arms over the new committed `restore-archive-bag-keys.zip`). Round record:
  `status-log.md` → "Round record — the `d883a5ee1` drift catch-up round
  unification".

- **The `0b0617fee` drift catch-up round (2026-09-03, baseline `6d2a50382` →
  `0b0617fee`):** `303288fb4` ABSORBED(p4.d148 server ∥ p4.d149 SPA — the
  create-time `conciergeState` through the existing `apply_concierge_flip`
  chokepoint on all three branches, the greeting ladder's attempt 0 on the
  uncensored desk asked WITH the chat row, the one shared desk closure; the
  New Chat dropdown + the omit-when-monitored body rule + the gated create-time
  beat flipped live at unification; Continue Elsewhere seeding recorded as a
  NO-COUNTERPART), `02d4efa1b` ABSORBED(p4.d150 — `distill_memory_search`
  takes the latency class, the fallback interactive, pinned at the real call
  sites by a budget-recording provider since the corpus is provably blind),
  `c9faa2c74` ABSORBED(p4.d150 — the inter-character timing debug line,
  capture-pinned three arms), `b448eddd7` NO-PORT-RATIFIED(p4.d151 — docs
  only, five files, zero lib/app/packages/plugins; its measurement
  obligations discharged by p4.d151 for bugs 116/118 and p4.d152 for 117),
  `0b0617fee` ABSORBED(p4.d151 bugs 116 + 118 ∥ p4.d152 bug 117 — the
  describer arrival verdict ahead of every content check with the
  `CompletionResponse.cache_usage` widening; the manifest regen proven
  byte-identical, v5 never had bug 118; the four bug-117 legs with the
  within-tree boolean comparand and the `realign-file-entry-sha256-v1` boot
  heal in the P4.D140 ledger shape, its presence-vs-drift stamp rule a
  RECORDED both-directions divergence). `15573c3a1` deliberately NOT swept
  (still §3). Round record: `status-log.md` → "Round record — the
  `0b0617fee` drift catch-up round unification".

- **The `6d2a50382` drift catch-up round (2026-09-02, baseline `4622411fd` →
  `6d2a50382`):** `70505745a` ABSORBED(p4.d146 — the presence gate at the
  three story-background sites incl. the reworded 400, the
  `backgroundDisplayMode` normalizer at the overlay parse + the narrowed
  update schema + the GET's dead arms deleted, the SPA card; the committed
  `cost-background` pair and the story builder widened so the gates can be
  seen), `a00e18f0d` ABSORBED(p4.d147 — v5 had NO folder picker; v4's
  post-fix `FolderPicker` built fresh over the existing verbs with the live
  beat), `a5df98b3f` ABSORBED(p4.d145 — the unique-constraint predicate,
  `ensure_by_path` over the seven create sites with the two private lookups
  deleted, the collapse-then-index boot ensure in the index-guarded idiom
  with NO ledger row, the restore quiet-drop arm + a new committed archive;
  the order's provisioning-hook suggestion REFUTED by measurement),
  `f3351d54f` NO-PORT-RATIFIED(p4.d143 — three files, zero lib/app),
  `c43d3b1b4` ABSORBED(p4.d143 server ∥ p4.d144 SPA — the derived
  `conciergeState`/`dangerCategories` pair on all four list payloads, the
  predicate delegation, the per-turn enqueue guard, the `has-dangerous`
  probe v5 never had; the presentation table once in the SPA, the mark, the
  pill, `shouldHideChat` as the one rule), `6d2a50382`
  NO-PORT-RATIFIED(p4.d143 — four version files). Round record:
  `status-log.md` → "Round record — the `6d2a50382` drift catch-up round
  unification".

- **The P4.D138 follow-up (2026-09-01, baseline unchanged at `4622411fd`;
  the LoRA train's three PARTIAL rows completed):** `84f33ce94`
  ABSORBED(p4.d138 units 1–4 + unit 6 ∥ p4.d139 — the read side landed:
  `list-models` `loraSupport`, the `options-schema` action, the NanoGPT
  detailed-catalog cache with the augmentation arm the unit-1 narrowing had
  named; the tripwire fired and was deleted; the two SPA beats live),
  `648d5c8aa` ABSORBED(p4.d138 unit 5 — bug 110's family-first `apply_loras`
  with the corpus re-recorded at the tip, exactly the two predicted rows
  moving; bug 111's error-level request log + v4's debug line, both
  capture-pinned), `2ece98c90` ABSORBED(p4.d138 unit 7 ∥ p4.d139 — the
  HuggingFace lookup + repo-id twins with a 57-row differential over v4's
  real modules, the `lora-metadata` action live behind an engine gate, the
  host transport; one recorded divergence, V8's own `SyntaxError` wording).
  Round record: `status-log.md` → "Round record — the P4.D138 follow-up
  unification".

- **The round-2 drift catch-up (2026-09-01, baseline `7fb668263` →
  `4622411fd`):** `5f56f7a7d` ABSORBED(p4.d142 — `.qt-range` + tokens
  byte-identical, all twelve v5 range hosts adopted, dogfood #107's
  `qt-markdown-field` rule, the host-class guard at the ordered NARROW
  scope + the `--self-test` landed at unification), `735d9408c`
  ABSORBED(p4.d140 — bug 112 whole: the `chat_activity` chokepoint with
  its tier-1 family, both write sites red-first, the six readers, the
  restore re-derive, the ai-import twin NO-COUNTERPART, the boot recompute
  heal in the P4.D97 ledger shape with its own family, the four SPA
  display flips, the e2e seed landmine repaired; plus the out-of-mandate
  `allowCheapFallback` P4.D135 remainder), `e41fcb12e`
  NO-PORT-RATIFIED(p4.d138 — CHANGELOG + two help files, +34, zero
  lib/app/packages/plugins; help hunks banked to `p4.9i2`), `4622411fd`
  NO-PORT-RATIFIED(the unify — `docs/releases/4.9.0.md` alone, +70/−6),
  `60e3c4a0a` ABSORBED(p4.d141 — the Concierge four-state whole: the
  predicate family reshape at every call site, the resolver's operator
  arms, the flips + writer sentences byte-exact, the `conciergeState` PUT
  arm closing v5's long-named deferral, the classifier-gate corpora that
  can finally see the gate, the SPA control + single-pill badge + client
  twin, the four-state walk live; the §3 review's sidebar-latch fix and
  the broken `post_office_writers_tier3` family repaired at unification).
  The LoRA train (`84f33ce94` → `648d5c8aa` → `2ece98c90`) is PARTIAL —
  the whole client half (p4.d139) + p4.d138 units 1–4; units 5–7 stay in
  §3 as PARTIAL rows and in the order's resume list. The §3 unification
  review (five parallel readers over six lanes) fixed six finding groups
  on the unify branch — headline: the Concierge select's permanent
  optimistic latch, the optimistic-bubble echo scoped across two clocks,
  and a harness family the kind rename had silently broken. Round record:
  `status-log.md` → "Round record — the drift catch-up round 2 of 2
  unification".

- **The round-1 drift catch-up (2026-09-01, baseline `b121ac77f` →
  `7fb668263`):** `1560bd43b` ABSORBED(p4.d134 — the Lima/WSL2 retirement
  whole: env/lock/CLI, the data-dir `isVM` wire deletion with two renamed
  deletion pins, the host-rewrite two-strategy collapse, self-inventory/
  almanack retirements, the SPA About/footer/profile mirrors; the grep
  census; the unported host gateway resolver named as a follow-up order),
  `7819afb1d` + `3c3432ae9` NO-PORT-RATIFIED(p4.d134, file lists in the
  lane record), `65f5021c8` ABSORBED(p4.d135 — provider/model fallback
  chains whole: the D23 re-dump with the generateDDL-position correction,
  the pure engine tier-1 at 155→158 cases, the Salon spine both
  entrances, cheap-LLM + image-description sites, both id-remap paths,
  the delete-nulls cascade with the updatedAt stamp, the SPA mirrors +
  the live understudy round-trip beat), `97ebfb9fc`
  NO-PORT-RATIFIED(p4.d136 — both bug files' v5-relevance columns quoted
  in the lane record), `a1d88aa3a` ABSORBED(p4.d136 — bug 106 proven
  red-first in both halves + the three-spellings consolidation; bug 107's
  budget rewrite with the latency class threaded from 45 call sites, the
  timeout-only retry, five of six handler guards [scene-state deferred
  loud], the C4 dogfood row superseded per §5.5; the outfit-consult
  bound inversion measured as v4's own and reproduced), `487ae16b1`
  ABSORBED(p4.d137 — bugs 108/109 red-first: the argument guards
  byte-exact, the fold module + rebuilt per-UTF-16-unit diacritics map,
  the 5/25 replay split executable), `7fb668263` ABSORBED(p4.d134's About
  rider; README/CHANGELOG_V4 remainder NO-PORT). The §3 unification
  review fixed four finding groups on the unify branch — headline: the
  `[CheapLLM] Task failed` warn fired AFTER the chain instead of before
  it (v4 warns first; the counter bug 107 was measured from would have
  under-counted). Round record: `status-log.md` → "The drift catch-up
  round 1 of 2 unification".

- **The P4.D131 round (2026-08-27, baseline `aec86a613` → `b121ac77f`):**
  `679e450e3` ABSORBED(p4.d131 — the bug-105 CONVERGENCE retired by
  measurement per §5.4: FULL convergence, no residue; the arm — code name
  `execute_bug105_seed_abort`, the row's `import_aborts_on_non_string_
  provider` was its class description — is now a plain state-compared
  regression guard; the retirement measurably WIDENED coverage, the
  formerly-subtracted `main.image_profiles` table now discriminating),
  `0bd841394` + `1b0ce9eba` ABSORBED(p4.d132 — the Tooltip primitive +
  nine-button adoption + the ConfirmationBadge net-new + the deletion
  rider; help rows banked to `p4.9i2`; theme-storybook NO-PORT),
  `b121ac77f` ABSORBED(p4.d133 — `instances restore-key` whole, Tier R
  188 → 212; the row's ⚠ scope question resolved at ordering — the write
  proved in-sandbox via `reset_live`, the real-pepper walk banked 💸; the
  NO-PORT remainder RATIFIED: README/docs/help/package files + v4's two
  new test files, their behavior carried by the Tier R arms + unit pins).
  Round record: `status-log.md` → "The P4.D131 ∥ P4.D132 ∥ P4.D133 ∥
  P4.65 round unification".

- **The P4.D130 round (2026-08-27, baseline `8872d7efc` → `aec86a613`):**
  `aec86a613` ABSORBED(p4.d130 — the outfit pull-down + garments-only slot
  pickers, SPA-only; the composed-outfits selectors' recorded-vector corpus
  pinned at the drift commit, re-proven byte-identical after mid-lane
  drift), `b6c6d7793` NO-PORT-RATIFIED(this round — docs-only, this port's
  own bug-105 filing; file list verified `docs/developer/bugs.md` + the bug
  file, zero lib/app/packages/plugins content). Round record:
  `status-log.md` → "The P4.D130 ∥ P4.62 ∥ P4.63 ∥ P4.64 round".

- **The 4.9.0-push round (2026-08-27, baseline `f3892158d` → `8872d7efc`):**
  `914b59e13` + `805ef12bf` + `e000d6bfc` ABSORBED(p4.d126 — the full-wipe
  chokepoint, the variable-limit chunking, bug 103's legacy profile-column
  seeding; a v4 REGRESSION found in `e000d6bfc` itself — the helper sits
  outside the per-item try, one malformed profile aborts a whole v4 import —
  pinned v5-side, filed upstream), `964ffb959` + `8872d7efc` + `21f573039`
  ABSORBED(p4.d127 — bug 104, the per-task cheap-LLM budgets + failure warn,
  the coalesce-trace silence pin; the §1-predicted 💸 expiry executed: the
  Z.AI refusal-sentence proof retired, replaced by the glm-5.3 wire proof),
  `97d0b8f8e` + `57e7b1bc2` ABSORBED(p4.d128 — the qt-* utilities sweep,
  the four completion flags Tier R red-first), `8440b6391` ABSORBED(p4.d128,
  the AboutView hunk) + NO-PORT-RATIFIED(p4.d129, the docs remainder),
  `487ae57fe` `561466cfe` `7509c5cfb` `c0352fdba` NO-PORT-RATIFIED(p4.d129,
  each with evidence; `561466cfe`'s rider removed one vestigial v5 wardrobe
  twin), and `dcab791c2` NO-PORT-RATIFIED(p4.d129 — 410-family neutrality
  sweep + hunk measurements) **EXCEPT its title-cleaner second-trim collapse,
  which was measured NON-NEUTRAL (10/76 vectors) and ABSORBED at the
  unification wires** (both v5 cleaners + the `regen_title_quoted_padded_
  inside` tier-3 arm). Round record: `status-log.md` → "The 4.9.0-push
  drift catch-up round".

- **`f3892158d`-round (2026-08-26, baseline `b220999da` → `f3892158d`):**
  `664cfca84` ABSORBED(p4.d123 server ∥ p4.d125 client — the jobs/activity
  accounting whole), `f3892158d` ABSORBED(p4.d124 server ∥ p4.d125 client —
  the realtime subsystem whole, the hints riding v5's existing Event
  channel per the round's §Shared contract §B rather than a second
  WebSocket). Round record: `status-log.md` → "The `f3892158d` drift
  catch-up round".

- **`b220999d`-round (2026-08-26, baseline `8f9101370` → `b220999da`):**
  `b86bb1a58` ABSORBED(p4.d119 server ∥ p4.d121 SPA — the per-tier
  dressing instructions whole), `d25dacc1d` ABSORBED(p4.d120 ∥ p4.d121 —
  archive-instead-of-delete whole), `b220999da` ABSORBED(p4.d122 — the
  Documents-search vertical whole), `a47d3e034` + `2417cbed1`
  NO-PORT-RATIFIED(the two feature specs — docs-only, confirmed by the
  implementing lanes' hunk surveys; the search spec's defects paragraph is
  quoted in the p4.d122 order). Round record: `status-log.md` → "The
  `b220999d` drift catch-up round".

- **`8f910137`-round (2026-08-25, baseline `f6a10055d` → `8f9101370`):**
  `44a8137e9` ABSORBED(p4.d115 ∥ p4.d116 — the scenario-change feature whole),
  `8018c487a` ABSORBED(p4.d117 — bug 99, measured-then-ported),
  `309aaa97a` ABSORBED(p4.d117 — bugs 100/102 + the check-qt-classes guard),
  `6afacb187` ABSORBED(p4.d118 — bug 101, Tier R red-first),
  `8f9101370` NO-PORT-RATIFIED(p4.d118 — CI + tests-only; the +18 test lines
  absorbed by `completion_behavior.rs`). Round record: `status-log.md` →
  "The `8f910137` drift catch-up round".
- (This ledger was seeded 2026-08-25 with the baseline at `f6a10055d`. Drift
  older than that baseline is recorded in CLAUDE.md's round bullets and
  `claude-md-status-history.md`.)
