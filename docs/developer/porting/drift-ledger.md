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

- **Oracle baseline: `f5e953a3f`**: "feat(startup): daily database optimize
  before migrations" (v4 main, 2026-10-07 23:28, `4.10.0-dev.117`), adopted
  when the `f5e953a3f` wardrobe-programme + convergence drift catch-up round
  was unified, all ten lanes (P4.D255 → {P4.D256 ∥ P4.D262 ∥ P4.D263 ∥
  P4.D264} ∥ P4.D257 ∥ P4.D258 ∥ P4.D259 ∥ P4.D260 ∥ P4.D261, 2026-10-08).
  The eleven rows it absorbed (`938144eb4` … `f5e953a3f`) are retired to §6.
  CLAUDE.md's Status bullet agrees (the round's bullet: "the oracle baseline
  MOVES to `f5e953a3f`"; its "Oracle baseline" bullet names it).
- **Checked:** 2026-10-09 (`/driftcheck` — after `git fetch`, the §2 probe
  PASSED whole: `783385873..main`, `..origin/main`, `1a2b2164c..bugfix`,
  `..origin/bugfix` all empty, `release` still `8fbf2afe0`, checkout `main`
  CLEAN; no new commit to classify, §3's nine rows stand as recorded;
  CLAUDE.md's Status agrees). Previous checks: 2026-10-08 (`/unify` of the
  `f5e953a3f` round; the eight commits past the target were classified the
  same evening by a `/driftcheck` run from the unification session — its
  draft rows are §3's eight); 2026-10-08 morning (`/driftcheck`, the eleven commits to
  `f5e953a3f`); 2026-10-07 (`/unify` of the boot-hardness round).
