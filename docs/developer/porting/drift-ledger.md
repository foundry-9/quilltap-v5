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

- **Oracle baseline: `4a38222e6`**: "Wardrobe folders — file items into a
  tier's closet (#85)" (v4 main, 2026-10-12 13:03, `4.10.0-dev.148`),
  adopted when the `4a38222e6` wardrobe-programme drift catch-up round was
  unified, all nine lanes landed (P4.D279 → {P4.D280 ∥ P4.D281 ∥ P4.D282 ∥
  P4.D283 ∥ P4.D284} ∥ P4.D285 ∥ P4.D286 ∥ P4.D287, 2026-10-13). CLAUDE.md's
  Status bullet for the round names `4a38222e6`. MOVED from `01a83539d` by
  this unification — the eight rows it passes retired to §6.
- **Checked:** 2026-10-15 (`/unify` of the `83334b9a5` round — the §2 probe FAILED at
  the start: v4 SEVENTEEN commits further, INCLUDING the `release: 4.10.0`
  squash and the `4.10.0` bug branch's start, and the checkout DIRTY with
  in-flight Pascal work; seventeen rows appended (§3), every one UNPROCESSED
  and PAST the round's pin, classified from the hunks by three parallel
  readers — ONE new D23 move among them, `362426c8f`'s
  `chat_settings.customToolTriggers`). Before that: 2026-10-14 (`/setupphase`, late in planning — v4 committed its
  dead-code dirt as `7417e9a1b` and landed `01f508d4e`; two rows appended,
  both UNPROCESSED, both PAST the `83334b9a5` round's pin). Earlier the same
  session: (`/setupphase`'s §2 probe FAILED mid-planning;
  §4 procedure run by hand for ONE commit: `83334b9a5` the API-standard
  audit — NO-PORT?, identical-output `successResponse` swaps — and the
  checkout's new dirt recorded). Before that: 2026-10-13 late evening (`/driftcheck` — six rows
  appended: `4ae0c4ca6` TF-IDF tuning + bug 243, `104e986c7` Phase F,
  `b045dd986` Phase G + bugs 242 / 244, `48790fb73` the refactor's close +
  **the Friday backfill record**, `9ec4fa2e9` the V4test dogfood, `64743ee13`
  bugs 245 / 246). Previous checks: 2026-10-13 afternoon (`/driftcheck`
  `dd70e65e8` — `93c5af6ad` bugs 235–242 + `584d28164` Phase E); 2026-10-13
  (`/unify` of the `4a38222e6` round — the eight absorbed / ratified rows
  retired, the eleven past the pin kept, their dispositions confirmed
  unchanged bar `97f0d8cb8`'s PORTED); 2026-10-13 (`/driftcheck` `732eebcd7` — the five memory-refactor
  rows appended); 2026-10-12 evening (`/driftcheck` — `8050958be`,
  `97f0d8cb8`, `4ce64fa1e`); 2026-10-12 (`/driftcheck` — the `01a83539d`
  unification's close probe, seven rows); 2026-10-12 (`/unify` of the
  `01a83539d` round); 2026-10-12 morning (the four wardrobe rows);
  2026-10-09 (three checks); 2026-10-08 (`/unify` of the `f5e953a3f` round +
  the evening check).
- **v4 `main` HEAD at check: `362426c8f`** ("feat(pascal): trigger phrases run
  custom tools when their words are said", 2026-10-15 16:18, `4.11.0-dev.3`) —
  local `main` = `origin/main`. **THIRTY-NINE commits past the baseline**
  (`4a38222e6..main`); the seventeen newest (`a42c282b8` … `362426c8f`, §3)
  land AFTER the `83334b9a5` round's pin. Among them: **v4 RELEASED 4.10.0** —
  `2432b9057` (`release: 4.10.0`, 2026-10-14 10:26, the squash whose first
  parent is `release: 4.9.2`; tree-identical to main's pre-merge tip save five
  stamp files — NOTHING in it is new relative to main), `f573a7576` the merge
  back, `37246654d` `dev: started 4.11.0 development` (v4 now stamps
  `4.11.0-dev.N`); the five release-checklist commits 7 / 8 / 10 / 12 / 13 (one
  SPA class, plugin-utils 2.7.1's `normalizeUsage` cache-hit exclusion for
  `openai-compatible`, the restore remapper's three JSON-string participant
  arrays + `targetParticipantIds`, the CLI help / completions bytes + the
  `migrations status` id fix, the About page's four blurbs + a docs tidy); bug
  247 FILED (OPEN — the v5 harness's `PROTOTYPE_KEY_MONTH_THROWS` finding,
  provenance `Original to v4`; the CONVERGENCE row belongs to the future fix);
  bug 248 (the chunk-embedding queue from the pre-write read); `bbbcd2388`
  wardrobe copy-to-folder + the components ask (a wire-shape change on
  P4.D281's routes, NO schema move); `7458ff94e` the styled-hair precedence
  note (model-visible text on `describeOutfit`); `936a34720` three new feature
  specs (docs-only); and **`362426c8f` Pascal trigger phrases — PORT-NEW +
  PORT WITH THE TENTH D23 MOVE** (`add-custom-tool-triggers-field-v1`:
  `chat_settings.customToolTriggers INTEGER DEFAULT 0`, the schema field
  between `customTools` and `compositionModeDefault`; `pascalMeta.invokedBy`
  widens to `'trigger'`; one run service replaces two ported entrances). Before
  them, at `01f508d4e` ("Release checklist 6: prune leftover debug logging",
  2026-10-13 21:52, `4.10.0-dev.178`) — local `main` = `origin/main`, tree
  CLEAN: **TWENTY-TWO commits past the baseline**; the two newest (`7417e9a1b` dead-code sweep, `01f508d4e`
  debug-line prune) land AFTER the `83334b9a5` round's pin (§3 rows).
  Before them: **`83334b9a5`** ("Release checklist 4: API
  standard audit; use successResponse in new handlers", 2026-10-13 21:18,
  `4.10.0-dev.176`) — local `main` = `origin/main`. **TWENTY commits past
  the baseline** (`4a38222e6..main`). `83334b9a5` (15 files, +41 / −35):
  24 `NextResponse.json(x[, {status}])` → `successResponse(x[, status])`
  (`lib/api/responses.ts:63-68` IS `NextResponse.json(data, { status })`,
  default 200 — byte-identical responses; every non-swap hunk line is the
  import widening, measured) in the chat actions / GET handlers, connection
  profiles, memories (`maintain` / `maintenance-status` / `repair` /
  config handlers + manual create), plugins and the scenario item route
  factory; a jest mock fix; CHANGELOG / README / stamps. **NO-PORT?** — its
  only effect on v5 is pin line numbers (one-for-one, no line count move in
  `memories/route.ts`). Before it, at `64743ee13`: **NINETEEN commits past
  the baseline** (`4a38222e6..64743ee13`). The six NEW
  this check (§3 rows carry the full classification; dependencies
  stamps-only — `package.json` / `packages/quilltap/package.json` move only
  their `version`; `help/` 132 → **131**: `memory-consolidation.md` +
  `memory-housekeeping.md` DELETED, `memory-maintenance.md` ADDED, twelve
  edited):
  - `4ae0c4ca6` (Built-in TF-IDF tuning + bug 243, 14:31, `.169`; 69
    files, +1,655 / −122) — **the NINTH D23 move** (the message's first
    paragraph says "no schema change"; the PR-review commit inside the
    squash adds one): migration `add-tfidf-text-shape-version-v1`
    (`tfidf_vocabularies.textShapeVersion INTEGER DEFAULT 0`, APPENDED by
    `addColumnIfMissing`) AND the field on `TfidfVocabularySchema` between
    `fittedAt` and `createdAt` — so generateDDL's fresh table carries it in
    SCHEMA order (the informs two-shape precedent). Behaviour: BUILTIN
    embeds drop the anchor line (`lexicalEmbeddingTextFor`), per-provider
    admission thresholds (`ADMISSION_THRESHOLDS_TFIDF` 0.80 / 0.40 / 0.20),
    `memoryConsolidation.tfidfClusterThreshold` (0.30), the refit-when-due
    policy (`builtin-refit-policy.ts`), `isLexicalEmbeddingProvider`, the
    anchor probe's `lexicalProfile` note. Bug 243 is the forked job child's
    plugin-load race — NO-PORT candidate (v5's runner is in-process over
    static manifests).
  - `104e986c7` (Phase F, 15:55, `.172`; 88 files, +4,235 / −3,696) — ONE
    `MEMORY_MAINTENANCE` job (fold-near-duplicates → consolidate → demote →
    retire → mirror) replaces `MEMORY_HOUSEKEEPING` + `MEMORY_CONSOLIDATION`;
    `housekeeping.ts`, `consolidation-triggers.ts`, the outcome cache,
    `scheduled-housekeeping.ts`, both handlers and the `memory-dedup` TOOL
    DELETED; the parent-side ledger; `memoryMaintenance.phases` toggles;
    `?action=maintain` / `maintenance-status`; the three retention knobs
    gone; migration `drop-housekeeping-consolidation-jobs-v1` (a row DELETE
    — no DDL).
  - `b045dd986` (Phase G + bugs 242 / 244, 17:05; 87 files, +4,135 /
    −2,441) — the `housekeep` / `housekeep-sweep` / `consolidate` /
    `housekeeping-config` actions RETIRED, NEW `?action=repair` (dangling
    links), ONE Maintenance card + dialog, the extraction limits onto the
    Extraction Grain card, CLI `memories maintain` / `validate --repair` /
    `db memories --tier`; **bug 244** (`patchSchemaOf` — Zod 4's
    `.partial()` keeps defaults) — v5's ruled both-ways table
    `PARTIAL_DEFAULTS_CLOBBER` (P4.D271 R-A) mirrored v4's clobber, so its
    three rows TRIP BY DESIGN at a pin past this (§5.4 — measure, then
    retire); **bug 242** (`inChain` only for `chainDepth > 0`) — v5's
    `chat-stream.reducer.ts:296` sets `inChain` on ANY `turnStart`, the
    pre-fix shape. NO migration (`add-memory-tiers-v1` gains a doc-path
    comment only).
  - `48790fb73` (the refactor's close, 17:46; 32 files, +105 / −65) — three
    report / ledger labels (SPA + CLI) and docs; **its spec records the
    Friday backfill: v4 ran the refactor migrations on LIVE Friday on
    2026-10-13 22:24 UTC** (the LIVE-DATA HAZARDS block below is now FIRED).
  - `9ec4fa2e9` (V4test dogfood, 20:17; 10 files) — docs, bugs 245 / 246
    filed, one CLI status line ("within the cap").
  - `64743ee13` (bugs 245 / 246, 20:36, `.175`; 38 files, +637 / −95) — a
    `turn` pass in `memory_extraction_cursors` (free-text `pass`, NO
    migration; `ExtractionCursorPassEnum` widens), the turn transcript's
    "EARLIER IN THIS TURN" context, autonomous speakers bounded by the
    previous anchor, `turnExtractionWatermarkMessageId` on the bundles
    (`qtap-export.schema.json`, the restore / import id remaps);
    consolidation's budget from `resolveMaxTokens` with 4,000 as a floor.

  The two from the previous check (`dd70e65e8`; NEITHER ships a migration):
  - `93c5af6ad` ("Fix bugs 235–241 from the memory refactor dogfood; file
    bug 242", 11:49, `.165`; 56 files, +2,253 / −217) — the in-flight dirt
    the last check recorded, committed. PORT: 236 (manual create / edit trim
    + require text → 400; refusal mapped by reason, `embedding-failed` →
    502), 237 (admission's mixed-dimension neighbour fallback), 238 (fold
    OTHER `occurredAt` at the observer's last line), 239 (the instance lock
    gains an optional `port` — v5's `quilltap-host/src/lock.rs` writes the
    SAME shared file; and three CLI tools' `--instance` / `--data-dir` /
    `--passphrase`), 240 (a chats-only `.qtap`'s memories attach to existing
    characters), 241 (the send-failure stream error frame tells the Salon
    whether the user row persisted — `SubmissionLedger` — and the SPA
    restores composer / draft / attachments; a buffered SSE reader). 235 is
    a Next.js per-bundle module-state bug (`processSingleton`) — NO-PORT
    candidate in itself. **Bug 242 filed OPEN** (a depth-0 `turnStart` on
    every first turn sends single-character turns down the chain path) —
    v5's `chat-stream.reducer.ts:296` likewise sets `inChain` on ANY
    `turnStart`; whether v5 shares it is a dogfood measurement, not assumed.
  - `584d28164` (Phase E, 12:01, `.166`; 132 files, +23,460 / −4,300 —
    18,012 of it the recall-probe fixture JSON) — recall rebuilt as a
    pipeline under `lib/memory/recall/` (query → gather → score → select →
    hydrate → assemble → render → deliver); `memory-service.ts` (−1,016) and
    `lib/chat/context/memory-injector.ts` (−822) DELETED, `context-manager.ts`
    −885; ONE query embedding per turn; cross-pool dedup (archive / head /
    inter-character); **the model-visible recall text loses every debug
    label** (`[m_xxxx]` tags, importance / relevance / weight numbers,
    `recall:` multipliers, targeting keywords) — request bytes move on every
    turn with memories; delivery stamps `lastAccessedAt` / the anti-
    repetition ring only for lines that SURVIVE compression; the turn's
    recall record written to the assistant message's `debugMemoryLogs`;
    `recall-replay` gains `fallbackPath` / `trace` / `pools` / `ring` /
    `ringSource`; the Memories API search stamps nothing.

  Eleven were already in §3 before that check. Six were already
  in §3 at round start, compressed (the rows carry the full classification):
  - `545e299bb` scenarios from anywhere (PORT-NEW + PORT, the EIGHTH D23
    move — `chats.scenarioSource` / `scenarioNotes`); `124c8aca3` memory
    Phase 0, bugs 199–201 (PORT — the trailing-only recall, the gate's
    orphan widen, the chat wipe sparing manual / cold / digest rows);
    `69d38dd60` bug 227, the deferred mail flip + one trailing-section list
    (PORT); `8050958be` bug 228, reminted message ids on a fresh-id `.qtap`
    chat import (PORT); `97f0d8cb8` bug 229, the folder picker (PORT, SPA —
    PORTED by P4.D285 with the round, the row stays until the baseline
    passes it); `4ce64fa1e` the merge renumbering the picker bug to 229
    (NO-PORT?).

  **Plus five — v4's memory system refactor, shipped** (classified by the
  same-day `/driftcheck` `732eebcd7`; each a squashed PR; every one moves
  behaviour on the memory programme v5 absorbed at `01a83539d`, and four of
  the five move the SCHEMA):
  - `c2795733f` ("Fix bugs 203–226: memory system defects from the refactor
    audit (#90)", 2026-10-12 17:02, `.157`; 154 files, +10,585 / −1,247) —
    24 bug fixes, each with an implementing hunk, and **FIVE migrations**
    (the message names three): `add-episode-extraction-watermark-v1`
    (`chats.episodeExtractionWatermarkMessageId TEXT DEFAULT NULL`,
    backfilled from each chat's last `summaryAnchorMessageIds` entry),
    `add-memories-superseded-by-index-v1` (partial `idx_memories_
    supersededById … WHERE "supersededById" IS NOT NULL`),
    `add-memory-superseded-at-v1` (`memories.supersededAt TEXT DEFAULT
    NULL`, backfilled from `updatedAt` where `supersededById` is set),
    `recompute-reinforced-importance-v1` (EVERY row → `min(1, importance +
    log2(max(1, count)) × 0.05)` — v5 computes `log2(count + 1)`,
    `memory_gate.rs:45-46`), `rezone-legacy-memory-occurred-at-v1` (every
    UTC-midnight `occurredAt` re-stamped at local midnight on the memory
    calendar).
  - `e0ac7493a` ("Memory refactor Phase A: scoring, clock, settings,
    candidate (#91)", 19:23, `.160`; 166 files, +5,887 / −2,781) — scoring
    to `lib/memory/scoring/*` (the floored `calculateEffectiveWeight` and
    `rankMemoriesByWeight` DELETED — orderings move), the per-turn recall
    floor .5 → .3, every `minImportance` filter on reinforced importance,
    `lib/memory/time/*` (write side stamps the window START), the Zod
    candidate, bugs 230–234, the fold-episode catch-up job; **migration
    `move-memory-maintenance-settings-v1`** — copies `autoHousekeepingSettings`
    into `instance_settings.memoryMaintenance`, deletes the
    `memoryExtractionConcurrency` instance key, then `ALTER TABLE
    "chat_settings" DROP COLUMN` `autoHousekeepingSettings`,
    `memoryExtractionLimits`, `memoryExtractionConcurrency` (each if
    present); `?action=maintenance-config` replaces the housekeeping config
    and `extraction-concurrency` is GONE.
  - `af48f6cc2` ("Memory refactor Phase B: index owns vectors, one deletion
    path (#92)", 22:31, `.161`; 117 files, +3,576 / −3,878) — vectors live
    ONLY on `memories.embedding`; ONE deletion path (`retireMemories` /
    `retireChatMemories`, the repository's bare `delete` now THROWS); backups
    stop writing `vector-entries.json` / `vector-index-metas.json`; **migration
    `retire-memory-vector-entries-v1`** — a verify-and-copy of each mirror
    vector onto a row with no embedding, then `DROP TABLE IF EXISTS`
    `vector_entries` AND `vector_indices`. **RULED 2026-10-13 (the human): MIRROR v4** — port `retire-memory-vector-entries-v1` as a boot ensure (verify-and-copy each mirror vector onto a row with no embedding, then `DROP TABLE IF EXISTS` both), differentially compared against v4's real `MigrationRunner`; every v5 reader / writer moves onto `memories.embedding`.
  - `8af764514` ("Memory refactor Phase C: admission in parent, one combine
    (#93)", 2026-10-13 00:08, `.163`; 87 files, +4,011 / −3,588) — NO
    migration; the gate and the merge DELETED for `lib/memory/admission/*`
    (same thresholds, a new decide: subject / kind / source eligibility,
    excluded neighbours become links, cold promote-and-reinforce, AUTO never
    promoted to MANUAL, a per-character lock + idempotency ledger) and ONE
    combine (`foldInto` / `applyFold`) for reinforce, merge and dedup; every
    `[MemoryGate]` / `[MemoryMerge]` / `[Gate]` line gone; **wire:** POST
    `/memories`' `action` is now the lowercase kind (`insert` /
    `insert-linked` / `reinforce` / `absorb` / `promote-and-reinforce`).
  - `ac6348c5f` (Phase D, above; 152 files, +10,498 / −9,320) — extraction
    rebuilt under `lib/memory/extraction/` (one context per unit, four
    passes, the fold catch-ups, a live rate-limit counter, cursors);
    **migrations** `add-memory-extraction-cursors-v1` (NEW table
    `memory_extraction_cursors` — `UNIQUE ("chatId","pass")`, FK to `chats`
    ON DELETE CASCADE — fed from both chats watermark columns as passes
    `other-fold` / `episode-fold`, then `ALTER TABLE "chats" DROP COLUMN`
    BOTH: `otherExtractionWatermarkMessageId` — the column v5's P4.D265
    ensure ADDS — and `c2795733f`'s `episodeExtractionWatermarkMessageId`)
    and `drop-inter-character-memory-jobs-v1` (deletes queued
    `INTER_CHARACTER_MEMORY` jobs; the type is retired); `MEMORY_EXTRACTION`
    payloads gain `pass` / `cursor` / `ignoreIdle` / `backfill`.
  - **CONVERGENCE check: NONE.** Every bug the five touch — 199–234 — reads
    `Provenance: Original to v4` (the audit of 2026-10-12 and PR 90's
    follow-ups); the port's filings (180, 183, 184, 185) are all absorbed by
    earlier baselines. v5 carries v4's PRE-fix shape on every surface they
    touch. **Re-checked for the two new commits: still NONE** — bugs
    236–241 read `Original to v4` or name a Phase B–D commit as their
    provenance (235's names the old singleton, 242's v4's own `5a99e4b8f`);
    no port filing returns. **Re-checked for the six (late evening): still
    NONE** — bugs 243–246 all read `Original to v4` (242's `5a99e4b8f`).
    One RULED-DIVERGENCE trip instead: bug 244 fixes the clobber v5's
    `PARTIAL_DEFAULTS_CLOBBER` table pins on both sides (above).
- **v4 `bugfix` tip at check:** `489b228b6` ("bugfix: started 4.10.0 bug
  branch", 2026-10-14; parent `1a2b2164c`, the 4.9.2 bug-branch start — the
  squash-like topology §4 step 2 describes; `origin/bugfix` the same).
  **Measured by CONTENT:** `git diff 2432b9057 bugfix -- lib app packages
  plugins` EMPTY (three stamp files overall, `4.10.1-bugfix.0`); `git diff main
  bugfix -- lib app packages plugins` = exactly main's two post-release code
  commits (`7458ff94e`, `362426c8f`) seen backwards. **Bugfix carries ZERO
  unabsorbed content.** **`release` tip:** `2432b9057` (`release: 4.10.0`;
  `origin/release` the same) — **the 4.10.0 squash EXISTS** (the earlier "still
  no `release: 4.10.0` squash" notes here and in CLAUDE.md's baseline bullet are
  superseded). Was, at the 2026-10-14 check: bugfix `1a2b2164c` UNMOVED,
  release `8fbf2afe0` (`release: 4.9.2`).
- **Checkout at check:** branch **`main`**, tree **DIRTY** at `362426c8f` —
  41 paths (+1,634 / −121; 38 modified, one rename, three untracked): the
  human's in-flight **Pascal wardrobe effects** (the `936a34720` spec —
  `effects[].wardrobe` wear / take_off / clear_slot through `wearItem` /
  `takeOffWearable` / `applyDisplacement`, NEW `lib/pascal/wardrobe-effects.ts`,
  audit entries on `pascalMeta.effects`; JSON-column only, NO migration, NO
  stored column) plus a **bug-249 fix** in `turn-orchestrator.service.ts` (a
  lone character chained to the depth cap — `shouldChainNext` returns
  `user_turn` for `only_character`) and a composite `takeOffWearable` fix in
  `wear-ops.ts`; it touches `lib/` (14 files), both `public/schemas/`, three
  `help/` pages, `CUSTOM_TOOL_SPEC.json`, `DDL.md`, `bugs.md` ("1–246, 248 and
  249 fixed; 247 open"). **It POISONS any regen from the live checkout** —
  `lib/pascal/*`, `wear-ops.ts`, `tool-executor.ts`,
  `turn-orchestrator.service.ts`, `chat.types.ts` are all oracle-imported.
  Read by NOTHING: every regen runs from a pin (§5.1). The §2 probe reads
  `362426c8f..main` and EXPECTS this dirt (its commit is the next
  `/driftcheck`'s row — classify from the commit, not from the dirt). Was, at
  the 2026-10-14 check: CLEAN at `01f508d4e` (the dead-code dirt committed as
  `7417e9a1b`). Recorded at the earlier probe: tree **DIRTY** — 11 modified
  (+9 / −87, deletions almost only) + 2 STAGED DELETIONS (`lib/memory/
  index.ts`, `lib/memory/recall/index.ts` — barrel files): `lib/memory/
  extraction/prompts/shared.ts`, `lib/memory/time/calendar.ts`,
  `lib/schemas/memory-extraction-cursor.types.ts`, `lib/mount-index/sync/
  {manifest,types}.ts`, `lib/progressions/engine.ts`,
  `lib/services/dangerous-content/refusal.ts`, `lib/database/repositories/
  vault-overlay/wardrobe-writes.ts` + three tests — the human's in-flight
  release-checklist dead-code removal (the next `Release checklist` commit,
  presumably). Read by NOTHING: every regen runs from a pin (§5.1). Local
  `main` = `origin/main` = `83334b9a5`. The §2 probe reads
  `83334b9a5..main` and EXPECTS this dirt (a commit of it is the next
  `/driftcheck`'s row).
- **Verdict: DRIFT PENDING — 39 commits (§3).** The seventeen past the
  `83334b9a5` pin (all UNPROCESSED): **8 NO-PORT?** (`2432b9057` the release
  squash, `f573a7576`, `37246654d`, `1e1712a12`, `3f64d3a6c` README +
  `docs/FEATURES.md` — itself a doc-convergence on this port's Phase-0 cipher
  finding —, `936a34720` the three specs, `052659624` + `809e5104d` bug 247's
  filing), **7 PORT** (`a42c282b8` one SPA class, `a8b2d2861` plugin-utils
  2.7.1's cache-hit exclusion on the `OpenAiCompatible` flavor, `49f8906c3` the
  restore remapper's participant-id arrays, `75d094e63` Tier R bytes + the CLI
  README, `8489d08b9` the About blurbs + a docs re-vendor with moves and
  deletes, `b82037d75` bug 248, `7458ff94e` the hair precedence note), **2
  PORT-NEW + PORT** (`bbbcd2388` copy-to-folder + the components ask;
  `362426c8f` Pascal trigger phrases **WITH THE TENTH D23 MOVE**). ⚠ Live-data
  note for the next round: a v4 4.11-dev boot ALTERs live `chat_settings`
  (`customToolTriggers`) and writes `pascalMeta.invokedBy: 'trigger'` rows a v5
  at the `83334b9a5` baseline refuses on read / restore / import — the §1
  copy-before-pull rule for dogfood copies continues past this round. **Regen
  rule: PIN REQUIRED** (HEAD past the baseline AND the checkout dirty) — every
  regen of the `83334b9a5` unification runs from
  `/tmp/qt-v4-pin-unify-83334b9a5` (its own `npm ci --offline` tree). The
  twenty-two from the previous check: 4 NO-PORT? (`4ce64fa1e`,
  `9ec4fa2e9`, `83334b9a5`, `7417e9a1b`), 1 PORT past the round's pin
  (`01f508d4e`, DEBUG lines), 16 PORT (`124c8aca3`, `69d38dd60`, `8050958be`,
  `97f0d8cb8`, the five — `c2795733f`, `e0ac7493a`, `af48f6cc2`,
  `ac6348c5f` each WITH a schema move, `8af764514` with a wire change —
  `93c5af6ad`, `584d28164`, and the five new: `4ae0c4ca6` WITH the NINTH
  D23 move, `104e986c7` with a job-row migration, `b045dd986`, `48790fb73`
  (labels riding Phase G's new surfaces), `64743ee13`), 1 PORT-NEW + PORT
  (`545e299bb`).** Nineteen are UNPROCESSED; `97f0d8cb8` is
  PORTED(P4.D285) — baseline not yet past. **Pin for the memory catch-up:
  `64743ee13` or later** — v4 declares the refactor CLOSED (`48790fb73`
  moved its spec to `complete/`), the first HEAD where the memory programme
  is not mid-rewrite.
- **Where the seven land** (the five + `93c5af6ad` + `584d28164`; Phase E
  adds `memory_injector.rs`, `recall_tuning.rs`, `api/recall_replay.rs`, the
  context builder's recall block and `debugMemoryLogs`; `93c5af6ad` adds
  `quilltap-host/src/lock.rs`, the `.qtap` memory import, the Salon
  stream-error frame + the SPA composer): the whole memory programme v5 absorbed at
  `01a83539d` (P4.D265–P4.D276) and the surfaces under it — `memory_gate.rs`
  (the reinforced-importance formula; the Phase-1 `memory_weighting`
  tier-1), `services/memory_gate.rs` + `memory_merge.rs` (P4.D266),
  `services/housekeeping.rs` + the recall / search scoring (P4.D266 /
  P4.D267), `services/episodic.rs` + the anchors + the embedding text
  (P4.d14 / P4.D267 / P4.D269; bug 212's one zone sits beside P4.119's
  host-zone dates), the fold passes + the idle catch-up
  (`services/fold_{other,episode}_pass.rs`, P4.D270),
  `services/carina_memory_extraction.rs`, `services/memory_processor.rs`,
  `services/consolidation.rs` (P4.D271), `db/memories.rs`
  (`delete_with_unlink`, P4.d12) + `services/memory_service.rs`'s
  `*_with_vectors` (P4.D126 / P4.D270), `db/vector_indices.rs` +
  `db/vector_store.rs` (Phase 2; the tier stamp P4.D265), the embedding
  jobs (`embedding_generate_job.rs` P4.6BL, `embedding_reindex_job.rs`;
  the embedding-status marks P4.D25), `services/cascade_delete.rs`,
  `db/chat_settings.rs` (the three columns + `find_auto_housekeeping_
  settings_by_user_id`) + `db/instance_settings.rs` (the concurrency key),
  `db/chats.rs` / `chats_read.rs` + `db/memory_tiers_repair.rs` (the
  watermark column, P4.D265), `api/memories.rs` (every action touched),
  the job runner / queue / scheduled maintenance / activity kinds / job
  topics (`INTER_CHARACTER_MEMORY`), the backup / restore / `.qtap`
  carriers (P4.9G, P4.D264, P4.D272), the Almanack's memory lines, the
  `search_scriptorium` / `self_inventory` tools, the anchor probe +
  `quilltap anchor-probe` (P4.D269), and the SPA `memory/` cards (P4.D276).
  The six new add: `db/tfidf_vocabulary.rs` + `fresh_schema.json` (the
  D23 re-dump) + the TF-IDF builtin provider and the embedding service
  (P4 provider layer), the job types / queue / scheduler (P4.D266 /
  P4.D271's housekeeping + consolidation jobs and triggers → one
  maintenance job), the `memory-dedup` tool's retirement from the tool
  table, `api/memories.rs` (four actions retired, `maintain` /
  `maintenance-status` / `repair` new),
  `PARTIAL_DEFAULTS_CLOBBER` (`memory_consolidation_routes_equivalence`),
  the SPA reducer (bug 242) and `memory/` cards (P4.D276), the per-turn
  extraction + turn transcript (P4.D270), the bundle carriers (P4.D272),
  the CLI (`anchor-probe`'s note; the `memories` verbs v5 has not ported),
  and `help/` (p4.9i2 — 132 → 131 files).
  **Order them as ONE memory catch-up with `124c8aca3` + `69d38dd60`** (and
  now `93c5af6ad` + `584d28164` + the six — Phase E rewrites the recall half the
  earlier phases left standing, so a pin short of it ports `memory-service.ts`
  / `memory-injector.ts`, both deleted at HEAD)
  (Phase 0 + bug 227) — each later phase rewrites the previous one's
  modules (B deletes what A renamed; C deletes the gate 203–226 fixed; D
  drops the column `c2795733f` added), so absorbing a prefix means porting
  code v4 deleted hours later. The planner decides whether the scenarios D23
  move (`545e299bb` + bug 228) shares that round or precedes it.

### ⚠ LIVE-DATA HAZARDS — read before any v5 build touches real data

**FIRED (2026-10-13 22:24 UTC): v4 ran the refactor migrations on LIVE
FRIDAY** (`48790fb73`'s spec, "Friday backfill (E2)": ten migrations in
2.96 s, 44,240 memories, `retire-memory-vector-entries-v1` copied 0 and
DROPPED the mirror, 3,166 importances recomputed, 1,862 `occurredAt`
re-zoned; then real maintenance runs — ~120 digests created, ~250 members
superseded, **913 rows demoted** at a 2,500 cap). Live Friday — and every
copy taken from it from now on — is POST-migration. HEAD adds two more
migrations past the nine (`add-tfidf-text-shape-version-v1`,
`drop-housekeeping-consolidation-jobs-v1`), eleven in all; three of them
DROP COLUMN / DROP TABLE, which cannot be undone. Once v4 at
or past `c2795733f` has migrated an instance, **a v5 build at the current
baseline must NOT open it** — not live Friday, not a copy of it taken
afterwards:

1. **The dropped `chat_settings` columns** (`e0ac7493a`). v5's
   `db/chat_settings.rs` reads and writes `autoHousekeepingSettings` and
   `memoryExtractionLimits` by name: `find_auto_housekeeping_settings_by_
   user_id`'s SELECT fails (`no such column`), and so do the settings row's
   writes. v4 now holds those knobs in `instance_settings.memoryMaintenance`,
   which v5 does not read.
2. **The dropped vector tables** (`af48f6cc2`). Every `vector_entries`
   statement fails `no such table`: a gated memory CREATE fails AFTER the row
   is written (the embedded-create closure's `Db::write` is autocommit — an
   Err with the row persisted); a character's cascade delete fails at
   `cascade_delete.rs:376` after its chats and memories are gone and before
   the character row — a HALF-COMPLETED delete answering 500; semantic recall
   errors and silently falls back to text search on every turn (WARN each
   time); EMBEDDING_GENERATE and the `all` reindex fail; the optimizer and
   the anchor probe error. `/health`'s structural pass does NOT notice (no
   vector table in `STRUCTURAL_TABLES`). A v4-HEAD backup restored into v5
   brings embeddings on the rows and no mirror, so recall stays cold until a
   reindex.
3. **The dropped chats watermark column, RE-ADDED by v5** (`ac6348c5f`).
   v5's P4.D265 boot ensure (`db/memory_tiers_repair.rs`) puts
   `chats.otherExtractionWatermarkMessageId` back as NULL — so every chat's
   OTHER-fold position reads empty and v5's fold OTHER / idle catch-up
   RE-READS EVERY CHAT'S WHOLE HISTORY (mass re-extraction, cheap-LLM spend,
   duplicate memories). v4 never looks at the re-added column (its
   migration is ledgered) and v5 never looks at `memory_extraction_cursors`
   — two independent positions, DOUBLE EXTRACTION on alternating builds
   (the shared-instance ping-pong; **RULED 2026-10-13 (the human): RETIRE P4.D265's chat-column arm** (the P4.D251-supersedes-P4.D179 precedent) — port `add-memory-extraction-cursors-v1` as a boot ensure (copy both watermark columns into `memory_extraction_cursors`, then DROP both columns; carry both v4 table shapes — generateDDL's bare one and the migration's `UNIQUE` + FK — as the informs ensure did), differentially compared against v4's real migration; v5 reads only the cursors). v4's new catch-up
   jobs (`pass: 'other-fold' | 'episode-fold'`, null opener) claimed by v5
   are misread as per-turn jobs — a wrong anchored-turn extraction.
4. **The reinforced-importance formula** (`c2795733f`). v4 rewrote every
   row to `log2(max(1, count))`; v5 writes `log2(count + 1)` on every
   reinforce and create — v5-written rows drift back up by up to 0.05,
   mixing two scales in the recall floors and the recap bands.
5. **Cursor loss through v5 bundles** (`ac6348c5f`). v4's backups / `.qtap`
   carry the cursors under the legacy chat keys; a v5 restore / import
   writes only the OTHER one into the re-added column and a v5 export
   carries only that (NULL-filled) column — the episode cursor is lost on a
   v5 round trip.

**Earlier hazards, still standing** (the older §3 rows cite them as
"real-instance note (1) / (3)" — (a) / (b) here): (a) `545e299bb` — v4's
boot adds `chats.scenarioSource` / `scenarioNotes`; a v5 chat UPDATE
rewriting `scenarioText` leaves a stale source, and a v5 backup / `.qtap`
export drops both keys (measure the read on a copy rather than assume). (b)
`124c8aca3` — v5's chat wipe deletes EVERY row with the `chatId` where v4
spares manual, cold and digest rows (bug 201's data loss), and v5's
per-row deletes orphan vectors (bug 200): **no v5 chat wipe at all** until
the memory catch-up lands. **RETIRED by this round:** the wardrobe-folders
hazard (the old note (2) — `4a38222e6`, v5's flat projection not seeing the
items v4 files into `Wardrobe/<sub>/*.md`, a v5 projection duplicating
them): v5 now reads `Wardrobe/<sub>/*.md` recursively (P4.D279).
Bug 228 adds none (v5's Create New import fails as pre-fix v4 did — do not
rely on it).

**Dogfood ONLY on a COPY taken BEFORE 2026-10-13 22:24 UTC** (when v4 past
`c2795733f` migrated live Friday — any newer copy is post-migration; until
the memory catch-up lands there is no Friday data v5 may open except a
pre-migration copy already on disk)
(the owed dogfood pass's copy qualifies if it predates that launch — check
for `memory_extraction_cursors` / `vector_entries` in it before the first v5
boot: a copy WITH the cursor table or WITHOUT `vector_entries` is
post-migration and must not be opened). **No v5 build against live Friday
until the memory catch-up round lands.** The P4.D274 store-name plan probe
on the copy, the `52d6e7ecd` / `07b8f0209` notes (the
`chat_informs.permanent` ALTER, the voice-mode translation) stand.

### Schema / D23

- **NO D23 move this round** — `4a38222e6` carries no DDL; the wardrobe
  folders live in the document store.
- **The EIGHTH re-dump is still PENDING** (`545e299bb`:
  `chats.scenarioSource` / `scenarioNotes` after `"scenarioText" TEXT`, from
  a pin at or past `545e299bb`; `add-chat-scenario-source-v1` re-homed as an
  unstamped boot ensure).
- **The memory programme forces at least ONE MORE** (from a pin at or past
  `ac6348c5f` if absorbed whole — an intermediate pin would carry
  `episodeExtractionWatermarkMessageId` and the vector tables, both gone by
  HEAD): `fresh_schema.json` LOSES `vector_entries` + `vector_indices` (two
  CREATE TABLEs + `idx_vector_{entries,indices}_createdAt`, today lines
  35-36 / 85-86) and `chats."otherExtractionWatermarkMessageId"`; GAINS
  `memories.supersededAt` and the NEW `memory_extraction_cursors` table —
  likely in TWO v4 shapes (generateDDL's bare `"id" TEXT PRIMARY KEY NOT
  NULL …` with no UNIQUE / FK vs the migration's `UNIQUE ("chatId","pass")`
  + `FOREIGN KEY … ON DELETE CASCADE`; confirm by re-dump, as with
  `chat_informs.permanent`); `chat_settings` LOSES `autoHousekeepingSettings`
  + `memoryExtractionLimits` (and `memoryExtractionConcurrency` where an
  instance carries it). `migration_indexes.json` GAINS the partial
  `idx_memories_supersededById` and LOSES `idx_vector_entries_characterId`
  + `idx_vector_indices_characterId` (today lines 58-59) — 63 → 62.
- **Boot ensures re-homing the NINE migrations**, each differentially
  compared against v4's REAL `MigrationRunner` (the `add-memory-tiers-v1` /
  `impersonation-voice-mode-v1` shape): the two ADD COLUMNs with their
  backfills, the partial index, the two data rewrites (reinforced importance;
  the occurredAt rezone), the settings move + three DROP COLUMNs, the
  cursor table + copy + two DROP COLUMNs (which REPLACES P4.D265's chat-column
  arm — kept, it would ping-pong a shared instance with v4, the P4.D179
  precedent), the job-type delete, and the vector retirement (**RULED 2026-10-13: mirror** the destructive
  verify-and-copy-then-DROP as a boot ensure). The two DATA rewrites
  (`recompute-reinforced-importance-v1`, `rezone-legacy-memory-occurred-at-v1`)
  are not idempotent: gate them once-only over v4's own `migrations_state`
  ledger (the cross-app heal precedent), never as unstamped ensures. Every committed
  `*-main.db` pair WIDENS / NARROWS in place through v4's real migrations at
  the pin. The restore mirror-boot list grows with each ensure.
- **The NINTH move rides the same re-dump** (`4ae0c4ca6`):
  `tfidf_vocabularies."textShapeVersion" INTEGER DEFAULT 0` — in
  generateDDL's dump it sits in `TfidfVocabularySchema` order (after
  `"fittedAt"`, before `"createdAt"`); the migration
  `add-tfidf-text-shape-version-v1` APPENDS it on an existing table — two
  shapes, carried as the `chat_informs.permanent` ensure did (P4.D249). A
  TENTH migration re-homes as an ensure (eleven past the baseline in all):
  `drop-housekeeping-consolidation-jobs-v1` (`104e986c7`, a row DELETE, no
  DDL). `64743ee13`'s `turn` cursor pass is free text — no DDL.
- `public/schemas/qtap-export.schema.json` moves again: chat
  `episodeExtractionWatermarkMessageId` + memory `supersededAt`
  (`c2795733f`), description text for the two chat keys (`ac6348c5f`), and chat
  `turnExtractionWatermarkMessageId` (`64743ee13`).

### Regen, dependencies, help

- **Regen rule: PIN REQUIRED at `4a38222e6`** (§5.1), UNCHANGED by the
  2026-10-13 late-evening check. HEAD (`64743ee13`) is nineteen
  commits past the baseline, so **a pinned worktree is mandatory for EVERY
  regen, no exceptions** — every regen, the whole sweep and every
  live-checkout guard of the `4a38222e6` unification ran from
  `/tmp/qt-v4-pin-unify-4a38222e6` with its own `npm ci --offline` tree.
  ⚠ **IMPORT-DEAD at HEAD** — beyond the remaining rows' moved shapes (the
  scenario location's three deleted tier modules, the Phase-0 gate / wipe,
  bug 228's reminted ids), a HEAD regen FAILS TO IMPORT: `lib/memory/memory-
  weighting.ts` (v5's Phase-1 `memory-weighting` oracle case), `reinforced-
  importance.ts`, `day-references.ts`, `memory-calendar.ts` (renamed under
  `time/`), `episodic-anchors.ts`, `memory-gate.ts`, `memory-merge.ts`,
  `memory-edit.ts`, `memory-processor.ts`, the fold-pass / catch-up /
  extraction-route modules, `lib/schemas/vector-indices.types.ts` +
  `VectorIndicesRepository` (`harness/oracle/cases/vector-indices.ts`, the
  vector / consolidation fixture builders, ~18 builders using
  `repos.vectorIndices`), and the removed exports (`createMemoryWithGate`,
  `runMemoryGate`, `reinforceMemory`, `deleteMemoryWithUnlink*`,
  `deleteMemoriesBy*WithVectors`, `needsHousekeeping`,
  `processTurnForMemory`, `batchExtractMemories`, `handleInterCharacterMemory`,
  …), and since the six: `lib/memory/housekeeping.ts`,
  `consolidation-triggers.ts`, `housekeeping-outcome-cache.ts`,
  `admission/after-insert.ts`, `lib/tools/memory-dedup.ts`,
  `background-jobs/scheduled-housekeeping.ts` and both housekeeping /
  consolidation handlers (Phase F). `provisioning_equivalence` REQUIRES `QT_FRESH_SCHEMA_LIVE` dumped FROM
  THE PIN. ⚠ **The dependency trap:** the root tree last moved at
  `a9c99a4a0` (inside the baseline); still build the pin's own tree
  (`npm ci --offline`, §5.1 — at `4a38222e6` `openai` 7.30.0, the pin's
  lockfile differing from HEAD's only in version stamps).
- **Dependencies: NONE moved** past the baseline (each `package.json` hunk is
  the stamp — `.157` / `.160` / `.161` / `.163` / `.164` on the five, `.165`
  / `.166` on `93c5af6ad` / `584d28164`, `.169`–`.175` on the six; no
  lockfile hunk beyond the stamps; Phase B deletes only the npm script
  `oneoff:rebuild-vector-indices` and its `scripts/` file).
- **Help / docs past the baseline (cumulative):** `help/` **131 → 132
  files** (NEW `help/scenario-browser.md`, `545e299bb`; the five add none —
  HEAD counted 132, the pin 131). Pages touched (`4a38222e6..main`, 19
  files): the scenarios set (`chats`, `general-scenarios`, `groups`,
  `project-scenarios`), `chat-participants`, `memory-regenerate`,
  `system-tools`, `post-office`, `system-import-export`, and from the five:
  `characters`, `chat-settings`, `cli-memories`, `episodic-memory`,
  `memory-consolidation`, `memory-housekeeping`, `chat-message-actions`,
  `memory-recall-relevance`, `embedding-profiles`; the two new add
  `chats`, `embedding-profiles`, `system-import-export` (`93c5af6ad`) and
  `episodic-memory`, `memory-recall-relevance` (`584d28164`); the six
  take it to **131 files at HEAD** (`b045dd986`: `memory-maintenance.md`
  NEW, `memory-consolidation.md` + `memory-housekeeping.md` DELETED; edits
  to `characters`, `cli-memories`, `conversation-summary-regenerate`,
  `data-retention`, `embedding-profiles`, `episodic-memory`,
  `memory-playing-a-character`, `memory-recall-relevance`,
  `memory-regenerate`, `settings`, `system-tools`, `the-almanack`) — the
  same count as the pin, different files. Docs: `API.md`, `DDL.md`
  (the vector tables "retired"), `BACKGROUND_JOBS_CHILD.md`,
  `DATABASE_ABSTRACTION.md`, `DEVELOPMENT.md`, `SYSTEM_FLOWCHARTS.md`,
  `features/memory-system-refactor.md` (phases marked done),
  `features/memory-consolidation-and-tiers.md`, `bugs.md` (1–246 fixed, none open
  past 241's era), `features/memory-system-refactor.md` → `complete/`,
  NEW `features/builtin-tfidf-memory-tuning.md`, `CLI.md`, `DEAD-CODE-REPORT.md` +
  45 bug-file changes. Each catch-up round re-vendors what it absorbs.

### Rounds, guards, state

- **The `83334b9a5` memory-refactor + scenarios drift catch-up round
  ORDERED 2026-10-14 (P4.D288–P4.D303 — sixteen lanes, three stacking points
  `SCORING` → `KEYSTONE` → `ADMISSION`; `phase-4.md` ORDERED section).** ALL
  TWENTY rows through `83334b9a5` are `ORDERED(…)` (`97f0d8cb8` stays
  `PORTED(P4.D285)` and retires with the baseline move); the two rows past
  the pin (`7417e9a1b`, `01f508d4e`) stay UNPROCESSED for the next
  `/driftcheck`. **Regen rule for the round: PIN REQUIRED at `83334b9a5`**
  (lane-unique pins `/tmp/qt-v4-pin-<order>-83334b9a5`). At its unification
  the baseline moves to `83334b9a5` and the LIVE-DATA HAZARDS block retires
  by measurement (its §S.6).
- **The `4a38222e6` wardrobe-programme drift catch-up round (P4.D279 →
  {P4.D280 ∥ P4.D281 ∥ P4.D282 ∥ P4.D283 ∥ P4.D284} ∥ P4.D285 ∥ P4.D286 ∥
  P4.D287) UNIFIED on main 2026-10-13 — all nine lanes landed; the baseline
  MOVED `01a83539d` → `4a38222e6`.** Eight rows retired to §6 (four docs
  rows NO-PORT-RATIFIED; `7a8d6dacc`, `9ecebf986`, `e23fb095b`,
  `4a38222e6` ABSORBED, bug 197's `analyze-image` half DEFERRED by name —
  R-L, the route stays a loud refusal); `97f0d8cb8` PORTED ahead of the
  baseline (R-P) and kept in §3. The standing ruling (2026-10-09) held: the
  work followed the PIN, not later v4 commits.
- **Live-checkout guards at HEAD (predicted, not run):** the six earlier
  rows' set (the scenarios routes + chat-create + `chat_scenario` + `.qtap`
  chat families, the Phase-0 gate / wipe / dedup / housekeeping /
  delete-chokepoint / buildContext / mail families, bug 228's fresh-id
  import families, `provisioning_equivalence`, `system_backup_equivalence`,
  the help and schema guards) PLUS, from the five: every memory tier-1 / 2 /
  3 family (reinforced importance, scoring, ranking, recall floors, the
  gate → admission decisions and lines, merge → fold, dedup, housekeeping,
  consolidation, the fold passes, Carina, the episodic anchors and zone,
  the embedding text), the memories route families (POST `action`, PUT
  re-embed, `maintenance-config`, the removed `extraction-concurrency`,
  `embeddings` `batchSize`), the chat-settings and instance-settings
  families, the vector-indices tier-2, the cascade-delete and
  character-archive families, the embedding job families, the backup /
  restore / `.qtap` carriers (no vector files; cursors), the job-type
  families (`INTER_CHARACTER_MEMORY`), the Almanack memory lines,
  `migration_indexes` (one gained, two lost), `qtap_schema_embed_guard` +
  `public_schemas_vendor_guard`, `help_tree_equivalence` — and every case
  that IMPORTS a deleted module fails outright, not red but dead.
  **Predicted GREEN:** `provider_sdk_version_guard`, `zod_version_guard`,
  `builtin_prompt_templates_guard`.
- **The workspace gate at the baseline:** the round record in
  `status-log.md` has the counts.
- **Schema state (`4a38222e6`):** UNCHANGED from `01a83539d` —
  `fresh_schema.json` is the SEVENTH D23 re-dump (P4.D265: the four
  `memories` tier columns, `chats."otherExtractionWatermarkMessageId"` after
  `timelineMode`, the NOCASE store-name index); `migration_indexes.json` 63
  statements (+ `idx_memories_character_tier`), dumped through v4's REAL
  `MigrationRunner`. `add-memory-tiers-v1` is the unstamped table-gated boot
  ensure `db::memory_tiers_repair`; the NOCASE index on an existing instance
  is `ensure_mount_point_name_unique_index` (P4.D274). Earlier re-dumps (the
  SIXTH — `wardrobe_wear_stats` + `wardrobeImageSettings`; the FIFTH —
  `impersonationVoiceMode`) unchanged. There is NO `migration_tables.json`.
  The EIGHTH (scenarios) and the memory programme's re-dump are PENDING —
  "Schema / D23" above.
- **Recommended next:** (1) the owed dogfood pass — the `01a83539d` and
  `4a38222e6` rounds' rows, on a COPY taken BEFORE v4 passed `c2795733f`
  (verify it per the hazards block first; the store-name plan probe on the
  copy FIRST); (2) the catch-up rounds — the scenarios D23 move
  (`545e299bb` + `8050958be` bug 228) and the memory programme (`124c8aca3`
  + `69d38dd60` + `c2795733f` + `e0ac7493a` + `af48f6cc2` + `8af764514` +
  `ac6348c5f` + `93c5af6ad` + `584d28164` — pin at or past `584d28164`,
  or the round ports `memory-service.ts` / `memory-injector.ts`, which
  Phase E deleted), the planner deciding the grouping and the order (the memory
  programme carries the heavier live-data hazard; the two share
  `fresh_schema.json`, the chats row and the `.qtap` chat carrier). **Both
  rulings MADE (the human, 2026-10-13):** the vector-table retirement —
  MIRROR v4's destructive copy-then-DROP as a boot ensure; the
  shared-instance watermark-column ping-pong — RETIRE P4.D265's chat-column
  arm for v4's cursor-table migration (as P4.D251 superseded P4.D179's
  ensure). Nothing now blocks planning the memory round.
- **`help/**`:** whole at `4a38222e6` (131 files, md5-identical to the
  pin's tree — measured at the unification). Past the pin: the pages listed under "Help /
  docs" above and ONE new (`help/scenario-browser.md`); each catch-up round
  re-vendors its own.

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
when absorbed/ratified. **PORTED(order-id) — baseline not yet past** marks a
row a lane ported ahead of its round's pin; it stays until the baseline
passes the commit.

| sha | date | subject | class | intersects (already-ported work) | disposition |
|---|---|---|---|---|---|
| `545e299bb` | 2026-10-12 | Scenarios from anywhere: location, browser, and source tracking (#86) | **PORT-NEW + PORT — WITH A D23 MOVE (the EIGHTH re-dump)** (classified 2026-10-12 from the hunks, §5.3; 88 files, +5,003 / −2,114 — a squashed PR of four commits: the feature, the control's review fixes, the heal / honest-count fixes, the read-only character location; `__tests__/` + `components/` are roughly half). **SCHEMA / MIGRATION:** NEW `migrations/scripts/add-chat-scenario-source-v1.ts` (registered in `index.ts`; `dependsOn: ['add-chat-scenario-text-field-v1']`; `introducedInVersion: '4.10.0'`; `shouldRun` when either column is missing; `addColumnIfMissing('chats', 'scenarioSource', 'TEXT DEFAULT NULL')` + `('chats', 'scenarioNotes', 'TEXT DEFAULT NULL')`, no backfill; INFO `Added the scenario source columns to chats` / ERROR `Failed to add the scenario source columns`, `context: 'migration.add-chat-scenario-source'`; `prettify.ts` label "Noting where each chat's scene was found"). `lib/schemas/chat.types.ts`: `ScenarioSourceSchema` `{ mountPointId: UUID, path: string.min(1).max(500) — \`Scenarios/<file>.md\`, bodySha256: string.length(64) — hex SHA-256 of the body as resolved, frontmatter stripped, before notes }`, `ScenarioRefSchema` `{ mountPointId, path }`; `scenarioSource: ScenarioSourceSchema.nullable().optional()` + `scenarioNotes: z.string().nullable().optional()` added to BOTH `ChatMetadataSchema` and `ChatMetadataBaseSchema` immediately after `scenarioText`, so v4's `generateDDL` (`schema-translator.ts`: a `ZodObject` field → `TEXT`, stored as JSON; nullable-optional → `DEFAULT NULL`) renders a fresh `chats` with `"scenarioSource" TEXT DEFAULT NULL, "scenarioNotes" TEXT DEFAULT NULL` after `"scenarioText" TEXT` — `fresh_schema.json` must be RE-DUMPED from a pin at or past `545e299bb` (D23, never by hand) and the migration re-homed as an UNSTAMPED table-gated boot ensure differentially compared against v4's REAL migration (the `add-memory-tiers-v1` shape, P4.D265). No index. `DDL.md` +2 column lines (`scenarioSource` "written only beside `scenarioText` by `resolveScenarioSelection`'s two callers; NULL = custom scene or pre-source chat"). **No dependency move** (stamps `.149`; no lockfile hunk). **NOT a convergence** (no bug number). **(a) The chokepoint:** NEW `lib/scenarios/location.ts` (566 lines) — `resolveScenarioLocation(scope: 'character' \| 'project' \| 'group' \| 'general', id, repos, userId, { ensure })` (overloaded: a store scope returns a `StoreScenarioLocation` with writes; the CHARACTER location is READ-ONLY — `list` reads the vault's `Scenarios/` documents through `parseScenarioFile` so paths are the files' real paths, the default from the row's `defaultScenarioId`; archived and other users' characters resolve null), `classifyScenarioMount` (a mount is a scenario owner when it is a live character's vault, a project's or group's OFFICIAL store, or General — NOT linked stores, NOT arbitrary Scriptorium stores), `lookupScenarioRef` → `ok \| missing \| refused` (one row read per character ref — the location is built from the row that classified the mount), `characterScenarioIdForPath`, `parseScenarioForScope`, `normalizeScenarioPath`, `ScenarioConflictError` / `NotFoundError` / `FilenameError`, `GENERAL_OWNER_NAME = 'Quilltap General'`; NEW `everywhere.ts` (`listScenariosEverywhere` — one document query, name / description / flags / a 240-char excerpt ending `…`, `scenarioId` on character rows; NO bodies) and `source-remap.ts` (client-safe). **DELETED:** `lib/mount-index/general-scenarios.ts` (−115), `project-scenarios.ts` (−91), `group-scenarios.ts` (−104), `resolveStoreScenarioTier`; `scenarios-common.ts` −147 (the test-only `listScenariosInFolder` / `readScenarioByPath` / `resolveScenarioBody` gone); the group tier's `Knowledge/` helper → `lib/mount-index/group-knowledge.ts`. NEW raw finders `characters.findByDocumentMountPointIdRaw`, `AbstractStoreBackedRepository.findByOfficialMountPointIdRaw` (projects + groups share it). `instrumentation.ts`'s General heal → `resolveScenarioLocation('general', null, repos, SINGLE_USER_ID, { ensure: true })`; `ensure-project-scenarios.ts` / `ensure-group-scenarios.ts` report `{ total, provisioned, healed, failed }` (an `ensure: true` folder failure RETHROWS — was swallowed); the project / group create hooks go through the location. **(b) Routes:** `scenario-item-route-factory.ts` (431 lines rewritten) serves General as well as projects and groups + `createScenarioCollectionBodies` (list + create); all six route files are thin configs (`scenarios/route.ts` −167 → `GET ?action=everywhere[&includeArchived=true]` `{ sources }`, `GET ?action=read&mountPointId=&path=` — 400 `mountPointId and path are required` / `Invalid scenario ref: <reason>`, 404 `Scenario`; `scenarios/[scenarioPath]/route.ts` −251; the project / group collection routes −150 / −148; `groups/scenarios` the union); item routes resolve with `ensure: true` on PUT / rename / DELETE (plus the group's `Knowledge/` via `afterEnsure`) while GET stays a probe and an owner with no store is still 404; the character scenarios route (`/characters/[id]/scenarios`) is UNCHANGED (id-addressed, through the repository); a character location update carries `defaultScenarioId` across a retitle and refuses a retitle onto an existing file. `chats` POST (`route.ts`, 66 lines changed): `scenarioRef: ScenarioRefSchema.optional()` in the body; `resolveScenarioSelection` takes `{ repos, userId, projectId, characterId, logTag }` (was `character`) and RETURNS `ResolvedScenario { scenarioText, scenarioSource, scenarioNotes }` (was a string) — precedence `scenarioRef` FIRST, then `scenarioId` > project > group > general, each failing soft; a refused ref throws `ScenarioRefRefusedError` → 400; the row is written with all three. `chats/[id]/actions/scenario.ts` (206 lines changed): `scenarioRef: ScenarioRefSchema.nullish()`, `reload: z.boolean().optional()` — WITHOUT `reload`, a request that resolves to the chat's OWN source file, revised since it was picked, keeps the stored preset body (`recoverPresetBody` from `scenarioText` minus the `\n\n<notes>` suffix, verified against the source hash) with the new notes beneath (debug `[Chats v1] Source file revised; keeping the chat's scene (no reload asked)`); `reload: true` pulls the revision; the no-op check `sameResolvedScenario` compares text + source + notes — a change of provenance or notes with byte-identical text is WRITTEN but neither recompiles nor announces; INFO `[Chats v1] Scenario changed` gains `source`. NEW `GET /chats/[id]?action=scenario-source` (`handleGetScenarioSource`: the source, `scenarioName`, `scenarioId` for a character source; debug `[Chats v1] Scenario source checked`). `lib/chat/scenario-text.ts` `recoverPresetBody`. `topic-map.ts`: the `chats` topic also invalidates `queryKeys.chats.scenarioSource(id)`. **(c) Carriers:** `qtap-export.schema.json` chat `scenarioSource` (object \| null; `mountPointId` "remapped with the document stores on import; dropped to null when that store did not come across") + `scenarioNotes` (string \| null); `quilltap-import/reconcile.ts` remaps `scenarioSource.mountPointId` in `reconcileRelationships` (mapped → new id; unmapped but present in this instance → kept; else → null; debug `Remapped chat scenario source`) — chats import BEFORE document stores; `restore/uuid-remap.ts` (new-account mode) through `remapScenarioSource` + the shared `UuidRemapper`. Backup: whole-table, nothing to do. SillyTavern export: not represented. **(d) SPA (Phases C–D):** NEW `components/scenario/ScenarioBrowserDialog.tsx` (399 lines — every character vault / project / group / General, searchable, a preview fetched for the highlighted row, `staleTime: 0`, no polling), `ScenarioSelect.tsx` gains **Browse all scenarios…** beside Custom (a pick outside the hierarchy shows under an **Elsewhere** optgroup as *Owner › Name*; one inside snaps to its native kind) and ALWAYS renders the dropdown; `ChatScenarioControl.tsx` (+201: the provenance line "From: *Owner* › *Name*", **Reload from source** when the file exists and its hash differs, a notes box under presets — the text box is the notes under a preset and the whole scene under Custom, always sent as `scenario`; a source whose file is gone is not selectable; leaving Custom for a preset clears the box); `NewChatForm.tsx` / `useNewChat.ts` / `types.ts`: ONE `scenarioSelection: ScenarioSelection` replaces the four pointer fields in form state (the wire still accepts all four + `scenarioRef`), seeded once by `seedScenarioSelection` (character default > project default > general default; continuation mode skips the character default; not re-seeded once `scenarioTouched`) — which FIXES the form showing the project default while submitting the character's; the project-default override note only when the project default is the selection and the character has one of its own; `useScenarioRefBody`; `lib/query/keys.ts` (+12). **(e) Help / docs:** NEW `help/scenario-browser.md` (52 lines — the help tree goes 131 → 132), `help/chats.md`, `general-scenarios.md`, `groups.md`, `project-scenarios.md`; `API.md` (+28), `DDL.md`, v4 `CLAUDE.md` (the chokepoint rule), `features/{ => complete}/scenarios-from-anywhere.md` §7 As built, `features/scenario-builder.md` (one line — the Builder is UNTOUCHED: its mount pool and `SaveScenarioDialog` already know all four owner kinds). **Its own lane group** — shares nothing with the wardrobe set | `db/scenarios.rs` (the chokepoint — `ParsedScenario`, P4.6n / P4.D208 / P4.D219) + `api/scenarios.rs` + `api/projects.rs` / `api/groups.rs`'s scenario arms + `services/builtin_mounts.rs` (General's folder heal) + `quilltap-host/src/host.rs` (the startup heals) — the P4.6n scenarios round (2026-07-11, the three tier modules v5 ported as they were) + P4.D129's smalls; `services/scenario_selection.rs` (P4.D115 unit 1 — returns the TEXT) + `services/chat_scenario.rs` (the `?action=scenario` verb, P4.D115 units 3–4 / P4.155 unit 3) + `scenario_text.rs`; `services/chat_create.rs` (P4.4 unit 2 — the create's scenario resolve); `db/chats.rs` + `chats_read.rs` (the row — no `scenarioSource` / `scenarioNotes` columns); `services/quilltap_import/reconcile.rs` (P4.155) + `entities.rs` + `services/backup/uuid_remap.rs` + `uuid_remapper.rs` + `restore/orchestrator.rs` (P4.9G / P4.D264); `provisioning/fresh_schema.json` (the SEVENTH re-dump — P4.D265; EIGHTH pending); the SPA `scenario/scenario-select.ts` + `scenario.api.ts` + `scenario.types.ts`, `chat/sidebar/chat-scenario-control.ts` (P4.D116 — no `reload` / `scenarioSource` / `scenarioRef` today), `screens/new-chat/new-chat.state.ts` + `new-chat.logic.ts` + `new-chat-form.ts` (the New-Chat vertical — four pointer fields), `core/realtime-topic-map.ts`, `help/` (p4.9i2 / P4.D278). Reddens (predicted, on a HEAD regen): the general / project / group scenario routes families (the modules are GONE; `?action=everywhere` / `read` NEW), the chat-create route (`scenarioRef`; the two new row keys), the `chat_scenario` action family (`reload`, the kept-scene arm, the no-op compare, the `source` field), the chat GET dispatch (`scenario-source`), the `.qtap` chat export / import / reconcile families + the new-account restore remap, `provisioning_equivalence` (two columns — the D23 tripwire firing as designed), `system_backup_equivalence` (the chat row), `help_tree_embed_guard` (132) / `help_tree_equivalence` / `host_help_docs_boot`, `qtap_schema_embed_guard` / `public_schemas_vendor_guard`, the realtime topic-map spec. ⚠ Live-data hazard once v4 runs past this on Friday: §1's real-instance note (1) | **ORDERED(P4.D288 (the D23 columns + `add-chat-scenario-source-v1` ensure + C2), P4.D295 (the server), P4.D302 (the SPA), P4.D293 / P4.D294 (the carriers), P4.D303 (help))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `124c8aca3` | 2026-10-12 | Fix bugs 199–201: memory gate, continue turns, chat wipe (#87) | **PORT** (classified 2026-10-12 from the hunks, §5.3 — read from `origin/main`, NOT yet pulled into the human's checkout; 56 files, +2,697 / −288 — a squashed PR of three commits: Phase 0, a version bump after merging main, the review fixes; 24 NEW open bug files 203–226 + four `fixed/` 199–202 + `bugs.md` +32 are 29 of the 56; nine code files). **NO schema / DDL / migration / index move; no dependency move** (stamps `.151`; no lockfile hunk). **NOT a convergence** — bugs 199–202 are v4's own 2026-10-12 memory audit, each `Original to v4` (the port's filings are 180, 183, 184, 185); 203–226 stay OPEN, each naming the refactor phase that closes it. ⚠ **Bug 199's trailing-only fix here is REWRITTEN by `69d38dd60` an hour later** (the one list) — port the two together. **199** `context-manager.ts` `buildContext`: the no-new-user-message branch (`else if (userNarrationAnchor \|\| coreWhisperLLMContext \|\| llmRecallText \|\| suparnaMailLLMContext \|\| turnSkipInstruction \|\| progressionsLLMContext \|\| informBlock)`) now pushes `[userNarrationAnchor, coreWhisperLLMContext, llmRecallText, suparnaMailLLMContext, progressionsLLMContext, informBlock, turnSkipInstruction]` (was `[userNarrationAnchor, progressionsLLMContext, informBlock, turnSkipInstruction]` — recall, mail and the Core whisper were POSTED, the rows stamped accessed and recorded in the anti-repetition ring, the letters marked alerted, and the model never saw them on continue / chained / autonomous turns); debug `[CommonplaceWhisper] Delivering recall in the trailing-only message (no new user message)` `{ chatId, participantId, length }`. **200** `memory-gate.ts`: `GATE_MAX_SEARCH_K = GATE_TOP_K * 4`; the gate searches, loads the matched rows (`findByIds`), and if fewer than `GATE_TOP_K` are LIVE widens `k` by the shortfall and searches again until live ≥ K, the store is exhausted (`results.length < k`) or `k ≥ 4K`; `results` = live hits only, sliced to K; orphans WARN `[MemoryGate] Vector index holds entries with no memory row; ignoring them` `{ characterId, orphanIds, searchedK }` + `debugInfo` `[Gate] Ignored N orphan vector(s)`; orphans are NOT pruned there (in the job child a row this job inserted is invisible to the snapshot — bug 209 — and would look like one); `bestMemory` is non-null by construction (the `bestMemory !== undefined` / `&& bestMemory` guards on the distinct-occasion, near-duplicate and merge branches and the `.filter(r => r.memory)` DROPPED). The delete chokepoint grows `removeDeletedVectors(idsByCharacter)` (best-effort per character: `removeVector` each, `save()` when any removed; WARN `[MemoryGate] Failed to remove vectors for deleted memories`): `deleteMemoryWithUnlink` removes the vector and logs `vectorsRemoved`; NEW `deleteMemoriesWithUnlinkBatchReport(ids, options) → { deleted, vectorsRemoved }` with `deleteMemoriesWithUnlinkBatch` as its `.deleted` wrapper; `DeleteBatchOptions` exported. Callers simplified: `memory-service.ts` `deleteMemoryWithVector` = `deleteMemoryWithUnlink` (its own vector removal gone), `deleteMemoriesBySourceMessageWithVectors` / `…BySourceMessagesWithVectors` through the report (their per-character loops gone; `characterCount` from a Set), `housekeeping.ts` runHousekeeping's vector cleanup DELETED, `memory-dedup.ts`'s vector cleanup DELETED, `DELETE /memories/[id]` calls the chokepoint directly (no second read) and answers 404 `Memory` when it returns false. **201** NEW client-safe `lib/memory/policy-protection.ts`: `policyProtectionOf({ source, tier })` → `'manual'` (source MANUAL) \| `'digest'` (source CONSOLIDATED) \| `'cold'` (tier cold) \| null, `partitionByPolicyProtection(rows) → { actionable, spared, sparedBy: { manual, digest, cold } }`. `deleteMemoriesByChatIdWithVectors` returns `{ deleted, vectorsRemoved, characterCount, spared }`: partitions `findByChatId`, then a spared DIGEST goes too when NO member not being wiped still names it (`consolidatedFrom` minus the doomed ids → `findByIds` once → `supersededById` set; a member re-promoted or re-consolidated elsewhere no longer speaks for it); INFO `[Memory] Chat wipe spared manual, cold and digest rows` `{ chatId, spared, manual, cold, digests }`; `handleDeleteByChatId` answers `sparedCount` and its debug is retitled `[Memories API] Wiped a chat's extracted memories` `{ chatId, deleted, spared }` (was `Deleted every memory for a chat`); `memory-regenerate-chat.ts` logs `spared` at both sites. `deduplicateCharacterMemories` folds only `actionable` rows (`originalCount` = total; `sparedCount` on every result incl. the `< 2` early returns and `deduplicateAllMemories`' per-character rows; debug `[MemoryDedup] Candidate rows for folding` `{ total, candidates, sparedBy }`); the card shows "Kept aside"; `useMemoryActions.ts` / `SalonListView.tsx` / `character-conversations-tab.tsx` wording. **202** comments only (`searchMemoriesSemantic`: anchor hits widen the pool and face the floor — R5). `help/chat-participants.md`, `memory-regenerate.md`, `system-tools.md` one line each; `features/memory-system-refactor.md` (+10 — Phase 0 marked done); v4 `CLAUDE.md` | `services/build_context.rs:3937-3955` (P4.D254's trailing-only list — EXACTLY the pre-fix four-item list: scene note, progressions, inform, turn note; `core_whisper` / `llm_recall_text` / the mail are computed above it and dropped on that branch — v5 REPRODUCES bug 199), `services/memory_gate.rs:444` (`store.search(embedding, GATE_TOP_K)` once; `:471` `if let Some(best_memory) = best_mem.as_ref()` — the pre-fix optional guard; P4.D266), `db/memories.rs:1052` / `:1141` (`delete_with_unlink` / `delete_many_with_unlink` — rows + neighbour scrub, NO vector removal; P4.d12), `services/memory_service.rs:189` (`delete_memories_by_chat_id_with_vectors` — every row with the `chatId`, no partition; `cascade_delete` + `remove_vectors_grouped` per caller; P4.D126 / P4.D270), `services/housekeeping.rs` (its own vector cleanup; P4.D267), `services/memory_dedup.rs:229` + `:461` (`deduplicate_character_memories`, every tier and source folded, its own `remove_vector`; P4.43 / P4.D268), `api/memories.rs:706` (the wipe route — no `sparedCount`) + `:644` (the item DELETE), the regenerate-chat job, the SPA `memory/` dedup card + the Salon list wipe wording (P4.D271). Reddens (predicted): the memory-gate tier-3 family (the orphan widen + WARN; the dropped guards), the chat-wipe + regenerate-chat families (`spared`), the dedup family (`sparedCount` + the debug), the housekeeping family, the memories item DELETE family (404), the delete-chokepoint tier-2 (`vectorsRemoved`), the buildContext families on a continue / chained / autonomous turn (the trailing-only content). **Order TOGETHER with `69d38dd60`**; ⚠ live-data hazard (the wipe): §1's real-instance note (3) | **ORDERED(P4.D288 (bugs 200 / 201 in the ONE retire path), P4.D290 (the orphan widen as it survives), P4.D291 (the trailing-only recall; bug 199), P4.D298 (the chat wipe route))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `69d38dd60` | 2026-10-12 | Fix bug 227: mail marked announced before its turn succeeds; one trailing-section list (#88) | **PORT** (classified 2026-10-12 from the hunks, §5.3 — read from `origin/main`, NOT yet pulled; 17 files, +635 / −56 — a squashed PR of three commits: 227, the 199 completion, the review follow-up; six code files, four tests, `help/post-office.md`, the 227 bug file + the 199 file's +7). **NO schema / dependency move** (stamps `.153`; no lockfile hunk). **NOT a convergence** — bug 227 is `Original to v4. Present since the Post Office's mail check was added to buildContext`, found 2026-10-12 in review of the 199 fix. **(a) The mail flip is deferred to delivery:** `buildContext` no longer imports `markAlerted`; `BuiltContext` gains `mailAlertsOwed: MailAlertsOwed \| null` (`{ vaultId, paths }` — the letters Suparṇā announced this turn, "like `informRowIds`, marked only once the turn has delivered them"); the mail block runs only when `mailVaultId && !isRegeneration` (`isRegeneration = (regenerationOfMessageIds?.length ?? 0) > 0` — **a swipe skips the mail check altogether**: a re-roll of a line already spoken must not carry a letter that arrived since, and the swipe path never reaches the finalizer that would mark it); on announce, debug `[Suparna] Mail announced; alerted flip deferred to delivery` `{ chatId, characterId, vaultId, letters }`. NEW `mailbox.ts` `markMailAlertsDelivered(owed, { chatId, via })` — `markAlerted` per path, warn-only per letter (`markMailAlertsDelivered: could not mark letter alerted`), debug `markMailAlertsDelivered` `{ chatId, vaultId, via, requested, marked }`. Call sites: `message-finalizer.service.ts` after the inform mark (`via: 'finalizer'`); `primary-stream.service.ts` `makePreservePartialOnError` takes `mailAlertsOwed` and marks `via: 'preserved-partial'`; `orchestrator.service.ts` passes it to the preserve-partial options at both sites, marks `via: 'tool-only'` after a tool-only save ("the model read this turn's mail and acted"), and `handleTurnSkip` takes `mailAlertsOwed` and marks `via: 'turn-skip'` ("a pass is an answer"). A failed or aborted turn therefore announces the letters AGAIN next attempt — at the cost of a second whisper, "the honest record of a letter re-announced". **(b) One trailing-section list:** `trailingContextSections = [coreWhisperLLMContext, llmRecallText, suparnaMailLLMContext, progressionsLLMContext, informBlock, turnSkipInstruction].filter(Boolean)` is built ONCE before the branch and used by BOTH — the new-user-message branch's hand-kept six-push copy DELETED, and the no-new-message branch becomes a plain `else` pushing `[userNarrationAnchor, ...trailingContextSections]` ONLY when non-empty (was pushed whenever any section existed; the PR's review note that "the Core whisper was also missing from turns with no new user message" describes the pre-Phase-0 list — `124c8aca3`'s seven-item line already carried it, and the one list keeps it); the comment carries the adjacent-user-message note (a chained turn's history routinely ends in a `user`-attributed line from another seat; OpenAI-compatible APIs accept adjacent user messages, Anthropic's combines them, the Google plugin merges them before sending). `help/post-office.md`: the check runs "whether the character is answering you or simply carrying on unprompted", and "the one exception is a turn that comes to nothing … Suparṇā will read it again on the character's next turn". Tests pin the order on both paths, the Core whisper case, the deferral and the swipe skip | `services/suparna_mail.rs:101` (`mark_alerted` AT COLLECT TIME inside the mail check — v4's pre-fix shape; P4.9E2A / P4.D234's Post Office, the P4.D45 string table), `services/suparna_notifications.rs:265`, `post_office/mailbox.rs:395` (`mark_alerted`; no `mark_mail_alerts_delivered`), `services/build_context.rs:3937-3955` (the trailing-only branch; the new-message branch's own list above it — two hand-kept copies, as v4 had; P4.D254), `services/message_finalizer.rs:1047-1090` (the inform mark — `inform_row_ids` empty on a swipe; the mail mark goes beside it), `services/primary_stream.rs:677` (`inform_row_ids` on the preserve-partial options), `services/orchestrator.rs` (the tool-only save + the turn skip), `tools/read_mail.rs:166` (the tool's own mark — unchanged by v4). Reddens (predicted): the buildContext tier-3 families on both branches (the one list; `mailAlertsOwed` in the built context; the deferred-flip debug; the swipe's missing mail block), the mail tier-3 family (the `alerted` flip's timing — it now happens AFTER the model call: a mocked-LLM failure must leave the letter unalerted), the finalizer / preserved-partial / turn-skip families (the four `via` lines), `help_tree_equivalence` on `post-office.md`. **Order TOGETHER with `124c8aca3`** (it rewrites that commit's `trailingOnly` line) | **ORDERED(P4.D291 (bug 227 — the deferred mail flip, one trailing-section list), P4.D303 (help))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `8050958be` | 2026-10-12 | Fix bug 228: importing a chat with "Create New" dropped every message (#89) | **PORT** (classified 2026-10-12 evening from the hunks, §5.3; 20 files, +910 / −69; `4.10.0-dev.154`, parent `69d38dd60`. `lib/` hunks ONLY in `lib/import/quilltap-import/`: NEW `message-id-remap.ts` (169 lines — `remintChatMessageIds`, `remapImportedMessageId`), `import-entities.ts` (`importChats`: the `duplicate` and fresh-id create paths made ONE; a chat created under a fresh id gets fresh message ids, each occurrence its own even when a bundle repeats a source id; `swipeGroupId` rewritten only in its `swipe-<messageId>` form; `summaryAnchorMessageIds` remapped, an unresolvable anchor CLEARING the summary + anchors + `lastSummaryTurn` together as rebuild-summary does; `otherExtractionWatermarkMessageId`; `courierCheckpoints[*].lastResolvedMessageId`, the JSON-string form read, unreadable entries dropped; the duplicate branch now maps the source chat to the REAL created id — the phantom `idMaps.chats` entry is gone), `types.ts` (NEW `IdMappingState.messages: Map<sourceChatId, Map<sourceMsgId, newId>>`, written only after an insert lands), `execute.ts` (annotation `sourceMessageId` through the remap), `reconcile.ts` (`remapChatInform`: `recordMessageId` / `consumedByMessageId` through the remap; a `null` = reminted-but-never-landed treated as missing; the refusal `reason` keeps the SOURCE id), `import-files.ts` (`remapLinkedTo` resolves a message only through a chat named in the same `linkedTo` array). The memories import `sourceMessageId` follows the same map. `help/system-import-export.md` +1 sentence (the "(imported)" copy keeps its transcript). Tests: NEW `duplicate-chat-message-ids.test.ts` (445 lines) + five touched. NO DDL, NO migration, NO dependency (stamps only). **CONVERGENCE check: NONE** — `Provenance: Original to v4`, found live on `V4test`, `v5 status: Not assessed`.) | **v5 SHARES the defect**: `services/quilltap_import/entities.rs:791` `import_chats` adds each message under its bundle id (`ChatMessagesRepository::add_message` with the decoded event) and keeps v4's ruled "Phantom-map quirk" (`entities.rs:829-834` + the module header, `DUPLICATE_MINTS`) — ported faithfully in the `.qtap` import family (P4.9G-era import EXECUTE; the bare-tail warnings P4.147/#140, P4.155 whole-entity refusal, P4.161 whole-row validation), so the phantom-map pins and every duplicate-strategy chat row flip by design. Sidecars to follow: `quilltap_import/reconcile.rs` (the inform remap, P4.155 / P4.D249's `permanent` carrier), `quilltap_import/files.rs` (`linkedTo`), `quilltap_import/memories.rs` (`sourceMessageId`; P4.161 / P4.D272's up-front id minting), the annotation import; the restore path (`backup/`) is NOT touched — restore preserves ids. Shares `reconcile.rs` + the chat import with `545e299bb`'s scenario reconcile — fold into or sequence with that lane (§1). `help/` re-vendor of one page. | **ORDERED(P4.D294 (bug 228 — reminted message ids on a fresh-id `.qtap` chat import))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `97f0d8cb8` | 2026-10-12 | fix(wardrobe): a click inside the folder picker no longer closes the wardrobe dialog (bug 199 → renumbered 229 by `4ce64fa1e`) | **PORT** (SPA-only rider on `4a38222e6`; classified 2026-10-12 evening from the hunks; 10 files, +167 / −26; `4.10.0-dev.150`, parent `545e299bb` — the close probe's recorded dirt, committed: `components/wardrobe/WardrobeBrowser.tsx` drops its local `picker` state for `actions.folderPicker`; `hooks/useWardrobeItemActions.ts` owns the picker state and counts it in `overlayOpen`, so a mousedown inside the portalled listbox no longer reaches the dialog's click-outside close; the jsdom folders test +1 case; `bugs.md` + the bug file; `package-lock.json` hunk = the root `version` lines only. No `lib/` / `app/` / `packages/` / `plugins/` hunk. **CONVERGENCE check: NONE** — v4's own SPA defect in a feature v5 has not ported.) | None today — v5 has no wardrobe folder browser or Move-to-folder picker; the SPA half of `4a38222e6` lands it in `apps/web/src/app/wardrobe/*` (P4.D261 / P4.D277's browser + dialog). Port the picker with its state in the actions service and counted by the dialog's overlay gate from the start, and carry the bug-229 test as a Playwright / component beat. | PORTED(P4.D285) — baseline not yet past (R-P: ported AHEAD of the baseline from its SPA hunks — the picker landed WITH the bug-229 fix in the `4a38222e6` round, unified 2026-10-13; the row stays until a later round passes it) — the `83334b9a5` round (ordered 2026-10-14) carries the baseline past it: retires as `ABSORBED(P4.D285)` at that unification (§S.6) |
| `4ce64fa1e` | 2026-10-12 | Merge origin/main; renumber the folder-picker bug to 229 | **NO-PORT?** (merge of `97f0d8cb8` and `8050958be`; classified from its OWN resolution, `git show --cc`: the README badge, the version stamp `.155` (both `package.json`s + the lockfile's root `version` lines), and the picker fix RENUMBERED 199 → 229 — the bug file renamed to `bugs/fixed/bug-229-folder-picker-closes-dialog.md`, `bugs.md`, two code comments and one test title. `git diff 8050958be 4ce64fa1e` equals `97f0d8cb8`'s content plus that renumber; no behaviour of its own. Resolves the close probe's predicted bug-199 collision in favour of the memory audit's 199.) | None — ratify with `97f0d8cb8` in the wardrobe round. | **ORDERED(P4.D303 — NO-PORT ratification)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `c2795733f` | 2026-10-12 | Fix bugs 203–226: memory system defects from the refactor audit (#90) | **PORT — WITH A SCHEMA MOVE (FIVE migrations)** (classified 2026-10-13 from the hunks, §5.3; 154 files, +10,585 / −1,247; `4.10.0-dev.157`, parent `4ce64fa1e`; 24 bug fixes, every one with an implementing hunk — the message names three migrations, FIVE ship, all `introducedInVersion '4.10.0'` with PRETTY labels in `lib/startup/prettify.ts`). **Migrations:** `add-episode-extraction-watermark-v1` (bug 207, `dependsOn add-memory-tiers-v1`) — `chats.episodeExtractionWatermarkMessageId TEXT DEFAULT NULL`, backfilled from the LAST entry of each chat's `summaryAnchorMessageIds` (no anchors → NULL; DROPPED again by `ac6348c5f`); `add-memories-superseded-by-index-v1` — `CREATE INDEX IF NOT EXISTS "idx_memories_supersededById" ON "memories" ("supersededById") WHERE "supersededById" IS NOT NULL` (partial; moves `migration_indexes.json`); `add-memory-superseded-at-v1` (bug 225) — `memories.supersededAt TEXT DEFAULT NULL`, backfilled from `updatedAt` where `supersededById` is set, then stamped / cleared by `updateTierBulk`; `recompute-reinforced-importance-v1` (bug 216, `dependsOn add-memory-gate-fields-v1`) — EVERY row's `reinforcedImportance` rewritten to `min(1, importance + log2(max(1, count)) × 0.05)` in batches of 500 (was `log2(count + 1)`: a count-1 row sat at importance + 0.05); `rezone-legacy-memory-occurred-at-v1` (bug 212, `dependsOn add-episodic-memory-fields-v1`) — every `occurredAt` that is exactly a UTC midnight re-stamped at local midnight on the instance's memory calendar. `chat.types.ts` + `memory.types.ts` gain the two columns (generateDDL moves); the `.qtap` export schema gains chat `episodeExtractionWatermarkMessageId` and memory `supersededAt`. **Per bug:** 203 `editMemory` embeds BEFORE the write (`MemoryReembedError`; PUT 502, 400 on `relatedMemoryIds`); 204 re-embeds through `buildMemoryEmbeddingText(…, tz)`; 205 rebuild / missing-embeddings keep the tier; 206/207 NEW `fold-window.ts` (watermarks, slices of 60, ≤ 4 per run, `continue:<id>` follow-ups) + the episode watermark; 208 NEW `commonplace-mirror.ts` — the parent mirrors after commit (the child refuses, an emptied bucket file is removed, a digest delete re-mirrors; host-RPC `writeCommonplaceDigestsToVault` REMOVED); 209 a per-job memory overlay in the child proxy; 210 `countCapBreakdown { hotTotal, demotable, protected, cold }`, housekeeping demotes AUTO only, lowest protection first ("Exceeded demotable-memory limit (N)"); 211 the consolidation corpus admits unsuperseded cold rows (`demotedCandidates`); 212 NEW `memory-calendar.ts` — ONE zone (ChatSettings.timezone → `QUILLTAP_TIMEZONE` → system) through anchors, day refs, the extraction clock, `timeRange` and the embedding text (`isoDateFromMs` removed); 213/222 `extraction-route.ts` (the fold episode's `uncensoredFallback`; Carina passes projectId / projectDescription / chatContextSummary); 214 the text fallback takes the full semantic options; 215 recap bands on reinforced importance (≥ .7, [.3, .7), < .3); 217 NEW `host/vector-store-invalidation.ts` (owners resolved before commit); 218 POST `/memories` MANUAL-only (`source` / `skipGate` stripped; projectId / kind / occurredAt / entities; 404s; `{ memory, action }` 201 / 200) + `promoteMatchedMemory`; 219 GET no longer stamps access time; 220 `createMemoryDirect` embeds first, retries once after 500 ms, null on failure (`SKIP_EMBEDDING_FAILED`); 221 `enqueueMemoryHousekeepingSweep` (one job per character; `housekeep-sweep` → `{ success, jobId, jobIds }`); 223 consolidate passes `clusterThreshold` (a PENDING duplicate adopts, PROCESSING warns); 224 the inter-character block skips memories already whispered; 225 cold retention ages by `COALESCE(supersededAt, updatedAt)` + `scrubDigestLinks` on delete; 226 a merge writes a DELTA (`incrementReinforcement { by, at }`, never rewinds `lastReinforcedAt`). SPA: the memory editor drops `source`; the 200-create toast "Already on file — the existing memory has been reinforced instead"; the housekeeping card "N counted · M protected". `help/` seven pages (characters, chat-settings, cli-memories, episodic-memory, memory-consolidation, memory-housekeeping, memory-regenerate); `bugs.md` 1–229 fixed, 230–234 open. **No dependency move** (stamp only). **NOT a convergence** — 203–226 are v4's own audit, each `Original to v4`. Every later memory commit builds on it (Phase A renames / deletes `memory-calendar.ts`, `reinforced-importance.ts`, `fold-window.ts` hours later) | v5 carries v4's PRE-fix shape on nearly every surface — the memory programme P4.D265–P4.D276 and its predecessors: `memory_gate.rs:45-46` (`calculate_reinforced_importance` = `log2(count + 1)` — the Phase-1 `memory_weighting` tier-1 pins the PRE-fix formula), `api/memories.rs` (POST / GET / PUT; P4.D269 / P4.D276), `services/memory_gate.rs` + `memory_merge.rs` (P4.D266), `services/housekeeping.rs` (P4.D266 / P4.D267 — demote order, no cap breakdown), `db/memories_read.rs` (the GET access stamp), `services/memory_service.rs`, `services/embedding_{generate,reindex}_job.rs` (P4.6BL; the embedding-status marks P4.D25), `services/episodic.rs` (anchors, day refs, the embedding text — P4.d14 / P4.D267 / P4.D269; bug 212's zone sits beside P4.119's host-zone dates), `services/fold_{other,episode}_pass.rs` (P4.D270), `services/carina_memory_extraction.rs`, `services/consolidation.rs` + the Commonplace canon (P4.D271 / P4.D270), the SPA `memory/` cards (P4.D276); `fresh_schema.json` + `migration_indexes.json` (P4.D265's SEVENTH re-dump); the `.qtap` carriers (P4.D272); backup / restore (P4.9G / P4.D264). Reddens at a HEAD regen (predicted): every family touching reinforced importance, the gate / merge / housekeeping / consolidation / fold / Carina families, the memories routes, `provisioning_equivalence` (two columns), `migration_indexes` (one partial index), `qtap_schema_embed_guard` + `public_schemas_vendor_guard`, `help_tree_equivalence`. ⚠ LIVE-DATA HAZARDS: §1 (4) the formula, (3) the watermark positions. Order WITH `124c8aca3` + `69d38dd60` and Phases A–D as ONE memory catch-up | **ORDERED(P4.D288 (five migrations as ensures; repository verbs), P4.D289 (the formula, bug 212's calendar), P4.D290, P4.D291, P4.D293 / P4.D294 (`supersededAt`), P4.D296, P4.D297 (bugs 208 / 210), P4.D298, P4.D300 (the editor's dropped `source` + the reinforced-instead toast — measured: they landed here, not in Phase C), P4.D303)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `e0ac7493a` | 2026-10-12 | Memory refactor Phase A: scoring, clock, settings, candidate (#91) | **PORT — WITH A SCHEMA MOVE (one migration, three DROP COLUMNs)** (classified 2026-10-13 from the hunks, §5.3; 166 files, +5,887 / −2,781; `4.10.0-dev.160`, parent `c2795733f`). **Migration `move-memory-maintenance-settings-v1`** (`'4.10.0'`): copies the single user's `chat_settings.autoHousekeepingSettings` (else the latest row's) into `instance_settings.memoryMaintenance` (keeping `enabled` / caps; never overwriting an existing key), DELETES the `memoryExtractionConcurrency` instance key, then `ALTER TABLE "chat_settings" DROP COLUMN` `autoHousekeepingSettings`, `memoryExtractionLimits`, `memoryExtractionConcurrency` (each if present); `restore.ts` +4 / −4 translates old backups the same way; its two predecessors' edits here are import-path moves only. **Behaviour:** scoring moves to `lib/memory/scoring/*` (same formulas: protection score, the .75 / .25 ranking blend, min cosine .30 / .10, reinforced importance, salience = the old raw weight) and the FLOORED `calculateEffectiveWeight` (max(raw, .70 · base)) and `rankMemoriesByWeight` (the < .05 filter) are DELETED — orderings change in the memory injector (the primary sort, the inter-character half, the dynamic head's fallback), the frozen-archive fill, the optimizer, and first-message context (salience, was raw importance); `RECALL_MIN_IMPORTANCE` .3, `TOOL_MIN_IMPORTANCE` 0, bands high ≥ .7 / low < .3; EVERY `minImportance` filter compares `reinforcedImportance ?? importance`; the per-turn recall floor .5 → .3; the medium band boundary .4 → .3 (`search_scriptorium`, the card / editor). Time (`lib/memory/time/*`): read windows close 1 ms before the next boundary; the WRITE side stamps the window START ("today" → local midnight, "this week" → Monday 00:00); number words 11–14, "a couple / few … earlier"; `search_scriptorium` since / until in instance-zone local days (was UTC). NEW Zod `MemoryCandidateSchema` (the old type renamed `MemoryDraft`; importance clamped [0, 1] — a 0 stays 0, was `\|\| 0.5`; blank content / summary fall back to each other, both blank skipped; entities ≤ 12). Maintenance instance-wide (`resolveMemoryMaintenance` / `capFor` override → global → 2000; caps 100–100k). Bugs 230 (the dry run passes project context), 231 / 234 (SQL-delta link methods, `appendCappedFootnotes` skips existing lines, `neighboursTouched`), 232 (NEW fold-episode catch-up job — scheduled-maintenance step 9, `continue-episode:<wm>`), 233 (search-replace through `editMemory` per row; `replaceInMemories` removed). Prompts: the extraction clock drops "Current in-story time"; the fold-episode prompt drops `participants`. **Wire:** `?action=maintenance-config` GET / POST (the housekeeping-config alias kept) answering `{ success, settings: { enabled, perCharacterCap, perCharacterCapOverrides } }`; `extraction-concurrency` REMOVED (unknown-action 400); housekeep POST `maxMemories` 100–100,000 defaulting to the character's cap (was 2000), `merged` / `mergedIds` dropped; the GET preview's new 400 "maxMemories must be an integer between 100 and 100000", `wouldMerge` dropped, `capUsed` added; search `minImportance` default 0. Tools: `search_scriptorium` (`effectiveWeight` → `salience`), `self_inventory` ("weight X" → "salience X", unfloored). Almanack: the "**Memory maintenance**" line; "Memory Cascade & Housekeeping" → "Memory Cascade", four lines removed. SPA: the housekeeping dialog (`capUsed`, 100–100k, no merged), the band, the card on `maintenance-config`. `help/` five pages; `bugs.md` 1–234 fixed. No dependency move. **IMPORT-DEAD at HEAD:** `lib/memory/memory-weighting.ts` (v5's Phase-1 `memory-weighting` oracle case), `reinforced-importance.ts`, `day-references.ts`; `episodic-anchors` / `memory-calendar` / `memory-calendar-refresh` renamed under `time/`; removed exports `resolveWhenPhrase`, `needsHousekeeping`, the extraction-concurrency setters, `replaceInMemories`, `DeleteBatchOptions`, the `AutoHousekeeping*` schemas, `effectiveWeight` / `rawWeight`, `FoldEpisode.participants`, `ExtractionClock.narrativeNow` | `db/chat_settings.rs` (reads / writes `autoHousekeepingSettings` + `memoryExtractionLimits` by name — `:1053-1055`, `:1118-1119`, `:1408`, `:1496`; `find_auto_housekeeping_settings_by_user_id` `:1740`) + `db/instance_settings.rs:527` (the concurrency key) + `fresh_schema.json`'s `chat_settings` (both columns; P4.D265's dump); `memory_weighting.rs` + the ranking blend (Phase 1 tier-1 — the floored weight v4 deleted); the memory injector / frozen archive (P4.D268), recall + search floors (P4.D267), `services/episodic.rs` + the anchors (P4.d14 / P4.D269), the candidate parse in the extraction passes (P4.D270), `services/housekeeping.rs` + the scheduled maintenance (P4.D266 / P4.D267), `api/memories.rs` (housekeeping-config, extraction-concurrency, housekeep, preview, search), the `search_scriptorium` / `self_inventory` tools, the Almanack memory lines, the SPA housekeeping dialog + card + band (P4.D276), restore's settings translation (P4.9G). ⚠ LIVE-DATA HAZARD: §1 (1) — the dropped columns break v5's settings reads and writes | **ORDERED(P4.D288 (`move-memory-maintenance-settings-v1` + the settings document), P4.D289 (scoring / clock / candidate), P4.D291 (the .3 floor), P4.D296 (the fold-episode catch-up, the clock), P4.D298, P4.D300, P4.D303)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `af48f6cc2` | 2026-10-12 | Memory refactor Phase B: index owns vectors, one deletion path (#92) | **PORT — WITH A SCHEMA MOVE (a destructive migration; RULED: MIRROR, 2026-10-13)** (classified 2026-10-13 from the hunks, §5.3; 117 files, +3,576 / −3,878 — a squash of seven; 59 production files; `4.10.0-dev.161`, parent `e0ac7493a`; no SPA, CLI or `bugs.md` change; `qtap-export.schema.json` untouched). **Migration `retire-memory-vector-entries-v1`** (`'4.10.0'`, registered LAST; `dependsOn` the six vector migrations from `sqlite-initial-schema-v1` to `quantize-embeddings-v1`; `shouldRun` while either table exists): pages `vector_entries` 500 rows a transaction and copies each vector onto a memory row with NO embedding (`UPDATE memories SET embedding = ? WHERE id = ? AND embedding IS NULL`; JSON-text vectors through `parseLegacyEmbeddingText` → `float32ToBlob`; counts `orphaned` / `alreadyOnRow` / `unusable`), then `DROP TABLE IF EXISTS` `vector_entries` AND `vector_indices`. INFO `Verifying memory vectors against their rows before retiring the mirror` / `Retired the memory vector mirror` {copied, alreadyOnRow, orphaned, unusable, tablesDropped}; ERROR `Retiring the memory vector mirror failed`. `normalize-vector-storage` no longer re-creates the mirror. **generateDDL:** `vector-indices.types.ts` + `VectorIndicesRepository` DELETED, so a fresh v4 instance ends with neither table. **Behaviour:** vectors live ONLY on `memories.embedding` (NEW `lib/memory/index-store/` — an in-process `memoryIndex`; `CharacterVectorStore.load` reads `SELECT id, embedding, tier FROM memories WHERE characterId = ? AND embedding IS NOT NULL`); ONE deletion path — NEW `lib/memory/retire.ts` `retireMemories(ids, { reason, keep? })` / `retireChatMemories` (ten reasons; `keep: 'manual'` spares MANUAL, cold and any digest with a living member; NEW: a digest whose `consolidatedFrom` empties is retired with its last member, except under `retention`; links scrubbed by SQL delta) REPLACING `deleteMemoryWithUnlink`, `deleteMemoriesWithUnlinkBatch[Report]`, `deleteMemoryWithVector`, `deleteMemoriesBy{SourceMessage,SourceMessages,ChatId}WithVectors`; the repository's bare `delete` THROWS (ERROR `Refused a bare memory delete; memories are deleted through retireMemories`), `bulkDelete` → `bulkDeleteUnsafe`, three delete helpers gone; `editMemory` → `admission/update-memory.ts` `updateMemory`. **Backup / restore:** no `vector-entries.json` / `vector-index-metas.json`, manifest counts dropped; restore step 22j gone — a legacy `vector-entries.json` only fills a row that arrived without an embedding (`legacyVectorsAdopted`); `embeddingReconcile` loses two keys; the delete-all service stops wiping the tables. **Wire:** `POST /memories?action=embeddings` drops `batchSize` (stripped — no longer a 400); the PUT rebuild's `failed` now means "memories without an embedding"; two re-attribution ERROR texts. Logs: INFO `Chat wipe spared manual, cold and digest rows` (the `[Memory]` prefix dropped); WARNs `Retirement rewrote an unusually large neighbour set`, `Memory index cache update failed; …`, `Skipped memory embeddings that could not be decoded`; the dispatcher's owner-lookup failure debug → WARN; ~20 `[MemoryGate]` / repository lines removed. `help/embedding-profiles.md`, `memory-consolidation.md`. No dependency move (the npm script `oneoff:rebuild-vector-indices` + its script deleted). **IMPORT-DEAD at HEAD:** `harness/oracle/cases/vector-indices.ts`, `build-vector-indices-fixture.ts`, `build-consolidation-fixture.ts`, ~18 builders using `repos.vectorIndices`; the memory delete / cascade / read / episodic / dedup / regenerate / fold-episode cases calling removed APIs | `db/vector_indices.rs` (whole; Phase 2, `vector_indices_tier2_equivalence`) + `db/mod.rs`; `db/vector_store.rs:124-127` (`load` reads `vector_entries`; P4.D265's tier stamp); `db/memories.rs:1052` / `:1141` (`delete_with_unlink` / `delete_many_with_unlink`, P4.d12) + `services/memory_service.rs`'s `*_with_vectors` (P4.D126 / P4.D270) and its search; `services/memory_gate.rs` (the gate search; P4.D266), `episodic.rs` (`build_memory_embedding_text`), `housekeeping.rs`, `memory_dedup.rs` (P4.43 / P4.D268), `consolidation.rs` (P4.D271), `anchor_gate_probe.rs` (P4.D269), `generators/optimizer.rs`, `embedding_generate_job.rs` (P4.6BL) + `embedding_reindex_job.rs`, `services/cascade_delete.rs:376` + `character_archive/service.rs`, `embedding_dimension_reconcile.rs` (table-guarded), `delete_all.rs`; backup `collect.rs` / `manifest.rs` / `staging.rs` + restore 22j (`restore/orchestrator.rs:1766-1800`) / `archive.rs` / `uuid_remap.rs` (P4.9G); `api/memories.rs:1838-1855` (`batchSize` 1–50); SPA `core-contract.ts` (`batchSize`). D23: `fresh_schema.json` loses two tables + two `createdAt` indexes, `migration_indexes.json` two `characterId` indexes. ⚠ LIVE-DATA HAZARD: §1 (2) — gate creates fail after writing the row, the character cascade half-completes with a 500, recall silently falls to text, embedding jobs fail. **RULED 2026-10-13 (the human): MIRROR v4** — port `retire-memory-vector-entries-v1` as a boot ensure (verify-and-copy each mirror vector onto a row with no embedding, then `DROP TABLE IF EXISTS` both), differentially compared against v4's real `MigrationRunner`; every v5 reader / writer moves onto `memories.embedding` | **ORDERED(P4.D288 (`retire-memory-vector-entries-v1` — MIRROR, ruled; the index store over rows; the ONE retire path), P4.D292 (the embedding jobs), P4.D293 / P4.D294 (no vector files; legacy adoption), P4.D298)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `8af764514` | 2026-10-13 | Memory refactor Phase C: admission in parent, one combine (#93) | **PORT — NOT a pure refactor; a WIRE change, NO migration** (classified 2026-10-13 from the hunks, §5.3; 87 files, +4,011 / −3,588; `4.10.0-dev.163`, parent `af48f6cc2`; no migration file, no DDL). Thresholds UNCHANGED (`ADMISSION_THRESHOLDS` nearDuplicate .90 / reinforce .85 / related .70 / dateGuardDays 7 / footnoteCap 8; `ADMISSION_TOP_K` 5); `extractNovelDetails` moved byte-identical. **The decide moves behaviour:** a close neighbour (≥ .85) is eligible only for the same subject (`aboutCharacterId \|\| holder`), the same kind (default semantic), not a MANUAL candidate on an AUTO row, and not date-guarded — excluded rows become LINK targets and the loop continues (it used to take the best match only); `insert-linked` links every band + excluded neighbour (reason `date-guard` / `mismatch` / `band`); the date guard reads the HYDRATED row (the digest when redirected); digest collapse over all top-5; cold: unsuperseded / dangling-digest → `promote-and-reinforce` (footnotes when cos < .90), via a digest → absorb + `redirectedFrom`, ≥ .90 absorb, else reinforce; a multi-hop dangling intermediate is now promoted; NEW reject reason `empty`; `embeddingProfileId` dropped; `promoteOnMatch` / `promoteMatchedMemory` GONE (AUTO never promoted to MANUAL; MANUAL folding into MANUAL / digest raises importance). `admit.ts`: a per-character promise lock, an idempotency ledger (`jobId:sha1(…):n`, 24 h TTL / 5,000 entries), a vanished target re-decided once then force-inserted; admissions commit OUTSIDE the job batch (host-RPC `admitMemory` + IPC `memory-index-patch` — Node plumbing, but the semantics matter: a failed job no longer rolls admissions back); the consolidation watermark at admission. **One combine** (`fold.ts` `foldInto` + `apply.ts` `applyFold`) replaces the gate's `reinforceMemory` and `planMemoryMerge` / `applyMemoryMerge`: earliest `occurredAt` now for reinforce too; entities (case-insensitive, ≤ 12) + `narrativeTime` fill now for dedup too; links by `addLinks` delta (no removal); delta `max(1, count ?? 1)`; re-embed on ANY content / anchor change INCLUDING at the footnote cap (the code contradicts v4's CHANGELOG — port the code); write order footnotes → anchor patch → links → `incrementReinforcement` (the first landed write is the commit point). **Logs:** every `[MemoryGate]` / `[MemoryMerge]` / `[Gate]` string GONE; child loggers `memory:admission` / `:neighbours` / `:apply` ("Memory admitted", "Admission target vanished; deciding again", "Folded into survivor", `[Admission] Linked memory to neighbours both ways`, `[Admission] Name-presence check collapsed the subject to the holder`); `memory-processor.ts`'s debug through `describeAdmission` (new heads: `REINFORCED…`, `(promoted from cold, count N)`, `Created … (linked to N related, band\|date-guard\|mismatch)`, `SKIPPED near-duplicate … absorbed into id (similarity X[, via id])`, `SKIPPED label — no usable text`). The anchor-gate probe decides per side (`anchoredAction` / `anchorFreeAction`); the CLI `anchor-probe` labels renamed ("would fold", "near-dup absorbed", "Fold only without anchors"). **Wire:** POST `/api/v1/memories`' `action` is now the lowercase kind (`insert` / `insert-linked` / `reinforce` / `absorb` / `promote-and-reinforce`), 201 on insert else 200. `help/characters.md`, `episodic-memory.md`; notes appended to fixed bugs 200, 209, 216, 218, 226, 231, 234; no dependency move. **IMPORT-DEAD at HEAD:** `lib/memory/memory-gate.ts`, `memory-merge.ts` (+ five test files) and `createMemoryWithGate`, `createMemoryWithEmbedding`, `CreateMemoryOptions`, `runMemoryGate`, `reinforceMemory`, `linkRelatedMemories`, the three threshold constants, `GateDecision` / `GateResult` / `MemoryGateOutcome` | `services/memory_gate.rs` (the whole gate, its tier-3 family and every `[MemoryGate]` / `[Gate]` line pin; P4.D266 + the earlier gate ports), `services/memory_merge.rs` + `memory_dedup.rs` (P4.43 / P4.D268), `services/memory_processor.rs`'s outcome debug, `services/fold_episode_pass.rs` (P4.D270), `services/anchor_gate_probe.rs` + `quilltap anchor-probe` (P4.D269 — the probe's row fields and labels), `api/memories.rs`'s POST (the `action` wire; P4.D269 / P4.D276) + the SPA's create toast, the job runner's commit boundary (the single-writer runtime — admissions committed outside the job's batch is a writer-task decision for v5, not a Node workaround to drop) | **ORDERED(P4.D289 (the pure decide / fold), P4.D290 (admission in the parent), P4.D298 (POST `action` kinds), P4.D300)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `ac6348c5f` | 2026-10-13 | Memory refactor Phase D: extraction — one context, four passes (#94) | **PORT — WITH A SCHEMA MOVE (two migrations; a NEW table; two DROP COLUMNs, one of them v5's ensure's)** (classified 2026-10-13 from the hunks, §5.3; 152 files, +10,498 / −9,320 — 90 production files +5,377 / −5,412, NEW `lib/memory/extraction/` 22 files +3,578; `4.10.0-dev.164`, parent `8af764514`; v4 HEAD at this check). **Migrations** (both `'4.10.0'`, registered LAST after the vector retirement): `add-memory-extraction-cursors-v1` (`dependsOn add-memory-tiers-v1, add-episode-extraction-watermark-v1`; PRETTY "Gathering every chronicle's bookmarks into one drawer…"; `shouldRun` while the cursor table is absent or either legacy column present) — `CREATE TABLE IF NOT EXISTS "memory_extraction_cursors" ("id" TEXT PRIMARY KEY, "chatId" TEXT NOT NULL, "pass" TEXT NOT NULL, "messageId" TEXT NOT NULL, "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL, UNIQUE ("chatId","pass"), FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE)`, copies non-empty `chats.otherExtractionWatermarkMessageId` → pass `other-fold` and `episodeExtractionWatermarkMessageId` → `episode-fold` (id `<chatId>:<pass>`, `ON CONFLICT("id") DO NOTHING`, one transaction per column), then `ALTER TABLE "chats" DROP COLUMN` each present column (INFO `memory_extraction_cursors table in place`, `Copied watermark column into cursors` {column, pass, rows}, `Dropped chats.<column>`; ERROR `Extraction-cursor migration failed`); `drop-inter-character-memory-jobs-v1` — `DELETE FROM "background_jobs" WHERE "type" = 'INTER_CHARACTER_MEMORY'` (INFO `Deleted retired INTER_CHARACTER_MEMORY jobs`). **generateDDL:** `ChatMetadataSchema` / `BaseSchema` drop both watermark fields; the NEW `memoryExtractionCursors` repository materialises the table in generateDDL's bare shape (no UNIQUE / FK expected — TWO v4 shapes again; confirm by re-dump); `BackgroundJobTypeEnum` loses `INTER_CHARACTER_MEMORY`. **Behaviour:** `memory-processor.ts`, both fold passes, `fold-window.ts`, both catch-ups, `extraction-route.ts`, `handlers/inter-character-memory.ts` and the extraction half of `cheap-llm-tasks/memory-tasks.ts` (−1,664) DELETED → `lib/memory/extraction/` (`buildExtractionContext` once per unit; four passes `self-turn` / `other-turn` / `other-fold` / `episode-fold`; `runTurnExtraction`, `runCarinaExtraction`, `runFoldPass`, `runFoldCatchup`, `runExtractionDryRun`, `cursor-bundle`); prompt bodies verbatim (only `batchExtractMemories`' is gone). Real changes: the cheap LLM honours the global default cheap profile; a per-pass cap `min(base, ceil(maxTokens / 8000))` incl. fold OTHER; memory `createdAt` = the source message's time on every path; the rate limit a LIVE counter (an in-memory parent ledger, host-RPC `countRecentAdmissions`; now applies to the episode pass; a restart forgets), `backfill: true` exempt (regenerate / queue-memories / ST import / dry run), a rate-limited fold slice deferred (`skippedReason 'rate-limited'`, cursor not advanced); Carina provenance `pass: 'carina'`; regenerate clears the cursors and enqueues anchored turn jobs for autonomous rooms + an immediate episode catch-up; queue-memories tries a seat profile first; the greeting kept as a null-opener turn. `MEMORY_EXTRACTION` payload gains `pass` (`turn` / `other-fold` / `episode-fold`), `cursor`, `ignoreIdle`, `backfill` (the old `foldOtherCatchup` / `foldEpisodeCatchup` / `foldOtherIgnoreIdle` read as deprecated); the dedupe key (chat, opener, anchor, pass, cursor). ~57 log lines added, ~76 removed (`[Memory] Extraction context built`, `[Memory] Pass complete`, `[ExtractionCursors] …`, `[FoldCatchup] …`; every `[FoldOtherPass]` / `[FoldEpisodePass]` / `[CarinaMemoryExtraction]` line gone). **Wire:** the dry run's two new 400 sentences, `decision` / `extractionPass` on candidates, an NDJSON `fold` event, embedding calls; `system/tools` drops the INTER_CHARACTER estimate; backup / export append the cursors at the END of the chat object under the legacy key names (absent, not null, when no cursor), restore / import strip and write them, a `replace` restore clears the table; CLI `memory-diff` prints `fold` lines. `help/` four pages; no `bugs.md` change; no dependency move. **IMPORT-DEAD at HEAD:** `processTurnForMemory`, `extractSelfMemoriesFromTurn`, `extractOtherMemoriesFromTurn`, `batchExtractMemories`, every fold-pass / catch-up / route module, `handleInterCharacterMemory`. Not absorbable alone — built on `c2795733f` and Phases A–C | `db/chats_read.rs:96,322` + `db/chats.rs` (the column read, the INSERT, the watermark UPDATE) and **`db/memory_tiers_repair.rs:105`** (P4.D265's ensure ADDS `chats.otherExtractionWatermarkMessageId` when absent — on a v4-migrated instance it RE-ADDS what v4 dropped: the ping-pong); `services/fold_other_pass.rs` + `fold_episode_pass.rs` + the idle catch-up (P4.D270 / P4.6bj's extraction wiring), `services/memory_processor.rs`, `services/carina_memory_extraction.rs`, `services/memory_extraction_job.rs` (reads only `foldOtherCatchup`), `services/queue_service.rs`, `services/scheduled_maintenance.rs`, `memory_tasks.rs` + `memory_tasks/prompt_text.rs`, `services/job_runner.rs` + `activity_kinds.rs` + `realtime/job_topics.rs` + `api/system_data.rs` (`INTER_CHARACTER_MEMORY`), `api/memories.rs` (the dry run — a standing v5 deferral, `services/turn_transcript.rs:73`), backup `uuid_remap.rs` + `qtap_export/key_order.rs` + `generators/qtap_schema.rs` (P4.D272 / P4.D264), `services/provisioning/mod.rs` + `fresh_schema.json`; fixtures in `enclave/lifecycle.rs`, `enclave/step.rs`, `almanack/phase3_ledgers.rs`. SPA: no hit. ⚠ LIVE-DATA HAZARDS: §1 (3) the re-added column → mass re-extraction + double extraction, v4's `pass:` catch-up jobs misread as turn jobs; (5) cursor loss through v5 bundles. **RULED 2026-10-13 (the human): RETIRE P4.D265's chat-column arm** (the P4.D251-supersedes-P4.D179 precedent) — port `add-memory-extraction-cursors-v1` as a boot ensure (copy both watermark columns into `memory_extraction_cursors`, then DROP both columns; carry both v4 table shapes — generateDDL's bare one and the migration's `UNIQUE` + FK — as the informs ensure did), differentially compared against v4's real migration; v5 reads only the cursors | **ORDERED(P4.D288 (`add-memory-extraction-cursors-v1` — P4.D265's chat-column arm RETIRED, ruled; `drop-inter-character-memory-jobs-v1`), P4.D296 (extraction whole), P4.D293 / P4.D294 (cursor carriage))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `93c5af6ad` | 2026-10-13 | Fix bugs 235–241 from the memory refactor dogfood; file bug 242 | **PORT — NO migration** (classified 2026-10-13 afternoon from the hunks, §5.3; 56 files, +2,253 / −217; `4.10.0-dev.165`, parent `ac6348c5f`; the 51-path dirt the previous check recorded, committed). **236:** `app/api/v1/memories/route.ts` + `[id]/route.ts` trim and require `content` (400) on manual create / edit; an admission `reject` is mapped by reason (`empty` → 400, `embedding-failed` → 502) instead of a 500. **237:** `lib/memory/admission/neighbours.ts` (+147) — a dimension-mismatched index reloads once, then the rows it cannot compare are word-matched and the likeliest few re-embedded and scored, so a re-embed no longer disables duplicate detection (`admit.ts`, `rate-ledger.ts`). **238:** `extraction/passes/other-fold.ts` — an undated draft's `occurredAt` falls back to the observer's last line, not the stretch end. **239:** `lib/database/backends/sqlite/instance-lock.ts` — `LockFileContent.port?` (from `PORT`, default 3000; dropped on takeover when invalid) on the SHARED `quilltap.lock`; v5's `quilltap-host/src/lock.rs` `LockFileContent` (no `deny_unknown_fields`) reads a v4 lock with `port` fine but a v5 rewrite drops the key, so v4's CLI tools cannot find a v5 server by instance name; `packages/quilltap/lib/lock-helpers.js` (NEW) + `anchor-probe` / `recall-replay` / `memory-diff` accept `--instance` / `--data-dir` / `--passphrase` (CLI Tier R; v5's `quilltap anchor-probe` from P4.D269). **240:** `lib/import/quilltap-import/import-entities.ts` — a chats-only bundle's memories resolve characters that already exist locally (`.qtap` carriers, P4.D272). **241:** `orchestrator.service.ts` + `streaming.service.ts` — a `SubmissionLedger` (`userMessagePersisted`, `pendingToolResultsPersisted`) rides `handleStreamError`, so the error frame says whether the typed text has a row; `app/salon/[id]/hooks/useSSEStreaming.ts` (453 lines changed) restores composer / draft / attachments / tool results with a toast, buffers split SSE frames, and treats an early close as an error (v5: the Salon spine's error frame, `api/salon.rs`, and the SPA `chat-stream.reducer.ts` / composer). **235:** `lib/utils/process-singleton.ts` — module state on `globalThis` because Next.js evaluates a module once per bundle; a Node-bundling defect — **NO-PORT candidate** for that half (v5 has one process-wide cache by construction; measure, don't assume). **Bug 242 filed OPEN** (`5a99e4b8f`'s depth-0 `turnStart` on every first turn routes single-character turns through the chain's intermediate path) — no fix hunk; v5's reducer sets `inChain` on any `turnStart` (`chat-stream.reducer.ts:296`), so whether v5 shares it is a dogfood measurement. `help/chats.md`, `embedding-profiles.md`, `system-import-export.md`; `DDL.md` (the lock line); `API.md`. CONVERGENCE: none. | **ORDERED(P4.D290 (237), P4.D294 (240), P4.D296 (238), P4.D298 (236), P4.D299 (239), P4.D301 (241), P4.D303 (235 NO-PORT; help))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `584d28164` | 2026-10-13 | Memory refactor Phase E: recall as a pipeline; delivery (#95) | **PORT — NO migration; model-visible TEXT moves** (classified 2026-10-13 afternoon from the hunks, §5.3; 132 files, +23,460 / −4,300 — 18,012 lines are `__tests__/fixtures/recall-probes/probe-set.json`; `4.10.0-dev.166`, parent `93c5af6ad`; read from `origin/main`, NOT yet pulled). NEW `lib/memory/recall/` stages — `query.ts` (`buildRecallQuery`: distil, choose the text, embed ONCE; retrospective probes beside it), `gather.ts`, `score.ts`, `select.ts`, `hydrate.ts`, `assemble.ts` (617), `render.ts` (322), `deliver.ts` (198), `search.ts`, `types.ts`; moved in: `recall-tags` → `tags`, `recall-tuning` → `tuning`, `recall-history` → `history`, `memory-subject` → `subject`, `frozen-archive-cache` → `archive`, `memory-recap` → `recap`, `conversation-summary-search` → `conversations`, `recall-replay` → `replay`; `lib/memory/scoring/rank.ts` NEW. DELETED: `lib/memory/memory-service.ts` (−1,016; search → `recall/search.ts`, `generateMissingEmbeddings` / `rebuildVectorIndex` → `index-store/backfill.ts`, `markMemoriesAccessed` → `deliver`'s `stampAccessed`) and `lib/chat/context/memory-injector.ts` (−822); `context-manager.ts` −885 (the scene state to NEW `lib/chat/context/scene-state.ts`). **Behaviour:** one query embedding per turn shared by the head, the inter-character half, the recap's relevant-conversations list, the per-turn conversation list and the retrospective mini-recap; cross-pool dedup (a row in at most one of archive / head / inter-character; the recap narrative excludes what the archive rendered); **the recall lines lose every debug label** (`[m_xxxx]` ids, importance / relevance / weight numbers, `recall:` labels, targeting keywords) — the request body moves on every turn that recalls; delivery (after the body is built and Phase-2 compression ran) stamps `lastAccessedAt` and the anti-repetition ring only for lines still present; `buildContext` takes the proactive pass's query + pool as `preRecall`; `message-finalizer.service.ts` writes the recall record to the assistant message's `debugMemoryLogs` (extraction appends after it, no longer overwrites); `?action=recall-replay` + `quilltap recall-replay` gain `fallbackPath`, `trace`, `pools`, `ring`, `ringSource`; the Memories API `search` stamps nothing. Callers re-pointed: `carina.service.ts`, `voice-rewrite-core.ts`, `pre-compute.service.ts`, `context-builder.service.ts`, `search-scriptorium-handler.ts`, `relevant-conversations-refresh.ts`, `first-message-context.ts`, `memory-extraction.ts` handler. v5: `memory_injector.rs`, `recall_tuning.rs`, the recall / recap / frozen-archive services (P4.D267 / P4.D268), `api/recall_replay.rs` (P4.D267), the context builder + `debugMemoryLogs` (`chats_messages.rs`), the anchor / proactive paths, `search_scriptorium`, Carina, the voice rewrite, the CLI `recall-replay`. ⚠ IMPORT-DEAD at HEAD grows: every oracle importing `memory-service`, `memory-injector`, `recall-tags` / `recall-tuning` / `memory-recap` / `recall-replay` / `frozen-archive-cache` / `memory-subject` / `conversation-summary-search` by their old paths. `help/memory-recall-relevance.md` (two new sections), `episodic-memory.md`; `SYSTEM_FLOWCHARTS.md`, `features/memory-system-refactor.md`. CONVERGENCE: none. | **ORDERED(P4.D291 (Phase E whole), P4.D303 (help))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `4ae0c4ca6` | 2026-10-13 | Built-in TF-IDF tuning: refit policy, text shape, admission thresholds (#96) | **PORT — WITH A SCHEMA MOVE (the NINTH D23 re-dump; one migration)** (classified 2026-10-13 late evening from the hunks, §5.3; 69 files, +1,655 / −122 — a squash of three; `4.10.0-dev.169`, parent `584d28164`). ⚠ The squash's first paragraph says "No migrations, no schema change" — FALSE for the shipped tree: the PR-review commit inside it adds migration `add-tfidf-text-shape-version-v1` (`tfidf_vocabularies.textShapeVersion INTEGER DEFAULT 0` via `addColumnIfMissing`, APPENDED; no backfill — 0 means "fitted with anchor lines, refit once") AND `textShapeVersion: z.number().int().nonnegative().default(0)` on `TfidfVocabularySchema` between `fittedAt` and `createdAt` — the repository registers that schema, so generateDDL's fresh table carries the column in SCHEMA order: two shapes (the `chat_informs.permanent` precedent, P4.D249). **Behaviour:** `lexicalEmbeddingTextFor` (`index-store/embedding-text.ts`) — BUILTIN embeds leave out the anchor line, passed by every memory writer as `lexicalText` to `generateEmbeddingForUser` (neural providers ignore it); `isLexicalEmbeddingProvider` (`lib/embedding/cosine-scale.ts`) the one predicate for the recall floor, the R1 gate, admission and consolidation; `ADMISSION_THRESHOLDS_TFIDF` (0.80 / 0.40 / 0.20) carried on the candidate's `embeddingProvider`; `memoryConsolidation.tfidfClusterThreshold` (0.30, a new settings key + card field); `builtin-refit-policy.ts` (refit only when due: no vocabulary / `textShapeVersion` < `LEXICAL_TEXT_SHAPE_VERSION` / drift of max(25, 10%) new memories; `enqueueRefitIfDue` serialised; import forces; boot + the maintenance pass check), `embedding-refit.ts` gathers by `findDistinctCharacterIds`; new repo reads `backgroundJobs.findActiveByType`, `embeddingProfiles.findInstanceDefault`, `memories.countCorpusSince`; the anchor probe's `lexicalProfile` + note (CLI prints it). **Bug 243** (`child-entry.ts` + NEW `plugin-readiness.ts`: the forked job child ran jobs before its plugin registry loaded) — NO-PORT candidate: v5's runner is in-process over static provider manifests (measure, don't assume — the help-doc embed on a BUILTIN instance at first boot is the probe). v5: `db/tfidf_vocabulary.rs` + `fresh_schema.json` (D23 — re-dump from v4's live generateDDL, never by hand) + a boot ensure for the migration shape, the TF-IDF builtin + embedding service (the provider layer), admission (still the pre-Phase-C gate in v5 — this rides the Phase C port), `services/consolidation.rs` (P4.D271), the anchor probe + `quilltap anchor-probe` (P4.D269), the embedding jobs (P4.6BL), `api/embedding_profiles.rs`. `help/embedding-profiles.md`, `memory-consolidation.md` (deleted again by `b045dd986`). CONVERGENCE: none (243 `Original to v4`). | **ORDERED(P4.D288 (`add-tfidf-text-shape-version-v1` + the seam), P4.D289 (thresholds, `isLexicalEmbeddingProvider`, the lexical text), P4.D290 (the probe note), P4.D292 (the refit policy), P4.D297 (`tfidfClusterThreshold`), P4.D303 (bug 243 NO-PORT))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `104e986c7` | 2026-10-13 | Memory refactor Phase F: one maintenance job, ordered phases (#97) | **PORT — job-row migration, NO DDL** (classified 2026-10-13 late evening from the hunks, §5.3; 88 files, +4,235 / −3,696 — a squash of three; `4.10.0-dev.172`, parent `4ae0c4ca6`). `BackgroundJobTypeEnum`: `MEMORY_HOUSEKEEPING` + `MEMORY_CONSOLIDATION` → ONE `MEMORY_MAINTENANCE`; migration `drop-housekeeping-consolidation-jobs-v1` DELETEs both retired types' rows (any status — a row would fail the job schema's parse). NEW `lib/memory/maintenance/` (`run.ts` 385, `fold-near-duplicates.ts` 304 — LLM-free, survivor-first, host-RPC `foldNearDuplicates` under admission's lock, stops at 40% of the job budget; `consolidate.ts`, `demote.ts` — exactly demotable − cap, lowest protection first; `ledger.ts` 282 — the parent-side `shouldEnqueue`, `setJobResult` summaries; `scheduler.ts` — one daily sweep, the startup tick cancelled on stop; `triggers.ts`, `corpus.ts`, `types.ts`); payloads carry `exemptPhases`; a PROCESSING job never absorbs a request. DELETED: `lib/memory/housekeeping.ts` (−428), `consolidation-triggers.ts` (−198), `housekeeping-outcome-cache.ts`, `admission/after-insert.ts`, `background-jobs/scheduled-housekeeping.ts` (−200), both handlers, **the `memory-dedup` TOOL** (`lib/tools/memory-dedup.ts`, −464), `system/tools` route's dedup actions (−74); `consolidation.ts` −265 (keeps the planner + model route). Settings: `memoryMaintenance.phases {foldNearDuplicates, demote, retire}` (`.prefault({})` — a stored value without it reads every phase on); the three retention knobs GONE. API: `?action=maintain`, `?action=maintenance-status`; a live `housekeep` enqueues. Activity kinds, `job-topics.ts`, `queue-service.ts` (−371/+), the CLI `memories status` ledger. v5: `services/housekeeping.rs` + `memory_merge.rs` dedup (P4.D266), `services/consolidation.rs` + its triggers (P4.D271), the job runner / queue / scheduled maintenance / activity kinds / job topics, the tool table (the dedup tool + its census), `api/memories.rs` + the `system/tools` dispatch, `db/memories.rs`, the SPA's dedup / housekeeping / consolidation cards (P4.D276). `help/memory-housekeeping.md` + `memory-consolidation.md` + `system-tools.md` + `cli-memories.md` (the first two deleted by `b045dd986`). CONVERGENCE: none. | **ORDERED(P4.D288 (`drop-housekeeping-consolidation-jobs-v1`; the job types), P4.D297 (Phase F whole), P4.D298, P4.D303)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `b045dd986` | 2026-10-13 | Memory refactor Phase G: unified maintenance card and CLI (#98) | **PORT — NO migration; bug 244 trips a RULED table; bug 242 is shared** (classified 2026-10-13 late evening from the hunks, §5.3; 87 files, +4,135 / −2,441 — a squash of six; parent `104e986c7`). `add-memory-tiers-v1` gains a doc-path COMMENT only (no shape change — v5's ensure unaffected). **API:** `housekeep` / `housekeep-sweep` / `consolidate` / `housekeeping-config` RETIRED (`maintain` covers them; naming `consolidate` brings the mirror — `withImpliedPhases`); NEW `POST ?action=repair` (`lib/memory/repair.ts` `findDanglingLinks` over `relatedMemoryIds` / `consolidatedFrom` / `supersededById` — no fallback, a failed scan is a 500; `retire.ts` `scrubDanglingMemoryLinks` per row, a digest whose members are all gone retired with reason `graph-repair`; `enqueueMissingEmbeddings` / `enqueueEmbeddingGenerateMany` one batch insert; `findIdsWithoutEmbedding`); `memoryExtractionLimits.maxPerHour` `.max(10_000)`. **Bug 244** — NEW `lib/schemas/patch-schema.ts` `patchSchemaOf` (strips `.default()` / `.prefault()`, then `.optional()`), used by every `*-config` write, `maintain`'s run overrides and `TextReplacementRulePatchSchema`: **v5's `PARTIAL_DEFAULTS_CLOBBER` (P4.D271 R-A, `memory_consolidation_routes_equivalence.rs:355-396`, `api/memory_consolidation.rs:8`) pinned v4's clobber on BOTH sides — its three rows TRIP BY DESIGN at a pin past this (the assertion text already says "a v4 fix? re-measure"); retire the table by measurement (§5.4), and audit v5's text-replacement PATCH for the same shape.** **Bug 242** — `useSSEStreaming.ts`: `inChain` only for a `turnStart` with `chainDepth > 0`; v5's `apps/web/src/app/core/chat-stream.reducer.ts:296-299` sets `inChain: true` on ANY `frame.turnStart` — the PRE-fix shape, so v5 SHARES it if its server emits the depth-0 event (`services/orchestrator.rs` / `chat_events.rs` — measure). **SPA:** ONE Maintenance card (`memory-maintenance-card.tsx` 383, `-ledger.tsx`, `memory-consolidation-particulars.tsx`) + `maintenance-dialog.tsx` / `-report.tsx` replace the housekeeping / consolidation / dedup cards and the cleanup dialog; `memory-extraction-limits-section.tsx` on the Extraction Grain card; `useMemorySettingsDocument`; old `section=` deep links open the new card. **CLI:** `memories maintain` (replaces `consolidate`), `validate --repair`, `db memories --tier hot\|cold`, `--instance` port resolution, completions. **help/:** `memory-maintenance.md` NEW (`section=memory-maintenance`), `memory-housekeeping.md` + `memory-consolidation.md` DELETED, ten edited, `help-guide/categories.ts`. v5: `api/memories.rs`, `api/memory_consolidation.rs`, the SPA `memory/` cards (P4.D276) + the stream reducer, the CLI (v5 has `db memories`? — `db_cmd.rs`; no `memories` verb family), the vendored `help/` (p4.9i2). CONVERGENCE: none (242 / 244 `Original to v4`). | **ORDERED(P4.D288 (the retired verbs deleted, the new ones registered), P4.D297 (repair, bug 244), P4.D298, P4.D300 (the Maintenance card + dialog), P4.D301 (bug 242), P4.D303)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `48790fb73` | 2026-10-13 | Close the memory refactor: record the Friday backfill; fix three report labels | **PORT (labels, riding Phase G's new surfaces) + ⚠ LIVE-DATA FACT** (classified 2026-10-13 late evening from the hunks; 32 files, +105 / −65; parent `b045dd986`). Code: `maintenance-report.tsx` ("Clusters found" → "Clusters worth folding", `clustersQualified`; "Protected" → "exempt"), `memory-maintenance-card.tsx`, `memory-maintenance-ledger.tsx` (a no-excess demotion reads "Within the cap", muted), the CLI `memories-maintain-command.js` — all on surfaces `b045dd986` created, so they port WITH it. Docs: the refactor spec → `features/complete/`, link fixes across `bugs/fixed/*`, `help/cli-memories.md` + `memory-maintenance.md` (one word each), ROADMAP. **The spec's "Friday backfill (E2) — done 2026-10-13" records v4 migrating LIVE FRIDAY at 22:24 UTC** — ten refactor migrations, 44,240 memories, the vector mirror dropped, 3,166 importances recomputed, 1,862 `occurredAt` re-zoned, then maintenance: ~120 digests, ~250 members superseded, 913 demoted (cap override lowered 10,000 → 2,500). The §1 LIVE-DATA HAZARDS block moved ARMED → FIRED on this. CONVERGENCE: none. | **ORDERED(P4.D297 (server labels), P4.D300 (SPA labels), P4.D303 (docs))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `9ec4fa2e9` | 2026-10-13 | Memory refactor V4test dogfood; file bugs 245 and 246 | **NO-PORT?** (classified 2026-10-13 late evening from the hunks; 10 files, +187 / −7; parent `48790fb73`). Docs (bugs 245 / 246 filed OPEN, the phase B–G acceptance walks), version stamps, and ONE code hunk: `packages/quilltap/lib/memories-commands.js` — `memories status` prints a no-excess demotion as "within the cap". v5's CLI has no `memories` verb family (Tier R ported `db` / `docs` / `instances` / `sync` / `anchor-probe` / `recall-replay`), so the line has no v5 home today — it rides whichever order ports `memories status`. Ratify at the round that does. | **ORDERED(P4.D303 — NO-PORT ratification)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `64743ee13` | 2026-10-13 | Fix bugs 245 and 246: reopened turns re-extracted; consolidation token budget | **PORT — NO migration (a new free-text cursor `pass`); a bundle KEY added** (classified 2026-10-13 late evening from the hunks, §5.3; 38 files, +637 / −95; `4.10.0-dev.175`, parent `9ec4fa2e9`). **Bug 245:** `ExtractionCursorPassEnum` gains `'turn'` (`FoldCursorPass` excludes it); `extraction/turn.ts` + `window.ts` + `passes/other-turn.ts` — a live turn job reads past the forward-only `turn` cursor, earlier lines of the turn (opener included) become context under "EARLIER IN THIS TURN" (`turn-transcript.ts` +131, `priorSlices`), SELF only for speakers with new lines, OTHER for everyone (earlier speakers on new lines only), a bound outside the turn ignored; `memory-regenerate-chat.ts` clears it and never reads it; regenerate / `queue-memories` bound each autonomous speaker by the previous anchor (`listExtractionTurns`); `cursor-bundle.ts` + `uuid-remap.ts` + `message-id-remap.ts` carry / remap `turnExtractionWatermarkMessageId`; `qtap-export.schema.json` gains the key (re-vendor). **Bug 246:** `consolidation-tasks.ts` `profileMaxTokensFor` — the routed profile's `resolveMaxTokens` with 4,000 as a floor; an empty answer logged as empty. v5: the per-turn extraction + turn transcript (`services/carina_memory_extraction.rs`, `memory_processor.rs`, the P4.D270 passes — v5 is still pre-Phase-D, so this rides the cursor-table port ruled 2026-10-13), the backup / restore / `.qtap` carriers (P4.D264 / P4.D272), `services/consolidation.rs` (P4.D271), `docs/v4/` + the vendored export schema. CONVERGENCE: none (`Original to v4`). | **ORDERED(P4.D288 (the `turn` cursor pass), P4.D296 (bug 245), P4.D297 (bug 246), P4.D293 / P4.D294 (`turnExtractionWatermarkMessageId`))** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `83334b9a5` | 2026-10-13 | Release checklist 4: API standard audit; use successResponse in new handlers | **NO-PORT?** (classified 2026-10-14 from the hunks, §5.3; 15 files, +41 / −35; `4.10.0-dev.176`, parent `64743ee13`). Every `app/` / `lib/` hunk line is a `NextResponse.json(x)` / `NextResponse.json(x, { status })` → `successResponse(x[, status])` swap or the import widening that brings `successResponse` in — measured by `git show 83334b9a5 -- app lib \| grep -E '^[-+]' \| grep -v 'NextResponse.json\|successResponse'` returning ONLY the five import lines; `successResponse` (`lib/api/responses.ts:63-68`) IS `NextResponse.json(data, { status })` with `status = 200`, so every response is byte-identical (status, body, headers). Sites: `chats/[id]/actions/{impersonation-voice-preview,inform,rebuild-summary,story-background}.ts`, `chats/[id]/handlers/get.ts`, `connection-profiles/[id]/route.ts`, `memories/route.ts` (manual create's 201/200, `maintain` both arms, `maintenance-status`, `repair`, the maintenance / consolidation config reads + writes), `plugins/[name]/route.ts`, `lib/mount-index/scenario-item-route-factory.ts`. Plus `memories-manual-create.test.ts`'s mock now honouring `status` (tests-only), CHANGELOG, README badge, version stamps (no lockfile hunk beyond the root `version` lines). No dependency, schema, help or `docs/` (beyond CHANGELOG) move. | v5 effect: NONE in behaviour; pin line numbers in the touched files shift by zero lines (one-for-one swaps). Rides the memory + scenarios round's pin. | **ORDERED(P4.D303 — NO-PORT ratification)** — the `83334b9a5` memory-refactor + scenarios round, 2026-10-14 |
| `7417e9a1b` | 2026-10-13 | Release checklist 5: dead code sweep | **NO-PORT?** (classified 2026-10-14 from the hunks; 21 files, +80 / −338; `4.10.0-dev.177`, parent `83334b9a5`; the dirt the earlier probe recorded, committed). Every `lib/` hunk deletes an export with NO production caller: the `lib/memory/index.ts` (189) and `lib/memory/recall/index.ts` (47) BARRELS, `createMountWardrobeItem` / `deleteMountWardrobeItem` (`wardrobe-writes.ts`), `withMemoryTimeZone` + its `[Memory] Extraction clock zone resolved` DEBUG (`extraction/prompts/shared.ts` — no caller, as the extraction survey measured; the prompt bodies untouched), `MEMORY_CALENDAR_DAY_MS` (`time/calendar.ts`), `emptyBase` + two sync type aliases moved inline (`mount-index/sync`), `ProgressionIssue`, `EXTRACTION_CURSOR_PASSES`, `isModerationRefusal`; tests, `knip.json`, `DEAD-CODE-REPORT.md`, CHANGELOG, the refactor spec (one line), stamps. ⚠ **Oracle effect at a later pin:** any `harness/oracle` case importing the `@/lib/memory` or `@/lib/memory/recall` BARREL, or one of the deleted names, goes IMPORT-DEAD — measure at the next pin. | None in behaviour; the `83334b9a5` round's pin predates it. | **UNPROCESSED** — past the `83334b9a5` round's pin |
| `01f508d4e` | 2026-10-13 | Release checklist 6: prune leftover debug logging | **PORT — log lines only** (classified 2026-10-14 from the hunks; 9 files, +12 / −17; `4.10.0-dev.178`, parent `7417e9a1b`). DEBUG lines REMOVED: `[Transcript] Projecting chat transcript` (`lib/chat/transcript-projection.ts`), `[Sync] Reading disk file` (`lib/mount-index/sync/apply-disk.ts`), the SPA `console.debug('[useWardrobeItemActions] Duplicating wardrobe item', …)`. RENAMED: `markMailAlertsDelivered` → `Mail alerts marked delivered`, `discardLetter` → `Letter discarded` (`lib/post-office/mailbox.ts`, fields unchanged). | The `83334b9a5` round ports `markMailAlertsDelivered` with its PIN text (P4.D291 — the mail deferral); v5's transcript projection, the sync disk read and the Post Office letter discard carry the pre-commit lines where ported. A small catch-up (or rider) re-pins them. | **UNPROCESSED** — past the `83334b9a5` round's pin |
| `a42c282b8` | 2026-10-13 | Release checklist 7: qt-* theme classes | **PORT — one SPA class attribute** (classified 2026-10-15 from the hunks, §5.3; 6 files, +11 / −6; `4.10.0-dev.179`, parent `01f508d4e`). The message says 158 components were REVIEWED; exactly ONE ships a hunk: `components/memory/memory-card.tsx` — the archived card's "Superseded by a digest" `<button>` `className="text-primary hover:underline"` → `className="qt-link"` ("so themes control its color"). Everything else is `docs/CHANGELOG.md` (+5), the README badge, the three version stamps (`package.json`, `packages/quilltap/package.json`, the lockfile's two root `version` lines). No `lib/`, `app/api/`, schema, migration, dependency, `help/` or bug-file hunk. | v5 `apps/web/src/app/memory/memory-card.ts:70` (the P4.D276 memory-SPA card; last touched in the `01a83539d` round) carries the PRE-commit `class="text-primary hover:underline"` on that exact button, and `memory-card.spec.ts:253` asserts `classList.contains('text-primary')` — a one-token swap + one spec line. `.qt-link` already exists in the SPA (`styles/qt-components/_typography.css:157`, `_interactive.css:221/:227`; 19 other `class="qt-link…"` sites). The other `text-primary hover:underline` buttons in `memory-card.ts` (`:77` expand, `:114`, `:125` edit) and `memory-list.ts:143` are UNCHANGED in v4 — do not over-port. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `a8b2d2861` | 2026-10-13 | Release checklist 8: plugin self-containment; exclude cache hits in OpenAICompatibleProvider | **PORT — the `openai-compatible` provider's usage normalisation (BOTH send paths); a dependency move** (classified 2026-10-15 from the hunks, §5.3; 31 files, +257 / −95; `4.10.0-dev.180`, parent `a42c282b8`). The "audit" is prose — no hunk enforces plugin self-containment. The ONE source hunk: `packages/plugin-utils/src/providers/openai-compatible.ts` (+58 / −16) — NEW `protected normalizeUsage(usage: OpenAI.Completions.CompletionUsage \| null \| undefined): Pick<LLMResponse,'usage'\|'cacheUsage'>`: `cachedTokens = usage?.prompt_tokens_details?.cached_tokens ?? 0`; `promptTokens: Math.max(0, prompt_tokens − cachedTokens)`, `completionTokens` unchanged, `totalTokens: Math.max(0, total_tokens − cachedTokens)`; `cacheUsage: { cacheReadInputTokens: cachedTokens, cachedTokens }` ONLY when `cachedTokens > 0` (else the key is ABSENT); a NEW DEBUG line when `> 0`: `` `${providerName} usage: excluding prompt-cache hits from prompt/total` `` with `{ context: `${providerName}Provider.normalizeUsage`, promptTokens, cachedTokens }`. `sendMessage` spreads `...this.normalizeUsage(response.usage)` in place of the literal `usage` object (so `usage` stays ALWAYS present, all-zero on a missing block). `streamMessage`: `accumulatedUsage` is now the whole `chunk.usage` object (typed `CompletionUsage \| null`, no longer a three-key copy); the terminal chunk spreads `...normalizeUsage(accumulatedUsage)` + **`rawProviderUsage: accumulatedUsage`** ONLY when a usage frame arrived (previously `usage: undefined`, never `rawProviderUsage`). Tests: `__tests__/unit/plugins/local-provider-params.test.ts` +59 (four cases: 1000/50/1050 with 800 cached → 200/50/250 + `cacheUsage {800,800}`, stream twin + `rawProviderUsage` equals the raw block, no-cache → no `cacheUsage` key, cached 20 > prompt 10 → all 0). **Bundles:** `plugins/dist/qtap-plugin-{openai-compatible,deepseek,nanogpt}/index.js` each +49 / −15 (the inlined base class, verbatim); manifests openai-compatible 1.0.48 → **1.0.49**, DeepSeek 1.0.27 → 1.0.28, NanoGPT 1.2.9 → 1.2.10 — **behaviour changes for `openai-compatible` ALONE**: its `OpenAICompatibleEndpointProvider` subclass overrides neither `sendMessage` nor `streamMessage`, while DeepSeek and NanoGPT override BOTH and keep their own `extractCacheUsage` (`prompt_cache_hit_tokens`; `cache_read_input_tokens ?? prompt_tokens_details.cached_tokens`). **Dependency:** `@quilltap/plugin-utils` `^2.6.2` → `^2.7.1` in the root `package.json` and ALL 15 `plugins/dist/*/package.json`; the lockfile resolves `plugin-utils-2.7.1.tgz` from the registry (2.7.0 was a non-appearing publish; 2.7.1 is a version-only republish per `packages/plugin-utils/CHANGELOG.md`). No schema, migration, `help/`, `app/`, `lib/` or bug-file hunk. | v5's `crates/quilltap-core/src/model/decoders/chat_completions_sse.rs` `Flavor::OpenAiCompatible` arm of `build_usage` is `=> (None, None)` (no cache source) and its doc says "no `usage` inside … no cache usage"; the absent-usage-frame rule ("the plain base leave[s] `usage` undefined") and the no-`raw_provider_usage` rule are both now v4-stale for this flavor (the W4.7b decoders, P4.13/P4.14 `StreamMessage` rewrite, P4.122/P4.128 frames). `crates/quilltap-core/src/model/response_parse.rs` `ChatFlavor::OpenAiCompatible` ("the base — content / finishReason / usage / raw only", W4.7d) needs the same subtraction + conditional `cache_usage`; the shared `sub_floor` / `prompt_tokens_details.cached_tokens` readers already exist for ZAi / OpenRouterVision. `crates/quilltap-core/src/model/plugin_catch_log.rs` pins the OAC ERROR lines (`:173-178`) — the NEW DEBUG `… usage: excluding prompt-cache hits from prompt/total` / `context=OpenAICompatibleProvider.normalizeUsage` is a candidate capture pin. **Oracle:** `harness/oracle/fixtures/streams/chat_completions_sse/` has NO openai-compatible `cached_tokens` case (only `zai-cached`, `nanogpt-cache-*`, `openrouter-tools`) and `response-bodies.recorded.ndjson` none either — a port needs a NEW `openai-compatible-cached.wire` + response-body row; **the recorders (`record-stream-fixtures.mjs`, `record-response-bodies.mjs`) resolve `@quilltap/plugin-utils` from the PLUGIN dir's own `node_modules`**, so at the next pin that dir must `npm ci` to 2.7.1 or the recordings silently replay 2.6.3 behaviour (§5.2; memory note "pin node_modules drift"). `provider_sdk_version_guard.rs` reads plugin-utils via the SDK stamp — check the 2.7.1 bump does not trip it. The autonomous-room token budget (the enclave `step()` spend, P4.D79-era) is the consumer that motivated v4 — v5's budget arithmetic reads the normalised `StreamUsage`, so the fix lands there for free once the decoder moves. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `49f8906c3` | 2026-10-14 | Release checklist 10: backup/restore completeness; remap seat ids in new-account restore | **PORT — the pure `remapBackupData` gains four fields** (classified 2026-10-15 from the hunks, §5.3; 8 files, +201 / −6; `4.10.0-dev.181`, parent `a8b2d2861`). The "audited every table and column … all are backed up and restored" claim ships NO backup/restore code hunk — it is `__tests__/unit/lib/backup/restore-field-fidelity.test.ts` (NEW, 107 lines, mocked repos: the 4.10 chat / message / `memories.supersededAt` / `chat_settings.impersonationVoiceMode` + `wardrobeImageSettings` / `tfidf_vocabularies.textShapeVersion` columns), tests-only; the CHANGELOG records the deliberate NON-carry of the FTS index, `chats.transcriptVersion` and the Concierge refusal ledger (no hunk — a statement of existing behaviour). The ONE `lib/` hunk, `lib/backup/restore/uuid-remap.ts` (+41 / −1): NEW `const PARTICIPANT_ID_JSON_FIELDS = ['turnQueue','spokenThisCycleParticipantIds','cycleOrderParticipantIds'] as const` + `remapJsonIdList(value, remapper)` — a non-string returns as-is; `JSON.parse`; anything but an all-string array (or a parse throw) returns the ORIGINAL value untouched ("the turn manager already reads a malformed value as empty"); else `JSON.stringify(remapper.remapArray(parsed))`. Inside `remapBackupData`'s chat loop, immediately AFTER the `scenarioSource` remap (`545e299bb`'s line, `:349-351`) and BEFORE `remappedChat.participants`: `for (const field of PARTICIPANT_ID_JSON_FIELDS) if (field in chat) remappedChat[field] = remapJsonIdList(chat[field], remapper)` — an ABSENT key stays absent (the test asserts `not.toHaveProperty('turnQueue')`). The messages map's `remapArrayFields(…, ['attachments'])` → `['attachments', 'targetParticipantIds']` (a whisper's audience). Note the ORDER: the JSON-list pass runs before participants are remapped, which is fine because `UuidRemapper.remap` mints-or-reuses per id. `uuid-remapper.test.ts` +43 (two cases). No schema, migration, route, `help/` or bug-file hunk. | v5 `crates/quilltap-core/src/services/backup/uuid_remap.rs` (P4.9G6, the pure remapper; grown by P4.D205 / P4.D264) — the messages arm at `:850-864` is `chain(r, m, &["id","swipeGroupId","participantId"], &["attachments"])` and NO `turnQueue` / `spokenThisCycle…` / `cycleOrder…` handling exists; the proof is `backup_uuid_remap_equivalence` over the committed `harness/oracle/fixtures/uuid-remap-corpus.json` (19 lines) — the corpus holds `"turnQueue": "[]"` ×2 and `"cycleOrderParticipantIds": "[]"` ×2 and ZERO `targetParticipantIds`, so at the next pin the oracle output is BYTE-IDENTICAL for the current corpus (an empty list remaps to `"[]"`) — **the corpus must GROW** (a populated turn queue / cycle order / whisper audience, a malformed `'not json'`, an absent key) or the differential passes vacuously (memory note "Tier R vacuous-equal rows"). **Stacks on the current round:** P4.D293 ports `remapScenarioSource(chat.scenarioSource, …)` into the same loop (order §… `:539`), and this hunk sits directly under it — a rider on P4.D293's landed site, not a conflict. `preserve_order` + `shift_remove` rules of the module apply (a present key keeps its position; `JSON.stringify` of the remapped array is byte-level — v4 emits `["a","b"]` with no spaces, so the Rust side must serialise compactly). | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `75d094e63` | 2026-10-14 | Release checklist 12: CLI docs, completions and tooling; fix migrations status ids | **PORT (Tier R captured bytes: the main help line + zsh/fish templates; the vendored CLI README) — the `migrations status` fix is NO-PORT until v5 has a migration runner** (classified 2026-10-15 from the hunks, §5.3; 11 files, +101 / −16; `4.10.0-dev.182`, parent `49f8906c3`). (1) `packages/quilltap/lib/migrations-commands.js` (+22 / −4): NEW exported `extractMigrationId(fileContent)` — `const MIGRATION_ID(: T)? = '…'` + a `^\s*id:\s*MIGRATION_ID\b` reference wins, else the old `^\s*id:\s*['"]…['"]` literal, else `null`; `readSourceMigrations` uses it for `id` and the `description` fallback. Measured at the commit: **62 of 228** `migrations/scripts/*.ts` use the constant form — before this, those 62 vanished from `source` and every instance that had run them reported them `retired` (`status`'s `retired` / `retiredList`, `pending`'s complement). NEW `lib/__tests__/migrations-source-ids.test.js` (54 lines; also asserts > 100 scripts and every script yields an id). (2) `bin/quilltap.js` ONE line: `anchor-probe <characterId>    Probe whether episodic anchor lines suppress Memory Gate reinforcement` → `… suppress reinforcement at admission`; `lib/completion/zsh.template` `'anchor-probe:Probe whether episodic anchor lines suppress reinforcement at admission'`; `fish.template` `-d 'Probe anchor-line effect on admission'`; the bash template never carried the phrase (0 hits, no hunk). After the commit `git grep -i "memory gate" -- packages/quilltap` is EMPTY — `anchor-probe-command.js`'s own `--help` had already lost it (`93c5af6ad`-era, P4.D299's capture). (3) `packages/quilltap/README.md` +14 / −4: `docs ls -R/--recursive`, `--sort name\|time\|size\|links`, `-r/--reverse`, `--links`; `docs tree --long` ("accepted but reserved — it does not yet add columns"); `memories --limit N` (default 50), `--full-titles`, `grep -l/--paths-only`; `themes registry remove/refresh/keygen/sign` + `--key/--name/--output`; `instances list --names-only` / `docs list --names-only`. Docs only — no flag is ADDED to any command (the README catches up to existing parsers). CHANGELOG, badge, stamps. No schema, migration, `lib/`, `app/`, `help/` or bug-file hunk. | v5 `crates/quilltap-cli/src/help/main_help.txt:19`, `help/completion/zsh.template:46`, `help/completion/fish.template:64` carry the "Memory Gate" bytes — these are `include_str!` captures of v4's output at the pin (P4.D269 unit C re-captured `main_help.txt` at `01a83539d`; P4.D299 re-captures `src/help/*.txt` + `src/help/completion/*.template` at `83334b9a5`, which PREDATES this commit, so the round lands still-stale bytes by design) and `cli_differential.rs` / `completion_behavior` pin them → a three-line re-capture + Tier R regen at the next pin. (`help/anchor_probe_help.txt:7` ALSO says "Memory Gate" — that is anchor-probe's OWN help, which v4 changed EARLIER; P4.D299 owns it — verify it lands.) `migrations`: v5 `main.rs` `SUBCOMMANDS` lists `"migrations"` but `match sub_name` routes it to `not_yet_available` (no `migrations_cmd.rs`; the runner is deferred, CLAUDE.md) — nothing to port; when a v5 `migrations status` arrives it must read BOTH id forms from day one. `docs/v4/packages-quilltap-README.md` is the mirror of `packages/quilltap/README.md` (byte-equal to `4a38222e6` AND `01a83539d`; P4.D303 re-vendors it at `83334b9a5`) → re-vendor at the next pin. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `8489d08b9` | 2026-10-14 | Release checklist 13: documentation freshness; tidy feature specs | **PORT (ONE SPA hunk — the About page's four blurbs) + a `docs/v4/` re-vendor with MOVES and DELETES; the remainder NO-PORT?** (classified 2026-10-15 from the hunks, §5.3; 72 files, +357 / −1,793; `4.10.0-dev.183`, parent `75d094e63`). **The one user-visible code hunk:** `app/about/AboutView.tsx` (+8 / −4) — four `<span>` blurbs: *Aurora – Characters* "…pronouns, aliases, clothing records, personalities, multi-character…" → "…pronouns, aliases, personalities, subprompts and progressions, a four-tier wardrobe with folders, pictures and a wear ledger, multi-character…"; *The Salon – Chat* "…server-side rendering, and embedded tool messages" → "…server-side rendering, embedded tool messages, Inform notes from the wings, the Scenario Builder, and indexed message search"; *The Commonplace Book – Memory* "long-term memory with semantic recall, memory gate, proactive recall, and deduplication" → "long-term memory with semantic recall and proactive recall, active and archived shelves, consolidation into digests, and one per-character maintenance pass"; *The Concierge* "content classification with detection, auto-routing to uncensored providers, quick-hide integration, and a three-state…" → "refusal failover to the uncensored desk, an opt-in pre-screen, quick-hide integration, a Try Uncensored shield on every reply, and a three-state…" (tail unchanged). **Every other `lib/` / `app/api/` / `migrations/` / `eslint.config.mjs` / `__tests__` hunk is a COMMENT path rename** `docs/developer/features/<x>.md` → `features/complete/<x>.md` (measured: `inform-block.ts`, `user-narration-anchor.ts`, `builtin-refit-policy.ts`, `avatar-cache.ts`, `cache-key.ts`, `extraction/prompts/shared.ts`, `recall/tags.ts`, `recall/tuning.ts`, `scenario-builder.service.ts`, `scenario-builder/route.ts`, `collapse-duplicate-avatar-rolls-v1.ts` — the migration hunk is a comment; NO migration logic, NO DDL, NO `lib/schemas` change). **Docs:** 17 specs `git mv` to `features/complete/` (`avatar-configuration-cache`, `builtin-tfidf-memory-tuning`, `composer-smart-typography`, `composer-spellcheck`, `composer-text-replacement`, the six `concierge-overhaul*`, `llm_api_costs_breakdown`, `prompt-trust-and-anti-committee`, `recall-multiplier-retuning`, `recall-probe-set-runbook`, `salon-inform`, `scenario-builder`); FOUR DELETED — `features/MCP.md` (265), `distant_future_separate_api.md` (654), **`native-core-api-boundary.md` (322), `native-port-phase-0.md` (298)** (v4's copies of THIS port's early design notes); `docs/developer/API.md` +98 / −16 (documents `characters/[id]?action=depiction-guidelines` GET/PUT, wardrobe tier `?action=instructions` GET/POST, `link-image`, chat `rebuild-system-prompt` / `accessible-stores` / `update-tool-settings` / `render-conversation`, the Document-Mode action table, `mount-points/[id]?action=reindex` / `embed`, the remaining chat-settings fields; DROPS the nonexistent `groups/[id]?action=stores` / `linkStore` / `unlinkStore`); `docs/releases/4.10.0.md` reordered + "Repairs through bug 246"; `CLAUDE.md` glossary rows Ariel / Suparṇā / Brahma + the Carina link; README (one Concierge paragraph three states, memory as tiers, FTS search, etc.); `bugs.md` + `bug-62` / `bug-202` link paths ONLY (no provenance change → no convergence); `.claude/commands/*`. Stamps. | **SPA:** v5 `apps/web/src/app/screens/about/about-page.ts:326` (Aurora), `:330` (Salon), `:338` (Commonplace Book), `:346` (Concierge) carry the PRE-commit sentences byte-for-byte, and `about.spec.ts:116` asserts the old Concierge text — the About page is a static content port of `AboutView.tsx` (p4.9 About + Profile; last re-synced in P4.D128 unit 3 and P4.D156 unit 2, "the About page's widened sentences" — same shape, four sentences this time). **`docs/v4/` mirror (P4.D303 owns `docs/v4/**` at `83334b9a5`):** `docs/v4/developer/features/` holds the 17 to-be-moved specs at their OLD paths (36 entries + `complete/` 101) and the four to-be-deleted files, incl. `native-core-api-boundary.md` / `native-port-phase-0.md` — the re-vendor must MOVE and DELETE, not just overwrite (`git mv` + `git rm`, then the `find docs/v4 -type f` count check the P4.D303 recipe uses); `docs/v4/developer/API.md` (248,889 B at `4a38222e6`) refreshes; `docs/v4/releases/4.10.0.md` lands when `docs/releases/` is mirrored (currently 4.9.0–4.9.2 only — confirm the recipe's `docs/**` glob picks it up). v5's own 39 files that cite `docs/developer/features/<x>.md` in carried why-comments (`crates/`, `apps/web/src`, porting docs) are v5 prose, not a port obligation — leave or tidy at leisure. The deleted `native-*` specs are v4's copies; v5's authoritative versions live under `docs/developer/porting/` and are unaffected. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `052659624` | 2026-10-14 | docs(bugs): file bug 247 — a `when` phrase naming "constructor" as its month throws and fails the extraction job | **NO-PORT? — a bug FILING, not a fix; the CONVERGENCE row is owed to the fix commit, which does not exist yet** (classified 2026-10-15 from the hunks, §5.3; 2 files, +74 / −2; parent `8489d08b9`; no `lib/` / `app/` / `packages/` / `components/` / `help/` hunk, no version stamp). NEW `docs/developer/bugs/bug-247-month-name-prototype-key.md` (71 lines): **Status OPEN**; **Found** "2026-10-13, the v5 port's differential harness (P4.D289, the `memory_time` family at the `83334b9a5` pin); reproduced 2026-10-14 at `8489d08b9`"; **Provenance "Original to v4"** — the named-date branch has read month words from a plain object literal since the episodic-spine `resolveWhenPhrase` (`8bf3cb5f`), moved unchanged into `lib/memory/time/clock.ts` by `e0ac7493a`; **Defect site** `clock.ts:94-97` (`MONTH_NAMES` is a `Record<string, number>` literal inheriting `Object.prototype`) and `:160-165` (`MONTH_NAMES[monthFirst[1]] !== undefined` / `MONTH_NAMES[dayFirst[2]] !== undefined` accept the inherited `constructor` — measured the ONE all-lowercase own property of `Object.prototype` that `[a-z]+` can reach), the throw escaping `lib/memory/extraction/run.ts:308` because `resolveEpisodicAnchors` sits OUTSIDE `admitDraft`'s per-candidate `try` (which guards only `admitCandidate`); `applyEpisodicFallbackAnchors` (`admission/prepare.ts:72-106`) is the one safe caller. Severity Low (needs a model to emit `when` as `"constructor 5"` / `"5 constructor"` / `"on constructor 5"` / `"constructor 5th"` / `"constructor 5, 2026"`); a turn job then FAILS whole and the queue retries up to three times (`queue-service.ts:522`), re-spending the cheap-LLM call, the OTHER pass and the `turn` cursor advance never happening. `WORD_NUMBERS` / `UNIT_DAYS` are read the same way but harmless (`0 < fn` false; the unit regex is closed). Proposed fix (NOT shipped): `Object.create(null)` / `Map` / `Object.hasOwn`, and moving `resolveEpisodicAnchors` inside the guard. `bugs.md` +3 / −2: the status paragraph gains "**247** is **open**", the row `| 247 | … | 2026-10-13 | — | Low | … | Pinned (\`PROTOTYPE_KEY_MONTH_THROWS\`) |`. **Still OPEN at v4 HEAD `362426c8f`** (no `bugs/fixed/bug-247-*`; the 2026-10-15 `bugs.md` status line still says "**247** is **open**"). The row's "v5 status" cell quotes the pin by name. **The message's one behavioural sentence ("toISOString throws") is WRONG and corrected by `809e5104d`** — the throw is `formatToParts` in `zonedParts`. | v5 `crates/quilltap-harness/tests/memory_time_equivalence.rs:317` — `PROTOTYPE_KEY_MONTH_THROWS: &[&str] = &["constructor 5", "5 constructor"]`, pinned BOTH ways (each phrase must still throw in v4 on every zone / anchor; v5 answers `{ occurredAt: null, window: null }`), asserted as `throws_measured == throws_table` ("drifted — v4's throwing phrases"); the lane record is **P4.D289** (the memory programme's PURE leaf family, `status-log.md:179561`); v5's closed month table in the `memory_time` port (`quilltap_core::memory_scoring` / the clock module). ⚠ When v4 FIXES 247, that commit is the **CONVERGENCE** row and this pin RETIRES by measurement (§5.4) — note the bug file lists FIVE shapes where the pin holds two, so the fix's oracle regen should widen the measured set before retiring. These two filings change nothing to compare. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `809e5104d` | 2026-10-14 | docs(bugs): bug 247 — name the real throw site (formatToParts in zonedParts, not toISOString) | **NO-PORT?** (classified 2026-10-15 from the hunks; 2 files, +4 / −3; parent `052659624`; docs only). `bugs/bug-247-month-name-prototype-key.md` +3 / −2 and `bugs.md` +1 / −1: the root-cause stack corrected — `zonedTimeToUtcMs(year, Object, day, 0, tz)` → `Date.UTC` coerces the `Object` function to `NaN`, the first-pass guess is `NaN`, `offsetMs` → `zonedParts` hands `new Date(NaN)` to `Intl.DateTimeFormat#formatToParts`, which throws `RangeError: Invalid time value` before `iso()` is reached (`clock.ts:167` → `calendar.ts:152` → `:134` → `:120`, measured at `8489d08b9`); the `bugs.md` row's "`toISOString` throws" becomes "`zonedParts`' `formatToParts` throws". No code, no behaviour. | None beyond `052659624`'s — the same `PROTOTYPE_KEY_MONTH_THROWS` pin (P4.D289); v5's own code comment at the pin should name `formatToParts`, not `toISOString`, if it quotes the mechanism. Ratify together with `052659624`. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `bbbcd2388` | 2026-10-14 | feat(wardrobe): copy to folder, and ask about components when filing outfits | **PORT-NEW + PORT — NO schema / migration / DDL move (verified: the `lib/schemas/wardrobe.types.ts` +23 / −2 hunk is REQUEST-body Zod only)** (classified 2026-10-15 from the hunks, §5.3; 24 files, +872 / −137; parent `809e5104d`; version stamp in both `package.json`s + the lockfile's root lines + the README badge; no `migrations/`, no `generateDDL`, no stored `WardrobeItem` field, no vault document field). **Schemas:** NEW `WardrobeFolderComponentModeSchema = z.enum(['move', 'copy', 'none'])`; `moveWardrobeItemsSchema` gains `components: WardrobeFolderComponentModeSchema.optional()`; NEW `copyWardrobeItemsSchema = { itemIds: z.array(z.string().min(1)).min(1, 'At least one item is required').max(500), folder: WardrobeFolderInputSchema, components: z.enum(['copy', 'none']).optional() }`. **Server:** NEW `lib/wardrobe/folder-filing.ts` (199 lines, server-only) — `fileItemsToFolder(repos, userId, location, { action: 'move' \| 'copy', itemIds, folder, components? })` → `{ items, componentsCarried }`; `componentMode = components ?? 'none'`; `copy` + `move` → `FolderFilingRequestError('Copying an outfit cannot move its components — the original outfit still needs them')` (a **400** via `writeErrorResponse` → `badRequest(error.message)`); **the plain path (`move` + `none`) delegates to the EXISTING `moveItemsToFolder(location, itemIds, folder)` and returns `componentsCarried: 0` with NO new log line** — so `move-items` without `components` is behaviourally unchanged except the response gains the key and the INFO line gains two fields; every other combination: `location.resolveFolder(folder)`, `uniqueIds` (Set-deduped), `location.readItems(true)`, missing ids → `WardrobeItemNotInLocationError(missing)` BEFORE any write, `components = collectContainerComponents(requested.filter(isComposite), wardrobeItems)` minus ids already in the request; `move`+`move` → ONE `moveItemsToFolder(location, [...uniqueIds, ...componentIds], target)` returning `moved.slice(0, uniqueIds.length)`; copies (`components === 'copy'` → the components; `action === 'copy'` → the components THEN the requested items): `idMap` of `randomUUID()`, `remap(ids)` = `idMap.get(id) ?? id`, `takenTitles` seeded from the whole wardrobe and grown per copy, `nextCopyTitle(original.title, takenTitles)` (the Duplicate helper — `(copy)` titles), `mountPointId = location.writableMountPointId()`, one `now`; per copy `carryItemImages(repos, { mode: 'copy', sourceItemId, destinationItemId: id, destinationMountPointId, userId })` → `fileIdMap`, then `createItem(location, { ...original, title, folder: target, componentItemIds: remap(original.componentItemIds) }, { preserve: { id, createdAt: now, updatedAt: now, archivedAt: original.archivedAt ?? null, imageFileId: fileIdMap.get(original.imageFileId) ?? null, migratedFromClothingRecordId } })` (the pre-existing `preserve` option of `item-mutations.ts:101-125`; an archived original yields an archived copy); `move`+`copy` → `location.updateMany(requested.map(item => ({ id, patch: { folder: target, ...(isComposite ? { componentItemIds: remap(...) } : {}) } })), { requireAll: true })`; DEBUG `[FolderFiling] Filed wardrobe items into folder` `{ scope, mountPointId, action, componentMode, folder, requested, componentsCarried, context: 'wardrobe' }`. `lib/wardrobe/expand-composites.ts` +31: `collectContainerComponents<T extends CompositeNode & { id }>(composites: readonly T[], containerItems)` — MOVED from `app/api/v1/wardrobe/transfers/route.ts` (−27 there) and generalised from one composite to many (`visited` seeded with every composite id; `componentItemIds ?? []`); the transfers route now calls `collectContainerComponents([source.item], source.containerItems)` — same result for one composite. `lib/wardrobe/routes/wardrobe-route-factory.ts` +42: `handleFileItems(action, body, ctx, params)` shared by `handleMoveItems` and NEW `handleCopyItems` (`copyWardrobeItemsSchema.parse`); INFO `` `${logTag} ${action === 'move' ? 'Moved' : 'Copied'} wardrobe items to folder` `` with NEW fields `components: body.components ?? 'none'`, `componentsCarried: result.componentsCarried` beside `count: result.items.length`; response `successResponse({ items: serializeWardrobeItems(result.items, location.origin, repos), componentsCarried })`; `'copy-items': handleCopyItems` added to the POST action table (so `?action=copy-items` on all four tiers; the module doc lists it after `move-items`). **Client (v4 React):** `lib/hooks/use-wardrobe-folders.ts` — `WardrobeFolderComponentMode`, the mutation's `action` union + `'copy-items'`, `fileItems(mode, container, itemIds, folder, components?)` → `moveItems` / NEW `copyItems` (body `{ itemIds, folder, ...(components ? { components } : {}) }`); NEW `components/wardrobe/WardrobeComponentModeFieldset.tsx` (83) — `ComponentMode`, `defaultComponentMode(mode)` (`move` → `'move'`, `copy` → `'copy'`), radios for a move `Move the components along with it` / `Copy the components (originals stay behind)` / `Leave the components behind`, for a copy `Copy the components along with it` / `Copy the outfit alone`, the all-or-nothing footnote in two wordings (transfer: "Only components living in the same wardrobe travel…"; folder: "Only components filed in this wardrobe travel…"); `WardrobeFolderPickerDialog.tsx` +93 / −? gains `mode`, the fieldset when outfits are among the items (legend `This outfit bundles N component(s)` / `N of these are outfits, bundling M components between them`), `onSubmit(folder, components?)` with `components` set only when outfits were present, the current folder disabled ONLY for a move (`mode === 'move' && isSameFolder`), verb `Move` / `Copy`; `WardrobeTransferDialog.tsx` −? swaps its radio group for the fieldset; `WardrobeBrowser.tsx` `fileToFolder` (copy sends `components === 'move' ? undefined : components`), the select bar's `Copy to folder…`, toasts `A copy of "T" now hangs in …` / error fallback `The items would not be copied`; `wardrobe-item-row.tsx` `onCopyToFolder`; `useWardrobeItemActions.ts` `openFolderPicker(items, mode = 'move')`, `TransferRequest.mode`. **Help:** `help/wardrobe.md` +6 / −? (the `Copy to folder…` bullet, `Outfits ask after their components`, the select bar now `N selected · Move to folder… · Copy to folder… · Cancel`) and `help/project-wardrobe.md` 1 line — the help file COUNT is unchanged (131 at `4a38222e6`, 131 at HEAD). Docs: `API.md` (+`copy-items` row, `move-items` row rewritten), `features/complete/wardrobe-folders.md`, CHANGELOG. Tests: `folder-filing.test.ts` 207 (8 cases incl. the refused copy-moves-components and the refuse-before-write), route factory +40 (2 cases: `componentsCarried` 1; `Cloak (copy)` in `Formal` with a fresh id), the folders jsdom +53. **Message vs hunks: consistent** — every claimed behaviour has its hunk; the one thing the prose under-states is that `move-items`'s RESPONSE SHAPE changes for every caller (`componentsCarried` always present). | v5 core: `crates/quilltap-core/src/api/wardrobe_route_factory.rs` (**P4.D281 unit 2** — the five folder actions; `move_items_to_folder` at `:1057`, INFO `Moved wardrobe items to folder` at `:1067` pinned byte-wise incl. a `generalId=null userId=…` literal at `:1640`) — needs a `copy-items` arm, the shared file-items path, the two new INFO fields and `componentsCarried` on the wire; `api/types.rs` `WardrobeItemsMove` + a NEW copy arm and a `COLLAPSE_CENSUS` row; `quilltap-web/src/wardrobe_routes.rs` + `characters_routes.rs` `COLLECTION_POST_ACTIONS` literal `["instructions", "create-folder", "rename-folder", "delete-folder", "move-items"]` (P4.D281 unit 5); families `wardrobe_folders_routes_equivalence` (web, over v4's REAL four route files), `characters_wardrobe_route.rs`, `wardrobe_instructions_routes.rs`, `dispatch_wrong_type_census.rs` — the move-items rows' response bytes WILL trip at the baseline move (the extra key); `services/wardrobe_transfers.rs:410 collect_container_components` (**P4.D281 unit 3**) — v4's twin moved to `expand-composites.ts` and took a slice of composites; `services/wardrobe_item_mutations.rs` `create_item` + `CreateItemOptions.preserve` / `PreservedIdentity` (P4.D279 keystone, used by transfers), `services/wardrobe_item_images/carry.rs` (P4.D263 / P4.D281 — the copy mode), the Duplicate `(copy)`-title helper, `db/wardrobe_location.rs` (`resolve_folder` / `read_items` / `writable_mount_point_id` / `update_many` — P4.D279), `wardrobe_folders.rs` `WardrobeItemNotInLocation` (P4.D281). SPA: `apps/web/src/app/wardrobe/wardrobe-folder-picker-dialog.ts` (**P4.D285 unit 5**, SPA 0.5.840 — bug 229's shape), `wardrobe-transfer-dialog.ts` (**P4.D286** — its radio group becomes the shared fieldset), `wardrobe-browser.ts`, `wardrobe-item-actions.ts` (`openFolderPicker`), `wardrobe-folders.api.ts` (`move-items` literal; needs `copy-items`), the item row. Help: `help/wardrobe.md` + `help/project-wardrobe.md` re-vendor (p4.9i2 / **P4.D287**'s tree, byte-equal to `4a38222e6` today; count 131 → 131); `docs/v4/` API.md + the wardrobe-folders spec. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `b82037d75` | 2026-10-14 | fix(scriptorium): queue chunk embeddings from the pre-write read (bug 248) | **PORT — bug 248 FIXED the day it was filed; NOT a convergence (Original to v4, found on Ignite, `v5 status: Not assessed`); NO schema move** (classified 2026-10-15 from the hunks, §5.3; 11 files, +281 / −36; parent `bbbcd2388`; two production files, two tests, the bug file (`bugs/fixed/bug-248-render-embed-one-behind.md`, 81 lines), `bugs.md`, CHANGELOG, version stamps). **Provenance:** "Original to v4. The post-upsert re-read dates from Scriptorium Phase 1 (`a76240c27`, 2026-04-11); moving job handlers into the forked child made that read stale. Bug 17's fix (`62ab1bc8f`) made a content change clear the vector" — the defect is the Node job child's BUFFERED writes (a job cannot read its own writes), a mechanism v5's single-writer never had. Symptom: Ignite 108 of 1,634 `conversation_chunks` with `embedding IS NULL` after the upgrade's 98-job bulk re-render, 106 of them with an `embedding_status` row saying `EMBEDDED`; Friday's new chunks embedded 4–50 minutes late (at the NEXT render). **Hunks:** `lib/background-jobs/handlers/conversation-render.ts` (+71 / −?) — step 3 now reads `existing = repos.conversationChunks.findByInterchangeIndex(chatId, interchange.index)` BEFORE each `upserted = upsert({...})`, `needsEmbedding = payload.fullReembed \|\| !existing \|\| !existing.embedding \|\| existing.content !== interchange.content`, `chunkId = existing?.id ?? upserted?.id`, `if (needsEmbedding && chunkId) toEmbed.push(chunkId)`; NEW unconditional DEBUG `[ConversationRender] Chunks upserted` `{ jobId, chatId, interchangeCount, needingEmbedding: toEmbed.length, fullReembed: !!payload.fullReembed }`; step 4's gate becomes `toEmbed.length > 0` (was `result.interchanges.length > 0`), the post-upsert `findByInterchangeIndex` re-read loop and `embeddedCount` are DELETED, the enqueue iterates `toEmbed` with the same `{ entityType: 'CONVERSATION_CHUNK', entityId, chatId, profileId: defaultProfile.id }`; NEW DEBUG `[ConversationRender] No default embedding profile; chunks left for the startup reconcile` `{ jobId, chatId, pending: toEmbed.length }` in the no-default-profile arm. `lib/database/repositories/conversation-chunks.repository.ts` (+17 / −1) — `upsert`'s insert arm destructures `{ id: suppliedId, ...data }` and calls `_create(data, typeof suppliedId === 'string' ? { id: suppliedId } : undefined)` (the id kept OUT of the row data; the base repository mints one when none is supplied); NEW bare DEBUG `Creating conversation chunk` `{ chatId, interchangeIndex, suppliedId: <boolean> }`; `logger` imported. Tests pin: a changed chunk queued despite the stale vector (`chunk-1`, never the `synthetic-id`), a new chunk queued under the upsert's id (`new-1`), the interleave `['read', 'write', 'read', 'write']`, and the insert keeping / minting the id. **Under read-your-writes the per-chunk predicate is EQUIVALENT to the old one** (the upsert NULLs the vector exactly on a content change — bug 17 — so `!chunk.embedding` after the write ⇔ `!existing \|\| !existing.embedding \|\| content changed` before it), so the behavioural delta for v5 is: the read/write INTERLEAVE (a SQL-trace-visible order change), a chunk absent before AND unreturned by the upsert is skipped (was: skipped), the gate on `toEmbed` (no profile lookup when nothing needs embedding — the `findEmbeddingProfiles` read disappears for a no-op render), and the three new DEBUG lines. **The message's data claims ("108 on Ignite", "one render late") describe v4's child, not a hunk; v5 never had the lag.** | v5 `crates/quilltap-core/src/services/conversation_render_job.rs` (**P4.6BM unit 2** — the `CONVERSATION_RENDER` handler, `status-log.md:46281`): faithful to v4's PRE-fix `:46-98` — the upsert loop (`:166-186`, `db.write(... .upsert(&input, &new_id, &now_for_write))`) THEN `enqueue_embeddings` (`:217-262`) re-reading `find_by_interchange_index` and gating on `payload.full_reembed \|\| !chunk.has_embedding`, the whole block caught (`[ConversationRender] Failed to enqueue embedding, continuing` — the `an_enqueue_failure_warns_with_v4s_field_names` test); `db/conversation_chunks.rs:591 upsert(input, new_id, now_iso)` ALREADY takes the caller's id (the chunk-on-write round **P4.6BK**, `status-log.md:5580`; the bug-17 `embedding_remainder` family **P4.D51 / P4.D52**, `status-log.md:64431`), so the repository half is a log line only; `services/conversation_render_reconcile.rs` (P4.6BM unit 3 — unchanged by v4); `quilltap-host/src/spine.rs` job registration; `generators/refresh_archive.rs` / `services/chat_admin.rs` / `queue_service.rs` (`fullReembed` producers — unchanged); the `embedding_remainder_equivalence` harness family and any production-SQL-capture plan that pins the render job's statement ORDER. Port = the pre-write read + predicate + `existing.id ?? new_id`, the `toEmbed` gate, the three DEBUG lines (two `[ConversationRender]`, one bare `Creating conversation chunk`). | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `3f64d3a6c` | 2026-10-14 | docs(README): Updated README and broke out feature docs | **NO-PORT? (docs only — but it moves `docs/**`, so the mirror grows)** (classified 2026-10-15 from the hunks, §5.3; 5 files, +190 / −214; stamp unchanged at `b82037d75`'s `4.10.0-dev.185`, **parent `b82037d75` (bug 248 — "fix(scriptorium): queue chunk embeddings from the pre-write read", 11 files, +281 / −36) — NOT `8489d08b9`; that commit is outside this batch and needs its own row**). `README.md` −211 / +61: the whole "Core Features" section (AI Desktop → Image Generation, incl. the four `**bold**` accretion paragraphs — the per-character vault, archives, the Concierge's four-state description, "Memory protection now favors…"), "Where your data lives / What's stored / Backup options", "Architecture at a glance / Getting started / Contributing", "Tech Stack", "Support", the long Troubleshooting block REMOVED; NEW "Your First Ten Minutes", "A Tour of the House" (one paragraph each for the Salon, Aurora, the Commonplace Book, the Scriptorium, Prospero, the Concierge, the Host, Pascal, the Lantern, and a Librarian/Carina/Ariel line), "Providers", "Themes", "Troubleshooting & Support". NEW `docs/FEATURES.md` (119 lines, "A Tour of the House: Quilltap's Features in Full") holds the displaced sections with the "now" phrasing removed. **Corrects two of v4's own false claims — the ones CLAUDE.md's standing rule warns about:** every "SQLCipher" in the README is gone ("encrypted at rest with ChaCha20-Poly1305 (via SQLite3 Multiple Ciphers)"; Tech Stack "better-sqlite3-multiple-ciphers" with the parenthetical dropped) and "with WAL mode" is gone (TRUNCATE default, WAL opt-in); `CLAUDE.md`'s CLI note now reads "Databases are encrypted (ChaCha20 via SQLite3 Multiple Ciphers — not SQLCipher, despite the identifiers)". It does **NOT** touch `docs/developer/DATABASE_ENCRYPTION.md`, which still asserts SQLCipher. `CLAUDE.md` + `.claude/commands/update-documentation.md` gain the README / `docs/FEATURES.md` index rows. No code, schema, migration, `help/`, dependency or bug-file hunk; **NO version-stamp hunk at all** (no `package.json`, no badge line — `4.10.0-dev.185` is `b82037d75`'s stamp, measured; this commit is the first in the batch that does not bump). | v5 mirrors v4's `docs/**` under `docs/v4/` (+ `packages-quilltap-README.md`) but NOT the root `README.md` or `CLAUDE.md` — so the ONLY v5 effect is a NEW `docs/v4/FEATURES.md` at the next re-vendor (P4.D303's recipe: `git ls-tree … docs/` → `docs/v4/${f#docs/}`; the `find docs/v4 -type f` count +1) and `docs/v4/CHANGELOG.md`. The ChaCha20 correction is v4 converging on THIS port's Phase-0 cipher finding (`phase-0.md`; CLAUDE.md "The database cipher is ChaCha20/sqleet, NOT SQLCipher") — a documentation convergence worth a one-line note in `phase-0.md` / the standing rule ("v4's README and CLAUDE.md now say so too, as of `3f64d3a6c`; `DATABASE_ENCRYPTION.md` still does not"), not a code row. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `2432b9057` | 2026-10-14 | release: 4.10.0 | **NO-PORT? — the release squash** (classified 2026-10-15 from the hunks, §5.3). First parent `8fbf2afe0` = "release: 4.9.2" (2026-09-06), so `git diff 2432b9057^ 2432b9057 --stat -- lib app packages plugins` (1,013 files / +215,058 / −75,729) is the ENTIRE 4.10.0 dev cycle — 2,740 main commits, each already a row here or in §6 — and NOT new content; against main's pre-merge tip `3f64d3a6c` the squash differs in exactly FIVE stamp files (`README.md` badge, `docs/CHANGELOG.md`, `docs/releases/4.10.0.md`, `package.json` + `package-lock.json`: `4.10.0-dev.N` → `4.10.0`). No `lib/` / `app/` / `packages/` / `plugins/` hunk is new relative to main. **`release` tip = `2432b9057`** (`merge-base main release` = itself). **Bugfix measured by CONTENT (§4 step 2):** tip `489b228b6` "bugfix: started 4.10.0 bug branch" (2026-10-14, parent `1a2b2164c` the 4.9.2 bug-branch start); `git diff 2432b9057 bugfix -- lib app packages plugins` EMPTY, all paths = 3 stamp files (`4.10.0` → `4.10.1-bugfix.0`); `git diff main bugfix -- lib app packages plugins` = 26 files / +278 / −998 = exactly main's post-release `7458ff94e` + `362426c8f` seen backwards. **Bugfix carries ZERO unabsorbed content.** ⚠ §4 step 2's "still no `release: 4.10.0` squash" and CLAUDE.md's baseline bullet are now stale — re-probe BOTH branches as before, but the squash exists. | None (no code). The port's version strings are its own (`quilltap-core` 0.0.x etc.); nothing reads v4's `package.json` version. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `f573a7576` | 2026-10-14 | merge: 4.10.0 back into main | **NO-PORT?** (classified 2026-10-15 from the hunks). A two-parent merge (`3f64d3a6c` + `2432b9057`); `--stat` 5 files / +6 / −6 — the same five stamp files; the combined diff (`git show f573a7576`) is EMPTY. No code, no schema, no dependency move. | None. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `37246654d` | 2026-10-14 | dev: started 4.11.0 development | **NO-PORT?** (classified 2026-10-15 from the hunks). 4 files / +6 / −4: README badge `4.10.0` → `4.11.0--dev.0`, `docs/CHANGELOG.md` +2 (an empty `### 4.11-dev` heading), `package.json` + lock `4.11.0-dev.0`. No code. From here v4 stamps `4.11.0-dev.N`. | None. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `1e1712a12` | 2026-10-14 | docs(CHANGELOG): moved 4.9.x changes to v4 docs | **NO-PORT?** (classified 2026-10-15 from the hunks). 2 files / +57 / −57: the `### 4.9.2` section (bugs 124 / 125, both ABSORBED long since) moved verbatim from `docs/CHANGELOG.md` to `docs/CHANGELOG_V4.md`. No code. (The sibling `7458ff94e` moves 3,600 more lines the same way.) | `docs/v4/` mirror only, IF it carries `CHANGELOG*.md` — re-vendor with the next `help/` + `docs/v4/` lane (the `p4.9i2` / P4.D278 / P4.D287 shape). | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `7458ff94e` | 2026-10-14 | fix(wardrobe): mark a styled hair slot as overriding the natural hair | **PORT — a model-visible TEXT move on a ported pure function; the ±3,600 CHANGELOG hunk is a docs MOVE; NOT a convergence (no bug number)** (classified 2026-10-15 from the hunks, §5.3; 10 files, +3,630 / −3,606 — 7,200 of them `docs/CHANGELOG.md` → `docs/CHANGELOG_V4.md`; **parent `1e1712a12`, NOT `b82037d75`** — five commits sit between, incl. `2432b9057` `release: 4.10.0` and `37246654d` `dev: started 4.11.0 development`; this commit is the first `4.11-dev` entry; `4.11.0-dev.*` stamps in both `package.json`s + the lockfile + the README badge). **Code (18 lines):** `lib/wardrobe/slot-guidance.ts` +11 — NEW `export const HAIR_STYLING_PRECEDENCE_NOTE = "current styling — overrides the natural hair's texture and style in the physical description"` beside the three existing `HAIR_*` constants (`HAIR_SLOT_GUIDANCE`, `HAIR_PHYSICAL_BOUNDARY`, `HAIR_PHYSICAL_DESCRIPTION_NOTE` — unchanged); `lib/wardrobe/outfit-description.ts` +7 / −1 — in `describeOutfit`'s grouped-slots loop `const note = slotsForValue.includes('hair') ? \` (${HAIR_STYLING_PRECEDENCE_NOTE})\` : ''` appended: `` `- **${slotsForValue.join(', ')}:** ${value}${note}` `` — so `- **hair:** braided crown, silver pins (current styling — overrides the natural hair's texture and style in the physical description)`; the note rides ANY group containing `hair` (a multi-slot item equipped across `hair` + another slot puts it on the joined `**accessories, hair:**` line); the naked collapse, the empty-hair silence (rule 13 — no negative-space line) and the garment fallbacks are untouched (hair has no fallback, so an empty hair slot never carries the note). The doc comment gains one bullet. Reaches every `describeOutfit` caller with NO caller hunk: `lib/wardrobe/item-image-prompt.ts`, `lib/wardrobe/resolve-equipped.ts`, `lib/image-gen/appearance-resolution.ts`, `lib/image-gen/avatar-prompt.ts`, `lib/tools/handlers/wardrobe-handler-shared.ts`, `lib/memory/cheap-llm-tasks/image-scene-tasks.ts`, `lib/services/aurora-notifications/writer.ts` — i.e. image prompts, the wardrobe tools' output, Aurora's announcements, the Lantern, and (transitively, since the catalogue quotes the prompt) kept-photo catalogues, exactly the message's list; the message's "kept-photo catalogues" has no hunk of its own and is true only through the prompt text. Test: `outfit-utils.test.ts` two expectations now interpolate the constant. **Help:** `help/wardrobe.md` +3 / −1 — "Three consequences follow" → "Four", NEW bullet "**A styled coiffure outranks the natural hair.** … the hairdo is marked as the current styling, taking precedence over the natural texture and style…" (count 131 unchanged). **Docs:** the 4.10.0 CHANGELOG section (bug 248, the copy-to-folder entry, release checklists 10–13…) moves whole into `CHANGELOG_V4.md`, continuing `1e1712a12`'s split; the `4.11-dev` heading gains "Fix: styled hair slot read as contradicting the physical description". **NO schema / migration / dependency move.** | v5 `crates/quilltap-core/src/wardrobe.rs:309 describe_outfit` (**P4.D87 unit 1** — the hair slot's core half, `status-log.md:76258`; re-homed into the keystone by **P4.D279**): the `OrderedGroups` loop at `:72-73` `format!("- **{}:** {}", slots_for_value.join(", "), value)` needs the hair-group note; the `HAIR_*` constants block at `wardrobe.rs:144-169` (P4.D87; the duplicate strings folded at `status-log.md:90934`) gains a fourth constant; `generators/field_semantics.rs` (reads `HAIR_PHYSICAL_DESCRIPTION_NOTE` — unchanged). Callers whose prompt / tool-output pins WILL trip: `tools/wardrobe_wear.rs`, `tools/wardrobe_take_off.rs` (**P4.D280** — the wear text), `services/resolve_equipped.rs` (**P4.D282**), `services/appearance_resolution.rs`, `services/wardrobe_item_image_prompt.rs`, `services/image_scene_tasks.rs` (**P4.D283** — note P4.D283's review already re-shaped the appearance prompt to v4's struct), the Aurora writer; `tests/unreported_if_blank_slots.rs` and the tier-1 `describe_outfit` family (the P4.d5 ∥ P4.d6 ∥ P4.6ay round, `status-log.md:13025`) are the direct oracle pins. Help: `help/wardrobe.md` re-vendor (p4.9i2 / **P4.D287** tree). No SPA surface renders `describeOutfit`. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `936a34720` | 2026-10-15 | docs(features): spec Pascal wardrobe effects, trigger phrases, and composer slash commands | **NO-PORT? — docs-only, VERIFIED** (classified 2026-10-15 from the hunks; `git show --stat 936a34720 -- lib app packages plugins components help public migrations` → EMPTY). 5 files / +843 / 0: three NEW proposals — `docs/developer/features/pascal-trigger-phrases.md` (305; a `triggers` key + an off-by-default `chatSettings.customToolTriggers`, the scanner / runner at the two auto-RNG sites, the prerequisite one-run-service extraction — IMPLEMENTED by `362426c8f` an hour later, which moves it to `complete/`), `pascal-wardrobe-effects.md` (266; `effects[].wardrobe` wear / take_off / clear_slot through `wearItem` / `takeOffItem` / `applyDisplacement`, one `notifyWardrobeChanged` per run, audit in `pascalMeta.effects` — the checkout's UNCOMMITTED dirt, in flight), `composer-slash-commands.md` (262; `/` commands in the Salon composer over composer actions, built-in tools and Pascal's roster — supersedes Phase 4 of `composer-typeahead.md`; UNIMPLEMENTED) — + the catalogue + a `4.11-dev` changelog line. A planning input, not a port: two of the three specs become PORT-NEW rows when their code lands. | `docs/v4/` mirror (`docs/developer/features/`). Forewarning for: the Pascal server family (P4.6ay / P4.d10 / P4.6be / P4.D169 — `pascal/side_effects.rs`), the wardrobe keystone (P4.D279 `wear-ops`), the composer typeaheads vertical (the SPA composer lane). | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `362426c8f` | 2026-10-15 | feat(pascal): trigger phrases run custom tools when their words are said | **PORT-NEW + PORT — WITH A D23 MOVE (the NEXT re-dump after the NINTH, `4ae0c4ca6`; ONE new migration)** (classified 2026-10-15 from the hunks, §5.3; 58 files, ~+2,100 / −430; `4.11.0-dev.1` → `.3`; parent `936a34720`; 11 test files + the Almanack snapshot are ~a third). **SCHEMA / MIGRATION — flag loudly:** NEW `migrations/scripts/add-custom-tool-triggers-field.ts` (registered in `index.ts` import + `migrations[]` + re-export, three hunks; `id: 'add-custom-tool-triggers-field-v1'`; `introducedInVersion: '4.11.0'`; `dependsOn: ['add-custom-tools-field-v1']`; `shouldRun` = SQLite ∧ `sqliteTableExists('chat_settings')` ∧ `!sqliteColumnExists('chat_settings','customToolTriggers')`; `run` = `addColumnIfMissing('chat_settings', 'customToolTriggers', 'INTEGER DEFAULT 0')`, no backfill; INFO `Added customToolTriggers column to chat_settings table` — logged TWICE on a real add, once per-column inside the `if` with `{context}` and once after with `{context, columnsAdded, durationMs}` — ERROR `Failed to add customToolTriggers column`, `context: 'migration.add-custom-tool-triggers-field'`; result `message: 'Added customToolTriggers column to chat_settings table'` / `'Failed to add customToolTriggers column'`; `prettify.ts` label "Teaching Pascal which words to listen for" — NO-PORT per the standing prettify ruling). `lib/schemas/settings.types.ts` `ChatSettingsSchema` gains `customToolTriggers: z.boolean().default(false)` IMMEDIATELY after `customTools` and before `compositionModeDefault`, so v4's `generateDDL` renders a fresh `chat_settings` with `"customToolTriggers" INTEGER DEFAULT 0` between `"customTools" INTEGER DEFAULT 1` and `"compositionModeDefault" INTEGER DEFAULT 0` (v5's `fresh_schema.json` `main` has `customTools` at that very spot, index 20 — `chat_settings_seed.json` columns 20/21) — **`fresh_schema.json` must be RE-DUMPED from a pin at or past `362426c8f` (D23, never by hand), `chat_settings_seed.json` gains `customToolTriggers: 0`, and the migration is re-homed as an UNSTAMPED table-gated boot ensure beside the `customTools` one** — the `impersonation-voice-mode` / `wardrobe-image-settings` repair shape (`db/chat_settings_*_repair.rs`), differentially compared against v4's REAL migration (`table_info` + `sqlite_master`), and added to `restore_vintage_state`'s boot-align list (the memory note). `DDL.md` +1 `chat_settings` line, +`invokedBy` / `triggerPhrase?` prose on `chat_messages.pascalMeta` ("ride the JSON column with no migration"). `chat-settings.repository.ts` default row `customToolTriggers: false`. **No dependency move** (`package.json` / lock / `packages/quilltap/package.json` stamps only — NO CLI code hunk; Tier R untouched). **NOT a convergence** (no bug number). **(a) The definition — PORT-NEW on `QtapCustomToolSchema`** (`custom-tool.types.ts` +51): `MAX_TRIGGERS = 8`, `MIN_TRIGGER_PHRASE_LENGTH = 3`, `MAX_TRIGGER_PHRASE_LENGTH = 80`; `triggers: z.array(z.string().min(3).max(80)).min(1).max(8).optional()` placed after `chipLabel` / before `description`; a FOURTH superRefine `validateTriggers`: `trigger N has leading or trailing whitespace` (`phrase !== phrase.trim()`, `path: ['triggers', i]`) and `trigger N repeats trigger M (phrases match regardless of case)` (`toLocaleLowerCase` fold). `public/schemas/qtap-custom-tool.schema.json` +8 (`triggers`: array, `minItems 1`, `maxItems 8`, `uniqueItems`, items `minLength 3` / `maxLength 80`, the long description ending "Never sent to the model.") → v5's `apps/web/public/schemas/qtap-custom-tool.schema.json` byte-copy (the standing rule). The dice specimen `CUSTOM_TOOL_SPEC_DICE.json` gains `"triggers": ["make a saving throw", "roll to save"]` + a `Demonstrates:` sentence on the second outcome. `workbench.ts` `CustomToolLibraryEntry.triggerCount = definition.triggers?.length ?? 0` (a NEW wire key on the library response). `tool-draft.ts` +59: `ToolDraft.triggers: string[]` (round-trip `draftFromDefinition` / `definitionFromDraft` — emitted only when non-empty), a NEW `DraftIssue.where` arm `{ section: 'triggers'; index? }`, `validateDraftTriggers` (the loader's four rules as errors in the draft's own words — `at most 8 trigger phrases`, `a trigger phrase needs at least 3 characters`, `… is at most 80 characters`, `a trigger phrase must not begin or end with a space`, `"X" is listed twice (phrases match regardless of case)` — plus the SELF-ECHO warning `${what} contains the trigger "${phrase}" — a character who repeats Pascal's line will set this tool off again` over every outcome message, the chip label and, when `llmEnabled`, the consult prompt). **(b) The scanner — NEW `lib/pascal/trigger-phrases.ts`** (92, pure, client-safe): `MAX_TRIGGER_RUNS_PER_MESSAGE = 4`; `foldTriggerText = toLocaleLowerCase` (the mention ranker's fold — v5's `str::to_lowercase` byte-identity ruling applies); `triggersContainedIn(text, triggers)`; `scanTriggerPhrases(content, tools)` → `{ hits, dropped }`: strips Carina query lines FIRST (NEW `stripCarinaQueryLines` in `lib/chat/carina-parser.ts` +72 — `parseCarinaQuery`'s loop body extracted into `parseQueryLine(line)`, `stripCarinaQueryLines` filters EVERY line that parses as a query, `split(/\r?\n/)…join('\n')` — behaviour of `parseCarinaQuery` itself unchanged), folds, `indexOf` per phrase, ONE hit per tool (its earliest phrase), stable `sort` by index (ties keep roster order), `slice(0, 4)` / the rest `dropped`. Plain substring — "fire" matches "fireplace" — no boundaries. **(c) The runner — NEW `lib/pascal/trigger-runner.ts`** (241): `triggersEnabled(cs) = cs?.customTools !== false && cs?.customToolTriggers === true`; `runTriggeredCustomTools` NEVER throws (WARN `Trigger-phrase scan failed; no custom tools fired` `{context:'pascal.trigger-runner', chatId, speakerParticipantId, error}`); speaker perspective — a character seat loads `characters.findById` (`metadata`, `characterDocumentMountPointId`; WARN `Trigger-phrase scan skipped — speaking character not found` / `… unreadable`), the bare operator rolls as nobody (`{}` metadata, null mount); `resolveCustomToolRoster` per call, never cached (gates + `disabled` honoured by the roster); over-cap WARN `Trigger phrases over the per-message cap; later tools not fired` `{fired, dropped}`; `whisperTargetIds = speaker.participantId ? [it] : [userId]`; per hit `runCustomTool({ parameters: null, private: undefined, invokedBy: 'trigger', callerParticipantId: speaker.participantId, triggerPhrase: hit.phrase, nowMs })` — ONE `Date.now()` for the whole message; a refused run → DEBUG `Triggered custom tool refused` + `postProsperoCustomToolError({ toolName, reason, whisper, callerParticipantId: whisperTargetIds[0] })` and CONTINUE; `posted === null` → DEBUG `Triggered custom tool ran but its announcement did not land` + Prospero `the outcome could not be posted to the chat`; success → `onPosted(msg)` + INFO `Custom tool fired by trigger phrase` `{tool, phrase, speakerParticipantId, invokedBy:'trigger', state}`. **(d) The sites — NEW `lib/services/chat-message/trigger-phrase-sites.ts`** (107): `runUserMessageTriggers` (returns `[]` on `isContinueMode` or empty content; seat = `resolveRespondingParticipant`'s — a user-driven / impersonated CHARACTER seat speaks as that character; `existingMessages.push(...triggered)` — the in-memory splice so the responding model hears Pascal) and `runReplyTriggers` (the character's own perspective; NO splice — a chained next speaker re-reads the transcript). `orchestrator.service.ts` +25 calls `runUserMessageTriggers` AFTER the user message is persisted and the RNG detector, BEFORE the Carina user-message path (the dice land BEFORE the message; Pascal's announcement must FOLLOW it); `message-finalizer.service.ts` +26 calls `runReplyTriggers` after the saved reply (`cleanedResponse`), BEFORE the done event and the Carina assistant-markup path; both `onPosted = safeEnqueue(controller, encodePascalResultEvent(encoder, msg))` — the `pascalResult` SSE event (pre-existing at `streaming.service.ts:870`) now ALSO emitted from the finalizer. Swipes never reach either site (`regenerate-swipe.service.ts`) — fires nothing, matching the dice. **(e) The extraction — PORT (a refactor of two ported entrances):** NEW `lib/pascal/run-service.ts` (209) `runCustomTool(params) → { ok: true, result, posted, appliedEffects } | { ok: false, reason, whisper }` — cascade read ONCE (`params.chat ?? chats.findById`; `groupScope` = `{kind:'character'}` or `{kind:'none'}`; DEBUG `Custom tool run: state cascade unavailable, defaulting to {}`), `flattenProgressions(metadata, nowMs)`, `executeCustomTool` (`CustomToolRunError` → `{ok:false}` with DEBUG `Custom tool run refused` `{invokedBy}`; anything else rethrown), `applyCustomToolEffects` BEFORE the post, `buildPascalResultContent` + `postPascalResult` with the full `pascalMeta` (now `invokedBy: params.invokedBy`, `…(invokedBy === 'trigger' && triggerPhrase ? { triggerPhrase } : {})`, `callerParticipantId` when set), DEBUG `Custom tool dealt` `{tool, tier, invokedBy, state, effects, whispered, posted}`. `run-custom-handler.ts` −138 / +~30: the cascade / progress / execute / effects / post block REPLACED by one `runCustomTool({ …, whisperTargetIds: callerParticipantId ? [it] : null, invokedBy: 'llm' })`; `reportFailure(context, toolName, outcome.reason, outcome.whisper)`; the handler's OWN log lines (`run_custom: state cascade unavailable…`, its refused line) are GONE — the run service's replace them (a ported capture pin on those texts WILL trip). `app/api/v1/chats/[id]/custom-tools/route.ts` −173 / +~40 likewise → `runCustomTool({ chat, entry, parameters: body.parameters ?? null, private: body.private, characterId: body.asCharacterId ? perspective.characterId : null, metadata: body.asCharacterId ? perspective.metadata : {}, whisperTargetIds: [ctx.user.id], invokedBy: 'user' })`; the route's `Manual custom-tool run: state cascade unavailable…` line GONE (the run service's `Custom tool run: state cascade unavailable…` replaces it), while its `Manual custom-tool run refused` DEBUG line SURVIVES on the `{ok:false}` arm (`{context: HANDLER, chatId, tool: body.tool, reason: outcome.reason}`) followed by the Prospero post + `badRequest(outcome.reason)`; `callerParticipantId: null` passed explicitly. The handler likewise keeps `reportFailure(context, toolName, outcome.reason, outcome.whisper)` on `{ok:false}` and a `catch` for rethrown non-`CustomToolRunError`s (`whisper = isPrivate ?? defaultVisibility === 'whisper'`). **(f) `pascalMeta.invokedBy` widens** `z.enum(['llm','user'])` → `['llm','user','trigger']` + `triggerPhrase: z.string().optional()` in BOTH `MessageEventSchema` (`chat.types.ts`) and `ChatMessageRowSchema` (`chats-messages.ops.ts`) — JSON-column only; `.qtap` export schema NOT touched by this commit (the dirt touches it for `effects`). ⚠ LIVE-DATA: a v5 at the baseline parsing `pascalMeta` with the two-value enum would DROP / refuse rows v4 4.11-dev writes with `'trigger'` (restore / import / read) — the §1 hazard rule (dogfood only a copy taken BEFORE v4 ran past this) already covers it; and v4's boot ALTERs live `chat_settings`. **(g) Settings plumbing — PORT:** `app/api/v1/settings/chat/route.ts` PUT accepts `customToolTriggers?: boolean` (non-boolean → `Error('Invalid customToolTriggers value (must be boolean)')`, the route's 400 arm; GET echoes it); `help-settings-handler.ts` `fetchCategorySettings` adds `customToolTriggers` to the chat bag (the `help_settings` tool output GROWS — `help_tools_equivalence` will trip at the pin, the P4.d6 `customTools` precedent exactly); Almanack `types.ts` `FeatureConfigInfo.customToolTriggers: boolean`, `phase3-ledgers.ts` default `false` + `chatSettings?.customToolTriggers ?? false`, `render.ts` +1 line `- **Auto-Run on Trigger Phrases**: ${yesNo(...)}` under "Pascal's Workbench (Custom Tools)" (snapshot +1). **(h) SPA (React → Angular):** NEW `components/custom-tools/TriggerPhrasesField.tsx` (110; chip list, `aria-label="Trigger phrases"`, `Remove trigger phrase ${phrase}`, placeholder `abracadabra` / `At most 8 phrases`, Enter adds); `BuilderForm.tsx` mounts it; `WorkbenchLibrary.tsx` badge `✦ N phrase(s)` (title "Listens for trigger phrases — said in a chat with the setting on, they run this tool") when `triggerCount > 0`; `AutomationSettings.tsx` rewritten as a row table over `AUTOMATION_OPTIONS` (`autoDetectRng`, `customToolTriggers` — the latter `disabled: !customToolsOn`, `opacity-50 cursor-not-allowed`); `types.ts` option `label: 'Auto-Run Custom Tools on Trigger Phrases'`, `description: 'When anyone in a chat says a phrase one of Pascal's custom tools declares, run that tool automatically with its defaults. Requires Custom Tools.'`, `DEFAULT_CUSTOM_TOOL_TRIGGERS = false`, `AutomationKey`; `useChatSettings.ts` `handleCustomToolTriggersChange` → `patchChatSettings({ customToolTriggers }, 'Failed to update trigger phrase setting' ×2)`; `ChatTabContent.tsx` wires it. **(i) Help / docs:** `help/custom-tools.md` +30 (NEW section "Words that pull the lever — `triggers`": match rule, who can set one off, what Pascal never listens to, how it rolls, the four-per-message cap, echoes, "It must be switched on"; the limits paragraph; the Custom tools toggle paragraph + the Automation toggle), `help/chat-settings.md` +2, `help/pascals-workbench.md` +1, `help/rng-tool.md` +2; `API.md` (PUT field + the three-entrances paragraph on `?action=run`), `FEATURES.md`, `features/complete/pascal-custom-tools.md` (+item 16, the "Correction (4.11) — three entrances, one run service"), the spec moved to `complete/` with an As-built section. **(j) Tests-only / NO-PORT:** `registry.test.ts` +24 — an acyclic-dependency guard ("`add-custom-tool-triggers-field-v1` once named itself in `dependsOn`" — a dev-time slip, fixed before the commit); the `prettify` label. | **Pascal server family (P4.6ay units 1 / 3 / 4–9 / 12, P4.d10 unit 5, P4.6be unit 1, P4.d19, P4.D30, P4.D169):** `crates/quilltap-core/src/pascal/custom_tool_types.rs` (+ the SPA twin `apps/web/src/app/pascal/custom-tool-types.ts`; `pascal_custom_tool_definition_equivalence`), `pascal/custom_tools.rs` + `roster.rs`, `tools/run_custom.rs` (`pascal_run_custom_handler_equivalence`, `pascal_run_custom_equivalence` — the refactor moves v4's log lines and failure arms), `api/custom_tools.rs` + `quilltap-web/src/custom_tools_routes.rs` (`pascal_custom_tools_route_equivalence`), `pascal/side_effects.rs` / the Pascal + Prospero writers (`pascal_side_effects_equivalence`, `pascal_writers_equivalence`), `pascal/workbench.rs` (`pascal_workbench_equivalence` — `triggerCount` is a NEW wire key), `apps/web/public/schemas/qtap-custom-tool.schema.json` (byte-copy). **The chat spine (Phase 3 / P4.2 M2; `message_finalizer_tier3_equivalence`):** `services/orchestrator.rs`, `services/message_finalizer.rs` (two NEW call sites next to `rng_patterns.rs` and the Carina paths), the Event channel's `pascalResult` event (SSE + the Tauri pump). **Carina (P4.6ay-round Carina units; `carina_parser_equivalence`, SPA fixture `carina-parser.ndjson`):** `carina_parser.rs` gains `strip_carina_query_lines`. **Chat settings (P4.d6 cross-lane finding — the `customTools` precedent, 2026-07-17; P4.6an unit 3; the Settings vertical P4.6e):** `db/chat_settings.rs`, `api/settings.rs` (`bool_field` sibling), `services/provisioning/fresh_schema.json` + `chat_settings_seed.json` (D23), a NEW `db/chat_settings_custom_tool_triggers_repair.rs`-shaped ensure + `restore_vintage_state`, `tools/help.rs` `help_settings` (`help_tools_equivalence`), `almanack/phase3_ledgers.rs` + render (P4.37 unit 12, the `almanack-*` family), `apps/web/src/app/screens/settings/chat/automation-settings.ts`. **`pascalMeta` row parse** (`db/chats_messages*`, the `.qtap` import / export / backup carriers' enum). **Workbench SPA (P4.6bb, P4.D36 unit 4):** `apps/web/src/app/screens/custom-tools/builder-form.ts`, `workbench-library.ts`, a NEW trigger-phrases field. **`help/` 131 → 131 edited (`p4.9i2`)** + `docs/v4/` (API.md, DDL.md, FEATURES, features/complete). CLI Tier R: untouched (stamp only). | **UNPROCESSED — past the `83334b9a5` round's pin** |

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

- **The `4a38222e6` wardrobe-programme drift catch-up round (2026-10-13,
  baseline `01a83539d` → `4a38222e6`; P4.D279 → {P4.D280 ∥ P4.D281 ∥
  P4.D282 ∥ P4.D283 ∥ P4.D284} ∥ P4.D285 ∥ P4.D286 ∥ P4.D287):** the eight
  rows retired — `3e6e2f590` (the wardrobe refactor plan + bugs 187–197
  filed), `0841000c7` (the scenarios-from-anywhere + wardrobe-folders
  specs), `c19e5d0a9` (the memory system refactor spec) and `0fc8ce709`
  (the four audit reports appended to it) NO-PORT-RATIFIED(P4.D287 —
  docs-only, the file lists verified at the pin); `7a8d6dacc` (bugs
  187/188/189/191 — cross-tier component refs, the idempotent archive tool,
  the chat Concierge for tool pictures, no wearing archived items)
  ABSORBED(P4.D279, P4.D280, P4.D282, P4.D283, P4.D285, P4.D287);
  `9ecebf986` (the wardrobe refactor — one location, one wearable pool, one
  outfit pipeline, one route factory; bugs 190 / 192–197) ABSORBED(P4.D279,
  P4.D280, P4.D281, P4.D282, P4.D283, P4.D284, P4.D285, P4.D286, P4.D287)
  with bug 197's `analyze-image` half DEFERRED by name (R-L — the route
  stays a loud refusal); `e23fb095b` (bug 198, the stale cached outfit)
  ABSORBED(P4.D285); `4a38222e6` (wardrobe folders — items filed into a
  tier's closet, no DDL; v5 now reads `Wardrobe/<sub>/*.md` recursively,
  retiring the wardrobe-folders live-data hazard) ABSORBED(P4.D279, P4.D280,
  P4.D281, P4.D284, P4.D285, P4.D286, P4.D287). `97f0d8cb8` (bug 229, the
  folder picker) was PORTED(P4.D285) ahead of the baseline (R-P — the picker
  landed WITH the bug-229 fix) and stays in §3 until a later round passes
  it. No D23 move. ELEVEN commits stood past the pin during the round — the
  scenarios (`545e299bb`), memory Phase-0 (`124c8aca3`), bug 227
  (`69d38dd60`), bug 228 (`8050958be`), bug 229 (`97f0d8cb8`) and merge
  (`4ce64fa1e`) rows already in §3 at round start, plus the five
  memory-refactor commits `c2795733f` … `ac6348c5f` the same-day
  `/driftcheck` `732eebcd7` classified (nine migrations, three destructive)
  — all left in §3; every regen, the whole sweep and every live-checkout
  guard ran from the pin `/tmp/qt-v4-pin-unify-4a38222e6` with its own
  `npm ci --offline` tree. Round record: `status-log.md` → "The `4a38222e6`
  wardrobe-programme drift catch-up round — UNIFICATION record".
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