- **v4 `main` HEAD at check: `783385873`** — local `main` is now `3c56a41e7` (+2, this port's docs-only bug filings `5abcd01ea` + `3c56a41e7`, unpushed; §3's two NO-PORT rows — a §2 probe should expect `783385873..main` = exactly those two commits until `/driftcheck` re-anchors); at check: ("docs: update 4.10.0 release
  notes for memory consolidation, wardrobe ledger and pictures, daily
  optimize", 2026-10-09 00:36) — NINE commits past the baseline
  (`1825bfd53`, `3f7320138`, `da98ca58b`, `f7d8064be`, `d58548051`,
  `70f9b495c`, `197104649`, `7e9eaf42c`, `783385873`; a linear run, no
  merges). `origin/main` agrees. All nine arrived DURING the round (after the
  lanes' `1825bfd53`-only waiver, which the unification superseded) — the
  ninth, docs-only, during the unification's gate, recorded at its cleanup;
  the round's regens all ran from the `f5e953a3f` pin.
- **v4 `bugfix` tip at check:** `1a2b2164c`, UNMOVED (`1a2b2164c..bugfix`
  and `..origin/bugfix` empty). **`release` tip:** `8fbf2afe0` (`release:
  4.9.2`), UNMOVED; still no `release: 4.10.0` squash.
- **Checkout at check:** branch **`main`**, tree **CLEAN**.
- **Verdict: DRIFT PENDING — 9 commits (§3), all classified, all
  `UNPROCESSED`** — v4's **memory programme** (2026-10-08) plus a sync log
  line, a test commit and a release-notes update:
  - **NO-PORT?** (4): `1825bfd53` + `da98ca58b` (docs/specs only),
    `7e9eaf42c` (tests + a behaviour-preserving SPA refactor v5 already
    matches), `783385873` (`docs/releases/4.10.0.md` only — but the
    `docs/v4/releases/4.10.0.md` mirror P4.D260 vendored is now stale: a
    re-vendor rides the next round).
  - **PORT / PORT-NEW** (5): `3f7320138` (F1–F9 — near-duplicate
    reinforcement, the footnote cap, archive ranking, the per-chat frozen
    archive, delivered-only access stamps, budget-sized head/archive, real
    merges; PORT-NEW the anchor probe + `quilltap anchor-probe`);
    `f7d8064be` (recall multiplier retuning — the fresh boosts 1.6/1.35 →
    1.3/1.15, the relevance gate + the 1.4 boost cap, specific entity
    anchors, the recall-replay `tuning`/`signals`/`asOf`); `d58548051`
    (hot/cold tiers + consolidation — SCHEMA MOVE; housekeeping demotes
    instead of deleting, recall reads hot rows only, the OTHER extraction
    pass defaults to `hybrid`); `70f9b495c` (bug 182 — the atomic
    reinforcement increment; v5's `reinforce_memory` carries the bug's
    lost-update shape); `197104649` (one sync debug line).
  - **CONVERGENCE: none.** Bug 182 was found by v4's own review of PR 83,
    not by this port; `bugs.md` reads "Bugs 1–182 are fixed in v4; none is
    open".
  - The memory rows interlock: `f7d8064be` sits on `3f7320138`'s
    budget-sized head, `d58548051` RETIRES the housekeeping half of
    `3f7320138`'s F9 merge, and `70f9b495c` rewrites F1's reinforcement
    write — **order them as ONE memory catch-up, never commit-by-commit.**
  - **Schema: ONE D23 move pending** — the SEVENTH re-dump carries
    `memories.tier` / `supersededById` / `consolidatedFrom` /
    `consolidatedAt` and `chats.otherExtractionWatermarkMessageId`
    (`d58548051`), plus the migration-only index
    `idx_memories_character_tier` for `migration_indexes.json` and a boot
    ensure re-homing `add-memory-tiers-v1` (append-ADD shape + the NULL-tier
    heal; generateDDL's vs the migration's column shape predicted to differ,
    NOT measured). Two new `instance_settings` keys
    (`memoryConsolidation`, `memoryExtractionMode`) and the new job type
    `MEMORY_CONSOLIDATION` need no DDL.
  - **Dependencies: NONE moved** by the eight (version stamps only;
    `plugins/dist/` untouched).
- **Regen rule: PIN REQUIRED at `f5e953a3f`** (§5.1) — v4 HEAD is nine
  commits past the baseline. ⚠ **The dependency trap:** the root tree moved
  at `a9c99a4a0` (now INSIDE the baseline — `openai` 7.30.0,
  `@openrouter/sdk` 1.4.25, …); a `f5e953a3f` pin's own lockfile carries it,
  and the eight later commits move nothing, so the root `node_modules`
  symlink and a fresh `npm ci --offline` tree agree today — still build the
  pin's own tree (§5.1), as the unification did. `provisioning_equivalence`
  REQUIRES `QT_FRESH_SCHEMA_LIVE` dumped FROM THE PIN — a HEAD dump carries
  the memory-tier columns (drift, not a v5 bug).
- **Live-checkout guards at HEAD (predicted, not run):** the eight commits
  ALONE redden `qtap_schema_embed_guard` + the `generators/qtap_schema.rs`
  size pin (`d58548051` moves `qtap-export.schema.json` again); the help
  family on a HEAD regen (`help_tree_equivalence` — 130 → 131 with
  `help/memory-consolidation.md`, plus five modified pages); Tier R
  (`cli_differential`) where it runs v4's CLI from the LIVE checkout (the
  top-level help's `anchor-probe`, the `recall-replay` help/flags, all three
  `completion` scripts). **Predicted GREEN:** `provider_sdk_version_guard`
  (the `a9c99a4a0` move is inside the baseline and P4.D260 re-recorded it),
  `zod_version_guard`, `public_schemas_vendor_guard`,
  `builtin_prompt_templates_guard`. All green against the pin at the
  unification.
- **The workspace gate at the baseline:** the round record in
  `status-log.md` has the counts.
- **Schema state (`f5e953a3f`):** `fresh_schema.json` +
  `chat_settings_seed.json` are the SIXTH D23 re-dump (P4.D255, from the
  `f5e953a3f` pin — exactly three lines: `chat_settings."wardrobeImageSettings"`
  TEXT in v4's schema order, the NEW `wardrobe_wear_stats` table in
  generateDDL's text (`"wearCount" REAL NOT NULL`) and its auto `createdAt`
  index; the seed row's `wardrobeImageSettings` =
  `{"imageProfileId":null,"generateFromTools":false}`).
  `migration_indexes.json` re-dumped through v4's REAL `MigrationRunner` (+
  `idx_wardrobe_wear_stats_item_wearer` UNIQUE on `("itemId",
  COALESCE("wearerCharacterId", ''))` — the `ON CONFLICT` target the
  increment SQL needs — and `idx_wardrobe_wear_stats_wearer`). **There is NO
  `migration_tables.json`:** the planned R-A artefact was OVERRULED by the
  human mid-lane (a real v4 first boot carries 40 of 45 tables in migration
  text that differs from generateDDL, so the "exactly one table" premise was
  false); a fresh v5 instance keeps generateDDL's `wardrobe_wear_stats` and
  gets both hand indexes from the index family. v4's three migrations are
  re-homed as boot ensures (`db::wardrobe_wear_stats_repair` — the table +
  the ONE-transaction seed, STAMPING both `migrations_state` rows and
  skipping on v4-written ones; `db::chat_settings_wardrobe_image_settings_
  repair` — v4's one-key ADD default, no stamp), each differentially
  compared against v4's real migrations. Earlier state (the `07b8f0209`
  FIFTH re-dump — `impersonationVoiceMode` replacing the retired toggle,
  its boot ensure, the deleted P4.D179 ensure) unchanged.
- **Real-instance note:** NOT re-measured at this unification (the dogfood
  copy was not opened). Predicted for the next `/dogfood`: unless v4 has
  already booted the copy past `7c8572869`, the first v5 boot CREATES
  `wardrobe_wear_stats` (generateDDL's shape — or finds v4's migration
  shape), SEEDS it and STAMPS both rows, and ADDs `wardrobeImageSettings`;
  the PHASE 0.75 daily optimize stamps `data/db-optimize-state.json` and the
  startup trio lands in `data/backups/` (now JOINED before the pumps — the
  unification's §3 fix). The `52d6e7ecd`/`07b8f0209` notes (the
  `chat_informs.permanent` ALTER, the voice-mode translation) stand.
- **`help/**`:** whole at `f5e953a3f` (130 files, md5-identical — P4.D260;
  `help/wardrobe-images.md` NEW). **`docs/v4/`:** CURRENT at `f5e953a3f`
  (P4.D260's twelve paths, incl. three `bugs/fixed/` files, the three
  `features/complete/` specs and `releases/4.10.0.md`); residual only
  `packages-quilltap-README.md`. `qtap-export.schema.json` re-vendored at
  `f5e953a3f` (P4.D264, 101,092 bytes).
- **Standing deferrals unchanged:** the three text-compression migrations,
  the image re-encode migration and the stored-`renderedMarkdown`
  reclamation stay DEFERRED as reclamation; the animated-input ruling is
  LANDED (P4.108); the corrupt-second-frame ruling keeps v5's still. The
  ledger-gate divergence STANDS (v5 keeps its per-boot ensures, R5) beside
  the ported structural check. The production pdf/docx
  `DocumentTextExtractor` stays DEFERRED. `STAT4_NOT_COMPILED` RULED
  (P4.D259, leave the sys crate): v4 builds SQLite with `ENABLE_STAT4`, v5
  does not, so v5's daily optimize empties `sqlite_stat4` on a shared
  instance until v4's next pass.

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
| `1825bfd53` | 2026-10-08 | docs: memory improvement specs | NO-PORT? (docs only — re-confirmed 2026-10-08 evening by `--stat`: two NEW `docs/developer/features/{memory-consolidation-and-tiers,memory-recall-and-housekeeping-fixes}.md` specs, 664 insertions, no code, version stamp unchanged at `4.10.0-dev.117`; landed DURING the `f5e953a3f` round's planning and waived for that round's probe by the human — the §1 amendment, now SUPERSEDED. The two specs are the planning inputs for `3f7320138` (F1–F9) and `d58548051` (tiers + consolidation); both were AMENDED by later commits — read the post-`d58548051` copies, whose "Implementation notes" record what shipped) | — (planning input for the memory rows below) | UNPROCESSED |
| `3f7320138` | 2026-10-08 | Implement memory recall and housekeeping fixes (F1–F9) (#83) | **PORT + PORT-NEW** (classified 2026-10-08 from the hunks; NO schema / DDL / migration / index move — F4 rides the existing `reinforcedImportance` index; files bug 182 OPEN, fixed in `70f9b495c`). **F1** `absorbNearDuplicate`: a `SKIP_NEAR_DUPLICATE` (≥ 0.90) now WRITES count+1 / `lastReinforcedAt` / recomputed `reinforcedImportance` (text + vector untouched, action unchanged; debug `[MemoryGate] Near-duplicate absorbed as reinforcement`, WARN `… Failed to reinforce near-duplicate memory`). **F3** `MAX_REINFORCEMENT_FOOTNOTES = 8` (`appendCappedFootnotes`; debug `Reinforcement footnote cap reached; dropping extra details`); a row at the cap is never re-embedded even when anchors fill; `anchorsChanged` only when the truncated entity union actually grew; `reembedMemory` factored out (WARN `Failed to re-embed memory`). **F4** `findMostImportant` → raw SQL `ORDER BY reinforcedImportance DESC, COALESCE(lastReinforcedAt, createdAt) DESC, id ASC` (`[]` on limit ≤ 0). **F5** frozen-archive cache keyed `(characterId, chatId)`, freshness = generation AND size, LRU 64; `invalidateFrozenArchive` (all chats of the character) now CALLED by housekeeping + dedup, new `invalidateAllFrozenArchives` + a dispatcher completion hook for `MEMORY_HOUSEKEEPING` (v5: direct calls suffice). **F6** `searchMemoriesSemantic` + the text fallback STOP stamping `lastAccessedAt`; `markMemoriesAccessed` stamps only what was delivered — context-manager (formatted archive / head / inter-character entries), the scriptorium `search` tool (post-`limit`), first-message context, Carina recall, voice-rewrite `recallForSeed`; the Memories API search and recall-replay stamp NOTHING (the spec's "API already bumps its own" is false — §5.3); debug memory infos gain `memoryId`. **F7** `sizeMemoryPools(memoryBudget, retro)`: head tokens clamp(15 %, 200, 1200), entries clamp(round(t/40), 5, 15), retro max(600, 2×) / max(10, 2×), archive clamp(round((budget − head)/60), 25, 60) — **at the 2,000-token floor an 8-entry / 300-token head and a 28-entry archive, so EVERY turn's context moves**; search limit = entries × 3; debug `[ContextManager] Memory pools sized`; pre-compute `PROACTIVE_RECALL_POOL_SIZE = 90` (was 20) and no `.slice(0,10)`; recall-replay body `memoryBudget`, response `oldHeadSize` / `newHeadSize`, CLI `--memory-budget`. **F8** a child-only NaN deleted-count guard — NO-PORT (v5's `delete_many_with_unlink` returns a real count). **F9** NEW `lib/memory/memory-merge.ts` (`planMemoryMerge` pure — tier-1 portable; `applyMemoryMerge`): housekeeping pass 2 and dedup FOLD losers into the survivor (capped footnotes, summed counts, recomputed importance, unioned links minus the group, earliest `occurredAt`, latest `lastReinforcedAt`, re-embed) before deleting; a failed fold keeps its losers (WARN `Some merges failed…`); the cap pass skips merge survivors; dedup takes a `userId`, drops the explicit `updatedAt`; a `currentLinks` scrub override. **F2 PORT-NEW** (measurement only — the gate is unchanged): `lib/memory/anchor-gate-probe.ts`, `POST /api/v1/memories?action=anchor-gate-probe` (`{characterId, limit 1–200}`; 404 `Character`, 500 `Anchor gate probe failed`), top-level CLI `quilltap anchor-probe <characterId>` (v4's `SUBCOMMANDS` gains it after `recall-replay`; top-level help + all three completion templates move). `patchMemory`'s buffered-`undefined` arm is job-child plumbing (NO-PORT). Help `memory-housekeeping.md`, `memory-recall-relevance.md`. ⚠ `mergeSimilar` is RETIRED again by `d58548051` one commit later — order the two together | `services/memory_gate.rs` (`SkipNearDuplicate` arm ~:276 writes nothing today; `reinforce_memory` :657 — uncapped footnotes, the entity `anchors_changed` over-report), `services/memory_processor.rs`, `db/memories_read.rs:525` `find_most_important` (still `ORDER BY importance DESC`), `services/frozen_archive.rs` (`get_or_compute_frozen_archive` :57 character-keyed; `invalidate_frozen_archive` :91 has NO production caller) + `services/build_context.rs` (:2455 caller; head/archive :2632–2740; inter-char :2865), `services/memory_service.rs` `bump_access_times` (:430/:444), `tools/search.rs:565`, `services/first_message_context.rs`, `services/carina_query.rs`, `services/announcer/voice_rewrite_core.rs`, `memory_injector.rs` (:73–83 constants; `memory_id: None` at :655/:921), `services/pre_compute.rs` (:393 `Some(20)`, :440 `take(10)`), `services/housekeeping.rs` (`merge_pairs` :303–530 just deletes) + `services/memory_dedup.rs` + `api/memory_maintenance.rs` (dedup has no embedding provider), `db/memories.rs` scrub, `services/recall_replay.rs` + `api/recall_replay.rs` + `api/types.rs` `ChatRecallReplay`, `api/memories.rs` (F2 action), `quilltap-cli/src/{main.rs SUBCOMMANDS, recall_replay_cmd.rs, help/}`; rounds P4.d5 ∥ P4.d6 ∥ P4.6ay (gate / housekeeping / frozen archive), P4.d12 unit 6, P4.d13 unit 7/8, P4.19, P4.43 unit 6. Reddens (predicted): `memory_gate_tier3_equivalence`, `memories_read_equivalence`, `build_context_tier3_equivalence`, `precompute_equivalence`, `recall_replay_equivalence`, `search_tools_equivalence`, `first_message_context_equivalence`, `memory_dedup_equivalence`, `memory_housekeeping_tier2_equivalence`, the orchestrator / enclave / announcer / Carina tier-3 families where they seed memories | UNPROCESSED |
| `da98ca58b` | 2026-10-08 | docs(memory): F2 anchor-probe result + recall multiplier retuning spec (#84) | NO-PORT? (docs only — re-confirmed by `--stat`: `docs/CHANGELOG.md`, `.claude/commands/update-documentation.md`, `memory-recall-and-housekeeping-fixes.md` (the F2 result: the gate unchanged), NEW `recall-multiplier-retuning.md` (the spec `f7d8064be` implements); no code, no version bump) | — (the spec is the planning input for `f7d8064be`'s row) | UNPROCESSED |
| `f7d8064be` | 2026-10-08 | feat(memory): recall multiplier retuning — probe set, R7 harness, cap14 defaults | **PORT + PORT-NEW** (classified 2026-10-08 from the hunks; NO schema move — the only new zod schemas are request validators). **Live recall ranking moves for every injected memory:** `RECALL_MULTIPLIERS.freshEvent24h` 1.6 → **1.3**, `freshEvent48h` 1.35 → **1.15**; new `RECALL_TUNING_DEFAULTS` (`boostGate {abs 0.45, margin 0.15, ramp 0.1}`, `boostCap 1.4`, `freshBypassesGate`/`windowBypassesGate` false, `specificAnchors` true, `anchorMinHits 1`, `anchorOrder 'rarest'`, `backgroundReserve 0`; `MULTIPLIER_CLAMP` unchanged); `combineRecallMultipliers(memory, ctx, relevance?)` splits boost (>1) from penalty (≤1) factors and caps / relevance-gates the boost (new `fired` labels `cap1.4`, `gate×0.NN`; gate only when `relevance` is passed — both memory-service sites pass it); `searchMemoriesSemantic` captures raw cosine before the literal boost (`bestCosine`), turns the gate OFF for the `BUILTIN` (TF-IDF) provider when no explicit tuning (debug `[Memory] TF-IDF embeddings: recall boost gate off`), chooses entity anchors by specificity (R4 — `searchByContent` hit counts, drops present participants' names, rarest first, 3; debug `[Memory] Specific entity anchors chosen`) instead of the first three, `expandRelatedMemories` gates neighbours on their own cosine + takes a weight clock, new options `excludeMemoryIds` / `headSize` / `weightClockMs` / `embeddingMemo`; R6 `reserveBackgroundSlots` ships INERT (reserve 0; debug `[Memory] Background reservation (R6)`). `buildContext` / orchestrator / pre-compute thread `presentParticipantNames` into `buildTurnRecallContext`. No new settings or env knobs — tuning is reachable only through the replay body. **recall-replay:** body gains strict `tuning` (`RecallTuningInputSchema`, unknown keys refused → `validationError`), `signals`, `asOf` (boolean else 400 `asOf must be a boolean`); response gains `tuning` (`'defaults'`), `signalsPinned`, `asOf`, `excludedAfterAsOf`; default `limit` `max(25, newHeadSize+10)`; a process-lifetime 500-entry embedding memo; new debug lines + `Recall replay complete` INFO gains fields; CLI `--tuning` / `--tuning-file` / `--signals-from` / `--as-of`, `Harness` / `Tuning` header lines, completions. **⚠ Prose misleads (§5.3):** the replay's OLD path ALSO runs under the cap14 defaults (no tuning → `recallTuningOf` falls back), so the retained "byte-identical to pre-overhaul recall" / "Absent → today's constants" comments are now false; "R5 … pinned" is test-only (no R5 hunk). **NEW pure module** `lib/memory/recall-tuning.ts` (tier-1 portable). Help `memory-recall-relevance.md`, `episodic-memory.md`. Sits ON `3f7320138`'s budget-sized head (`memoryBudget`) — order after it | `recall_tags.rs` (`FRESH_EVENT_24H`/`48H` :174–175, `combine_recall_multipliers` :591, `RecallContext`; P4.d13 unit 2, P4.d26 unit 2), NEW `recall_tuning.rs`, `services/memory_service.rs` (`search_memories_semantic` :399 incl. the `take(3)` anchor loop :595, `apply_recall_multipliers`, `expand_related_memories` :889, `RecallContextInput`; P4.d13 unit 3), `db/vector_store.rs` `search` (:148 — NO filter param today; `excludeMemoryIds` needs one), `services/build_context.rs` / `pre_compute.rs` / `orchestrator.rs` (participant names), `services/recall_replay.rs` + `api/recall_replay.rs` + `api/types.rs` `Request::ChatRecallReplay` (P4.d13 unit 7), `quilltap-cli/src/recall_replay_cmd.rs` + the vendored completion templates (P4.d13 unit 8; `completion_behavior.rs`); reddens `recall_tags_equivalence`, `recall_replay_equivalence`, likely `precompute_equivalence` / `build_context_tier3_equivalence` / `memory_injector_equivalence` | UNPROCESSED |
| `d58548051` | 2026-10-08 | feat(memory): consolidation and hot/cold tiers | **mixed — PORT-NEW + PORT, SCHEMA MOVE (D23)** (classified 2026-10-08 from the hunks; 123 files, ~10k insertions). **Schema:** `MemorySchema` gains `tier` (`'hot'\|'cold'`, default `'hot'`), `supersededById` (uuid, nullable), `consolidatedFrom` (uuid[], default `[]`), `consolidatedAt` (timestamp, nullable); `MemorySourceEnum` gains `'CONSOLIDATED'`; `ChatMetadataSchema` / `…BaseSchema` gain `otherExtractionWatermarkMessageId` (string, nullable) → **the SEVENTH D23 re-dump** of `fresh_schema.json` (`memories` +4 columns, `chats` +1 — generateDDL renders them in schema order, the chat column likely bare `TEXT`, where migration **`add-memory-tiers-v1`** APPENDS `tier TEXT DEFAULT 'hot'`, `supersededById TEXT DEFAULT NULL`, `consolidatedFrom TEXT DEFAULT '[]'`, `consolidatedAt TEXT DEFAULT NULL`, `chats."otherExtractionWatermarkMessageId" TEXT DEFAULT NULL` — TWO shapes, as the inform `permanent` case; measure, don't assume), a heal `UPDATE memories SET tier = COALESCE(tier,'hot'), consolidatedFrom = COALESCE(consolidatedFrom,'[]')` + the migration-only index **`idx_memories_character_tier ON memories ("characterId","tier")`** → the `migration_indexes.json` re-dump (generateDDL does not create it) + a boot ensure (four INFO lines + `Stamped NULL-tier memories hot`). New job type `MEMORY_CONSOLIDATION` (zod enum; `jobs.type` is TEXT — no DDL). Two NEW instance-settings keys (no DDL): `instance_settings['memoryConsolidation']` (`enabled` false, `connectionProfileId` null, `clusterThreshold` 0.72, `minClusterSize` 3, `maxClusterSize` 30, `matureAfterDays` 7, `maxClustersPerRun` 40, `watermark` 150, `coldRetentionDays` null) and `['memoryExtractionMode']` (`otherPass` `'hybrid'` DEFAULT, `perTurnOtherFloor` 0.75, `foldCandidatesPerSubject` 3). **PORT on ported surfaces — behaviour moves with the feature OFF:** (1) **the OTHER extraction pass defaults to `hybrid`** — per-turn OTHER candidates below 0.75 importance are DROPPED on every turn (`memory-processor.ts`; debug `[Memory] OTHER pass mode resolved`, WARN on lookup failure), the rest move to fold grain (context-summary folds + a daily idle-chat catch-up `fold-other-catchup.ts` + a regenerate rebuild) advancing the chat watermark — the fold hook `runFoldOtherPass` after every context-summary fold is a NEW cheap-LLM call per observer (ERROR `[Context Summary] Fold OTHER pass failed:` on failure), so the tier-3 fold request sequences move too; (2) **housekeeping DEMOTES to cold instead of deleting** — the cap counts HOT rows only, `mergeSimilar` RETIRED (debug `[Housekeeping] mergeSimilar is retired…` — undoing `3f7320138`'s F9 fold in housekeeping), retention-flagged AND cap-pressure rows both go cold (`updateTierBulk` + the vector store's tier stamp), reason `Exceeded memory limit (N)` → `Exceeded hot-memory limit (N)`, new `demoted` / `coldCount` / `demotedIds` result fields (the Memories API housekeep + preview echo them, `wouldDemote`; `merged` always 0), the outcome cache records `demoted + deleted`, deletion only by the opt-in `coldRetentionDays` sweep of superseded cold AUTO rows; MANUAL and CONSOLIDATED rows never demoted; (3) **recall reads HOT rows only** (the repository's `COALESCE(tier,'hot')='hot'` predicate in `findMostImportant` / `findRecentByImportanceTier` / `findByCharacterAboutCharacters`; `searchMemoriesSemantic` gains `includeCold` default false — the search tool and the API search pass `true`; the character optimizer is hot-only), the frozen archive composes digests first (3 per present character, 5 self) and keys on presence, inter-character recall prepends `findHotDigests(other, 2)`, recap and first-message context put digests first, digests lead (`digestsFirst`) and carry the model-visible suffix ` (from N notes)` (`formatDigestProvenance`); the gate still reads cold rows — a match on a superseded cold row reinforces/links the digest, a re-observed demoted row is PROMOTED; (4) self-canon now reads `Commonplace/<Subject>.md` from the character's vault (`loadCanonForSelfWithCommonplace`) — model-visible extraction prompt input; (5) the scriptorium `search` tool labels cold rows `(archived — superseded by <id>)` / `(archived)` and accepts `CONSOLIDATED`; (6) backup/restore `uuid-remap` remaps `supersededById` + `consolidatedFrom` + `chats.otherExtractionWatermarkMessageId`; **`.qtap` import now REMAPS `relatedMemoryIds`** (it did not before) and mints every memory id up front; `qtap-export.schema.json` gains the five fields + a `source` description; (7) the memories list API gains `?tier=` + `source=CONSOLIDATED`; the Brahma SQL prompt's `memories` paragraph names the new columns (model-visible); the activity-kind title becomes `Memory work (extraction, regeneration, consolidation, housekeeping)`; `queue-service`'s extraction dedupe compares `foldOtherCatchup`; scheduled housekeeping runs consolidation first and defers 30 min when it enqueued; scheduled maintenance gains the `fold-other-catchup` sweep (idle > 2 h, < 30 d, cap 50) and a summary key; the SPA housekeeping dialog drops the merge checkbox and says Archive instead of Delete. Every memory row/JSON grows four keys (`tier: 'hot'`, `consolidatedFrom: []` written on create). `memory-regenerate-chat.ts`'s watermark reset lands on a handler v5 has not registered. The scriptorium tool's DESCRIPTION is unchanged (types only). **PORT-NEW:** the consolidation engine (`consolidation.ts` 1,067 lines, `-clustering`, `-plan`, `-triggers`, `cheap-llm-tasks/consolidation-tasks.ts` — a new cheap-LLM prompt family), the `MEMORY_CONSOLIDATION` handler + host-RPC + job topics, daily + watermark triggers (OFF by default), the Commonplace digest vault bridge + `commonplace-file.ts`, `fold-other-pass.ts` (634 lines), `vector-store.ts` additions; Memories API `POST ?action=consolidate` (dryRun in-process report, else `{jobId}`), `GET/POST ?action=consolidation-config` / `extraction-mode-config`; SPA Memory-tab cards (consolidation, report dialog, extraction grain), memory-list tier filters, card/editor; CLI `quilltap memories consolidate` + `--tier` (v5's `memories` verb is still "recognized but not yet available" — only the completion templates are ported). Help `memory-consolidation.md` NEW (help tree 130 → 131), `cli-memories.md`, `memory-housekeeping.md`, `memory-recall-relevance.md`, `memory-regenerate.md`. **Walk-measured 2026-10-09 (`/dogfood`, the Friday copy — `dogfood-walks/2026-10-09-wardrobe-programme-optimize-degraded-boot-pass.md`):** v4 has run `add-memory-tiers-v1` (2026-10-08 22:46Z) and its consolidation job (2026-10-09 04:19–05:01Z) on the LIVE instance — 7,932 `cold` rows and 365 `source = 'CONSOLIDATED'` digests already exist. Against them v5 at the baseline (a) REFUSES all 365 digests on a backup → `replace` restore under P4.161's baseline `MemorySchema` (`Failed to restore memory: … expected one of "AUTO"\|"MANUAL"`, 36,562 of 36,927 written) and (b) HARD-DELETES under housekeeping what v4 now demotes (one post-turn `MEMORY_HOUSEKEEPING` deleted 189 of one character's memories). **No v5 build should run against the live instance until this row lands** | `fresh_schema.json` + `migration_indexes.json` (D23), a new `db::` boot ensure beside the P4.D251/P4.D249 ones, `db/memories*.rs` (row type + every hot-only read + `MemorySchema` validation on import/restore — P4.161's ported schema), `db/chats*.rs`, `services/housekeeping.rs` (P4.d5 ∥ P4.d6 ∥ P4.6ay), `services/memory_gate.rs` + `memory_gate.rs`, `services/memory_processor.rs` + `services/carina_memory_extraction.rs` + `canon.rs` (P4.6bj), `services/context_summary*` + `services/fold_episode_pass.rs` (the fold hook), `services/frozen_archive.rs`, `services/build_context.rs` + `memory_injector.rs`, `services/memory_recap/`, `services/memory_service.rs`, `tools/search.rs`, `services/queue_service.rs` + `services/activity_kinds.rs` + `services/scheduled_maintenance.rs` + `realtime/job_topics.rs`, `services/backup/{collect,uuid_remap}.rs` + `restore/`, `services/quilltap_import/` + `services/qtap_export/` + the vendored `qtap-export.schema.json`, `api/memories.rs` (`memory_list` / `memory_search` / `memory_housekeep_preview` / `memory_housekeep`; P4.6s) + `api/memory_maintenance.rs`, `services/brahma_console/prompt_text.rs`, `generators/optimizer.rs`, `services/memory_extraction_job.rs`, `quilltap-host/src/spine.rs` (the housekeeping outcome record), `services/activity_kinds.rs` + SPA `layout/activity-kinds.ts`, the instance-settings accessors, `quilltap-cli/src/help/completion/*`; SPA `apps/web/src/app/memory/{memory-card,memory-list,housekeeping-dialog}.ts` + `screens/settings/memory/` (P4.6t). Reddens (predicted): `memory_housekeeping_tier2_equivalence`, `memory_watermark_tier3_equivalence`, `memory_processor_tier3_equivalence` + `memory_pipeline_jobs_tier3_equivalence` (any OTHER candidate under 0.75), `context_summary_service_tier3_equivalence` + `fold_episode_tier3_equivalence` + `orchestrator_tier3` / `build_context_tier3` (the fold OTHER call), `memories_read_equivalence` / `memories_tier2_equivalence` / `memories_routes_equivalence` (four new keys), `activity_tables_equivalence`, `realtime_topics_equivalence`, `maintenance_sweep_tier2_equivalence`, `brahma_console_tier3_equivalence`, `character_optimizer_tier3_equivalence`, `backup_uuid_remap_equivalence` / `system_backup_equivalence`, `qtap_import_equivalence` / `system_import_equivalence`, `provisioning_equivalence` (the D23 tripwire, by design) | UNPROCESSED |
| `70f9b495c` | 2026-10-08 | fix(memory): count concurrent reinforcements atomically (bug 182) | **PORT** (classified 2026-10-08; NOT a convergence — bug 182 was found by v4's own review of PR 83, never filed by this port; no schema move). New `MemoriesRepository.incrementReinforcement(characterId, memoryId, at)`: ONE synchronous transaction reads `importance` / `reinforcementCount` by `(id, characterId)`, writes `reinforcementCount = (row ?? 1) + 1`, `reinforcedImportance` recomputed, `lastReinforcedAt = at`, `updatedAt`; WARN `Memory not found for reinforcement` → null; debug `Memory reinforcement counted`. `memory-gate.ts` `countReinforcement` routes BOTH `absorbNearDuplicate` and `reinforceMemory` through it; `reinforceMemory` now patches ONLY content + anchors (and only when they changed — a second write, after the count), the returned memory merges the counted fields. The child-buffered `increment*` arm (debug `[MemoryGate] Reinforcement buffered (job child)`) is a forked-child artifact — NO-PORT half. `calculateReinforcedImportance` moved to the pure `lib/memory/reinforced-importance.ts` (formula byte-identical; re-exported). **v5 carries the bug's shape:** `services/memory_gate.rs` `reinforce_memory` computes `existing_count + 1.0` from the gate's snapshot and writes it absolutely through `update_for_character` in ONE patch with content/anchors (a lost update between two concurrent in-process extraction jobs — the single writer serializes the WRITES, not the read-modify-write; the fix is the increment inside one `Db::write` closure). v5 has no `absorbNearDuplicate` yet (`SkipNearDuplicate` writes nothing — `near_duplicate_skips_without_writing`; it arrives with `3f7320138`'s F1). bugs.md: 182 FIXED, none open | `services/memory_gate.rs` `reinforce_memory` (:657) + the near-duplicate arm, `db/memories*.rs` (a new increment write), `memory_gate.rs:45` `calculate_reinforced_importance` (tier-1, Phase 1 — unchanged); the memory-gate tier-2/tier-3 families (the write count / `updatedAt` ordering moves) | UNPROCESSED |
| `197104649` | 2026-10-08 | fix(sync): read target bytes through the disk adapter | **PORT (log line only)** (classified 2026-10-08). `bytesFor`'s inline `fs.readFile(resolveInTarget(…))` moves to a new `apply-disk.ts` `readDiskFile`, which resolves the SAME way and adds ONE debug line `[Sync] Reading disk file` `{relativePath}`. Behaviour otherwise identical — v5's `bytes_for` already reads through `resolve_in_target` (escape refusal included). The new `disk-boundary.test.ts` is a v4 test-only import guard | `services/mount_index/sync/mod.rs` `bytes_for` (:483) / `sync/apply_disk` (P4.D210); `sync_engine_equivalence` if it captures debug lines | UNPROCESSED |
| `7e9eaf42c` | 2026-10-08 | test: release checklist 2 — regression tests and coverage for 4.10 | **NO-PORT?** (classified 2026-10-08). Tests (19 new files) + ONE behaviour-preserving SPA refactor: the All-LLM pause dialog's Continue moved from `SalonView.tsx` into `continueAllLLMRoom` (`app/salon/[id]/hooks/all-llm-pause-actions.ts` — close, await `setPauseState(false)`, then `handleContinue`; the same order as before), and the unused no-op `handleAllLLMContinue` / `handleAllLLMStop` removed from `useChatControls`. v5 already carries bug 139's fix (`apps/web/src/app/chat/all-llm-pause.ts` + specs). The new v4 tests (sync engine walk/apply/orchestrator, consolidation handler + triggers, fold-other catch-up, daily-db-optimize, avatar-rolls + subprompts routes, save-attribution) are reference material for the lanes porting those surfaces, not ports | — (`apps/web/src/app/chat/all-llm-pause*.ts` already v4-faithful) | UNPROCESSED |
| `783385873` | 2026-10-09 | docs: update 4.10.0 release notes for memory consolidation, wardrobe ledger and pictures, daily optimize | NO-PORT? (docs only — `--stat`: ONE file, `docs/releases/4.10.0.md`, +75 / −9; recorded at the `f5e953a3f` unification's cleanup) | `docs/v4/releases/4.10.0.md` (P4.D260's mirror — stale until re-vendored) | UNPROCESSED |
| `5abcd01ea` | 2026-10-09 | docs(bugs): file bugs 183 and 184 — wardrobe wear wording and whose wear the tools report | **NO-PORT** (docs only — `--stat`: `docs/developer/bugs.md` + two NEW `docs/developer/bugs/bug-18{3,4}-*.md`; committed LOCALLY in the v4 checkout by this port's 2026-10-09 `/dogfood` session at the human's request, NOT pushed — the bug-169 precedent; `origin/main` does not have it). Files v4 bugs 183 ("last last week") and 184 (the wardrobe tools' household wear count), both OPEN and both Faithful in v5 — a future fix commit in v4 is the PORT row, not this one | — | NO-PORT (recorded by `/dogfood`) |
| `3c56a41e7` | 2026-10-09 | docs(bugs): file bug 185 — a restore orphans every archived vault and official store | **NO-PORT** (docs only — `bugs.md` + NEW `docs/developer/bugs/bug-185-restore-orphans-archived-stores.md`; committed LOCALLY by this port's `/dogfood` session at the human's request, NOT pushed). Bug 185 (High, OPEN) is dogfood #141 + #159: v5 FIXED `replace` (P4.147, a ruled divergence) and the human ruled 2026-10-09 that `new-account` is a bug too — v5's fix is ORDERED (`dogfood-findings.md` #159). A v4 fix would retire `FRESH_STORE_RESIDUAL`'s carve — a PORT row then | — | NO-PORT (recorded by `/dogfood`) |

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
