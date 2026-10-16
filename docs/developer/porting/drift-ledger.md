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

- **Oracle baseline: `83334b9a5`**: "Release checklist 4: API standard audit;
  use successResponse in new handlers" (v4 main, 2026-10-13 21:18,
  `4.10.0-dev.176` — behaviour-identical to `64743ee13`, the memory
  refactor's close), adopted when the `83334b9a5` memory-refactor +
  scenarios drift catch-up round was unified, all sixteen lanes landed
  ({P4.D289 → P4.D288} → {P4.D290 → {P4.D296 ∥ P4.D297 ∥ P4.D298}} ∥
  P4.D291 ∥ P4.D292 ∥ P4.D293 ∥ P4.D294 ∥ P4.D295, with P4.D299 ∥ P4.D300 ∥
  P4.D301 ∥ P4.D302 ∥ P4.D303; 2026-10-15). CLAUDE.md's Status bullet for the
  round and its "Oracle baseline" bullet both name `83334b9a5` — they AGREE
  with this file (checked 2026-10-16).
- **Checked:** 2026-10-16, twice. The second `/driftcheck` (07:07+) found ONE
  more commit, `25b8fab6a` — bugs 247 + 250 fixed — classified as the round's
  first **CONVERGENCE** row (bug 247 is this port's filing). The first
  (`/driftcheck` — the §2 probe FAILED as expected:
  v4 SIX commits past `362426c8f` on `main`, the 2026-10-15 dirt COMMITTED and
  the checkout now CLEAN; six rows appended to §3, every one UNPROCESSED,
  classified from the hunks — ONE new D23 move among them, `4877fc2da`'s
  `chat_settings.composerSlashCommands`, the ELEVENTH). Before that:
  2026-10-15 (`/unify` of the `83334b9a5` round — seventeen rows appended,
  the baseline moved); 2026-10-14 (`/setupphase`, `7417e9a1b` and
  `01f508d4e`); 2026-10-14 (the §4 procedure by hand for `83334b9a5`
  itself); 2026-10-13 late evening (`/driftcheck`); and the earlier checks
  the §6 bullets record.
- **v4 `main` HEAD at check: `25b8fab6a`** ("fix: month-word prototype keys
  (bug 247); orphaned typeahead menu (bug 250)", 2026-10-16 07:07,
  `4.11.0-dev.10`) — local `main` = `origin/main` after a fetch.
  **TWENTY-SIX commits past the baseline** — `25b8fab6a` on top of
  `b2793400f` ("chore(deps): Lexical 0.52; fix code-block Enter sending the
  message (bug 251)", 06:43, `4.11.0-dev.9`), and before it (`83334b9a5..main`): the nineteen recorded
  2026-10-14/15 (still UNPROCESSED — the `release: 4.10.0` squash, the
  release-checklist commits, bugs 247 / 248, copy-to-folder, the hair note,
  `362426c8f` Pascal trigger phrases with the TENTH D23 move) and the SIX this
  check classified: **`f0db8cbb8` Pascal wardrobe effects** (the 2026-10-15
  dirt; PORT-NEW on the `362426c8f` run service + a PORT fix to
  `wardrobe_take_off` on a composite — no schema move, JSON-column only);
  **`6069a7acd` bug 249** (the lone-character chain runaway — PORT on the turn
  orchestrator, provenance "Original to v4", v5 shares the bug); **`4877fc2da`
  composer slash commands — PORT-NEW + PORT WITH THE ELEVENTH D23 MOVE**
  (`add-composer-slash-commands-field-v1`: `chat_settings.composerSlashCommands
  INTEGER DEFAULT 1` between `composerUnicode` and `impersonationVoiceMode`);
  `b93c2efda` two dependency specs (docs-only); `eb5d42338` the dependency safe
  batch (`openai` 7.30.0 → **7.34.0** re-bundled into all ten `plugins/dist`
  bundles, `zod` 4.6.5 → 4.6.7, `katex` 0.19); `b2793400f` Lexical 0.52 + bug
  251's code-block exit (a narrowed trailing-blank trim v5's composer mirrors)
  + bug 250 FILED (open, Lexical-only, v5 N/A); and **`25b8fab6a` bugs 247 +
  250 FIXED** — bug 247 (this port's filing, the harness's
  `PROTOTYPE_KEY_MONTH_THROWS`) CONVERGES onto v5's answer, bug 250 is a
  Lexical-only typeahead guard (NO-PORT for v5).
- **v4 `bugfix` tip at check:** `489b228b6` ("bugfix: started 4.10.0 bug
  branch", 2026-10-14; `origin/bugfix` the same) — UNCHANGED since 2026-10-15.
  **Measured by CONTENT:** `git diff 2432b9057 bugfix -- lib app packages
  plugins` EMPTY. **Bugfix carries ZERO unabsorbed content.** **`release`
  tip:** `2432b9057` (`release: 4.10.0`; `origin/release` the same) —
  unchanged. Re-probe BOTH branches every time as before.
- **Checkout at check:** branch **`main`**, tree **CLEAN** at `25b8fab6a`. The
  2026-10-15 dirt (Pascal wardrobe effects, the bug-249 fix, the composite
  `takeOffWearable` fix) landed as `f0db8cbb8` + `6069a7acd`, and the commits
  match what the dirt described — nothing of it is left uncommitted. The
  live checkout no longer poisons a regen by DIRT, but HEAD is twenty-six
  commits past the baseline, so it still poisons one by DRIFT.
- **Verdict: DRIFT PENDING — 26 commits (§3), all UNPROCESSED: 1
  CONVERGENCE** (`25b8fab6a` bug 247 — its bug-250 half NO-PORT), **10 NO-PORT?**
  (the nine recorded 2026-10-15 + `b93c2efda` the two dependency specs),
  **11 PORT** (the eight recorded 2026-10-15 + `6069a7acd` bug 249,
  `eb5d42338` the `openai` 7.34.0 re-bundle — a recorder / SDK-guard move,
  `b2793400f` bug 251's trailing-blank trim on v5's code-block exit), **4
  PORT-NEW + PORT** (`bbbcd2388` copy-to-folder; `362426c8f` Pascal trigger
  phrases **WITH THE TENTH D23 MOVE**; `f0db8cbb8` Pascal wardrobe effects +
  the composite take-off fix; `4877fc2da` composer slash commands **WITH THE
  ELEVENTH D23 MOVE**). **Bug 247, this port's filing, is FIXED** (`25b8fab6a`)
  — at a pin past it `memory_time_equivalence`'s `PROTOTYPE_KEY_MONTH_THROWS`
  table trips BY DESIGN (v4 stops throwing); retire it by measurement (§5.4).
  v4's own `bugs.md` now reads "Bugs **1–251** are **fixed in v4**; none are open." **Regen rule: PIN REQUIRED at `83334b9a5`** (§5.1 — HEAD
  past the baseline; the checkout is now clean, so a pin at or past
  `25b8fab6a` is a clean choice for the next round, and the TENTH + ELEVENTH
  moves ride ONE `fresh_schema.json` re-dump there).

### ⚠ LIVE-DATA — what stands after this round

**The LIVE-DATA HAZARDS block of the `4a38222e6` round is RETIRED by
measurement.** Hazards (1)–(5) and (a)–(b) each closed by a named family on
this baseline: (1) the dropped `chat_settings` columns — the settings move
re-homed as a boot ensure (`memory_maintenance_settings_move_ensure_equivalence`)
and every v5 reader on `instance_settings.memoryMaintenance`
(`chat_settings_column_sites_guard`); (2) the dropped vector tables — the
verify-and-copy-then-DROP ensure (`memory_vector_mirror_retire_ensure_equivalence`)
and the index over rows (`memory_index_tier2_equivalence`,
`vector_store_tiers_equivalence`); (3) the dropped chats watermark columns —
P4.D265's chat-column arm RETIRED, the cursor table + chain
(`memory_extraction_cursors_ensure_equivalence`) and ZERO v4 re-runs after
v5's chain on planted, fresh and v4-migrated instances
(`zero_migrations_rerun_equivalence` — with `add-memory-tiers-v1` stamped in
the settled-chain arm since this unification, so a v5-provisioned instance
never ping-pongs the column with v4); (4) the reinforced-importance formula
(`memory_scoring_equivalence`, the stamped recompute in
`memory_rewrite_heals_ensure_equivalence`); (5) the cursor carriers
(`cursor_bundle_equivalence`, the backup / restore / `.qtap` families); (a)
the scenario columns (`chats_scenario_source_ensure_equivalence`, the chat
carriers); (b) the chat wipe sparing manual / cold / digest rows
(`memories_routes_equivalence`'s wipe rows, the ONE retire path). Live Friday
and every post-migration copy are openable by a v5 at this baseline again
(the round's 💸 headline: the first boot runs every ensure as a measured
NO-OP). Two notes REPLACE the block (R-AI): **a v5 build from BEFORE this
round must not open an instance this round's ensures have migrated** (the
same destructive moves, now v5's own); and **the extraction rate ledger is
per-process**, so a v4 and a v5 sharing one instance each keep their own
hourly count. **NEW, from the commits past the pin:** a v4 at `4.11.0-dev`
(past `362426c8f`) ALTERs live `chat_settings` (`customToolTriggers`) and
writes `pascalMeta.invokedBy: 'trigger'` rows a v5 at THIS baseline refuses
on read / restore / import (the two-value enum) — **dogfood only on a copy
taken BEFORE v4 ran past `362426c8f` on live Friday**, until the TENTH move is
absorbed. **Added 2026-10-16:** a v4 past `4877fc2da` also ALTERs live
`chat_settings` (`composerSlashCommands`, the ELEVENTH move) and a v4 past
`f0db8cbb8` writes `pascalMeta.effects` entries targeted `wardrobe.*` (the
`target` is a free string — v5 reads them, but its audit / export paths have
never seen the `WardrobeEffectRecord` shape); the same copy-before rule now
reads **a copy taken before v4 ran past `362426c8f`**, unchanged, since that
commit is the earliest of the three. The P4.D274 store-name plan probe on the copy, the `52d6e7ecd` /
`07b8f0209` notes (the `chat_informs.permanent` ALTER, the voice-mode
translation) stand.

### Schema / D23

- **LANDED this round (P4.D288):** ONE `fresh_schema.json` re-dump from the
  pin carrying the EIGHTH (`545e299bb`: `chats.scenarioSource` /
  `scenarioNotes`), the NINTH (`4ae0c4ca6`: `tfidf_vocabularies.
  textShapeVersion`) and the memory programme's moves (`memories.supersededAt`
  gained; `vector_entries` + `vector_indices` + `chats.
  otherExtractionWatermarkMessageId` gone; the NEW `memory_extraction_cursors`
  table in the migration's UNIQUE + FK shape at first boot; `chat_settings`
  without `autoHousekeepingSettings` / `memoryExtractionLimits`);
  `migration_indexes.json` 63 → 62 (+ the partial `idx_memories_supersededById`,
  − the two vector indexes); the TWELVE migrations re-homed as boot ensures in
  v4's registration order (three DESTRUCTIVE, the two data rewrites ledgered +
  stamped over v4's `migrations_state`), each differentially compared against
  v4's REAL `MigrationRunner`; the settled-chain arm stamps the episode,
  cursor AND tiers ids on a v5-provisioned instance; every committed `*-main.db`
  pair widened / narrowed in place through v4's real migrations;
  `provisioning_equivalence` REQUIRES `QT_FRESH_SCHEMA_LIVE` dumped FROM THE
  PIN; `restore_vintage_state`'s boot-align list carries every new ensure.
- **PENDING — the TENTH move** (`362426c8f`, past the pin):
  `chat_settings."customToolTriggers" INTEGER DEFAULT 0` between
  `"customTools"` and `"compositionModeDefault"` (`ChatSettingsSchema`
  `customToolTriggers: z.boolean().default(false)`); the migration
  `add-custom-tool-triggers-field-v1` (`addColumnIfMissing`, no backfill,
  `dependsOn add-custom-tools-field-v1`) re-homes as an UNSTAMPED table-gated
  boot ensure in the `chat_settings_*_repair.rs` shape, `chat_settings_seed.json`
  gains `customToolTriggers: 0`, and `pascalMeta.invokedBy` widens to
  `'trigger'` (+ `triggerPhrase`) in the row and event schemas (JSON-column
  only). `public/schemas/qtap-custom-tool.schema.json` gains `triggers`.
- **PENDING — the ELEVENTH move** (`4877fc2da`, past the pin, recorded
  2026-10-16): `chat_settings."composerSlashCommands" INTEGER DEFAULT 1`
  between `"composerUnicode"` and `"impersonationVoiceMode"`
  (`composerSlashCommands: z.boolean().default(true)`); the migration
  `add-composer-slash-commands-field-v1` (`addColumnIfMissing`, table-gated, no
  backfill, registered right after `add-custom-tool-triggers-field-v1`)
  re-homes as an UNSTAMPED boot ensure beside `chat_settings_composer_repair.rs`'s
  composer columns; `chat_settings_seed.json` gains `composerSlashCommands: 1`.
  **The TENTH and ELEVENTH moves ride ONE `fresh_schema.json` re-dump** from a
  pin at or past `4877fc2da`.
- No other DDL past the pin: `bbbcd2388`'s `wardrobe.types.ts` hunk is
  request-body Zod; `49f8906c3` touches the remapper only; `f0db8cbb8`'s
  `pascalMeta` changes are comments on a free-string `target` (JSON column);
  `6069a7acd`, `eb5d42338`, `b2793400f` carry no schema hunk.

### Regen, dependencies, help

- **Regen rule: PIN REQUIRED at `83334b9a5`** (§5.1) — HEAD is twenty-five
  commits past the baseline (the checkout is CLEAN since 2026-10-16). ⚠ **IMPORT-DEAD at
  HEAD** for the memory family's oracle cases is no longer the issue it was
  (the round re-aimed every case at the pin's modules); past the pin, HEAD
  renames nothing the cases import, but `362426c8f` moves the Pascal run
  entrances (`run-custom-handler.ts` / the manual-run route → `run-service.ts`)
  — the Pascal oracle cases import the handler, not the route, and still
  load; their captured log lines will MOVE at the next pin. `f0db8cbb8`
  re-shapes `takeOffWearable` callers (`wardrobe-take-off-handler.ts`) and
  `6069a7acd` adds a turn-orchestrator INFO line — both families' captures
  move at the next pin too.
- **Dependencies: ONE moved past the pin** — `@quilltap/plugin-utils` 2.6.3 →
  **2.7.1** (`a8b2d2861`; root `package.json` + all fifteen `plugins/dist/*/
  package.json` want `^2.7.1`; `openai-compatible` 1.0.49, DeepSeek 1.0.28,
  NanoGPT 1.2.10 rebuilt). The pin's own `npm ci --offline` tree is at 2.6.3;
  the provider recorders resolve plugin-utils from the plugin dirs'
  `node_modules` — at the NEXT pin those must be 2.7.1 or a regen silently
  replays 2.6.3 (`provider_sdk_version_guard` is the tripwire).
  **Added 2026-10-16 (`eb5d42338`):** `openai` 7.30.0 → **7.34.0** at the root
  AND re-bundled into all ten `plugins/dist/*/index.js` (`VERSION2 =
  "7.34.0"`), `zod` 4.6.5 → 4.6.7, `katex` ^0.19.0; `b2793400f` moves Lexical
  0.43 → 0.52 (v4's client only — v5's SPA is ProseMirror, NO-PORT).
  `provider_sdk_version_guard`'s `INSTALLED_ROOT_OPENAI_SDK = "7.30.0"` and the
  recorded stamps trip at a pin past `eb5d42338` BY DESIGN — re-record, never
  pin the old stamp.
- **Help / docs:** `help/` is **131 files at the pin**, vendored WHOLE and
  byte-identical (P4.D303: `memory-maintenance.md` + `scenario-browser.md` in,
  `memory-consolidation.md` + `memory-housekeeping.md` out). Past the pin
  (count stays 131): `bbbcd2388` edits 2 pages, `7458ff94e` `wardrobe.md`,
  `362426c8f` `custom-tools.md` (+30) / `chat-settings.md` / `pascals-
  workbench.md` / `rng-tool.md`. Docs past the pin: `8489d08b9`'s 17 spec
  moves to `features/complete/` + 4 deletions (incl. the two native-port
  specs and `MCP.md`) + `API.md` +98; `3f64d3a6c`'s README rewrite + NEW
  `docs/FEATURES.md`; `75d094e63`'s CLI README (+14); `1e1712a12` +
  `7458ff94e` moving CHANGELOG sections to `CHANGELOG_V4.md`; `936a34720`'s
  three specs; `362426c8f`'s `API.md` / `DDL.md` / `FEATURES.md` /
  `features/complete/pascal-*`; and (2026-10-16, count still 131 at
  `b2793400f`) `f0db8cbb8` `custom-tools.md` (+57) / `pascals-workbench.md` /
  `wardrobe.md`, `4877fc2da` `chat-settings-composer.md` (+48) /
  `custom-tools.md` / `rng-tool.md` / `run-tool.md`, `b2793400f` `chats.md`;
  docs: `b93c2efda`'s two dependency specs (moved to `features/complete/`),
  the wardrobe-effects + slash-commands specs moved to `features/complete/`.
  Each catch-up round re-vendors what it absorbs.

### Rounds, guards, state

- **The `83334b9a5` memory-refactor + scenarios drift catch-up round —
  UNIFIED 2026-10-15** (the §6 bullet; the round record in `status-log.md`).
- **Next:** the owed dogfood pass (`/dogfood`) on a Friday copy taken BEFORE v4
  ran past `362426c8f` — the round's 💸 headline first (a post-migration copy's
  first v5 boot: every ensure a NO-OP, measured); then the follow-ups smalls
  round (the review's recorded should-fixes: the repository-fallback `?`
  reads, `OFFSET_OCCURRED_AT_ROW_SCHEMA`'s ruling, the two unclassified
  admission / cascade divergences, P4.D291's probe fixture, P4.D298's
  `create_project_explicit_wins`, the lock's closed-enum read + v4's three
  lock lines, the D301 toast duration, the Almanack / API-doc smalls); then
  the `362426c8f` drift catch-up round — the TENTH D23 move (Pascal trigger
  phrases), plugin-utils 2.7.1's cache-hit exclusion, the restore remapper,
  bug 248, the hair precedence note, copy-to-folder, the Tier R bytes, the
  About blurbs — and the NO-PORT ratifications of the release-squash row and
  its siblings (`/setupphase`; `/driftcheck` first, the dirt's commit expected).
  **Revised 2026-10-16:** the dirt committed, so the catch-up round now spans
  all twenty-five rows — order `362426c8f` + `f0db8cbb8` + `4877fc2da`
  together (one re-dump carries the TENTH + ELEVENTH moves; wardrobe effects
  extend the trigger-phrase run service), with bug 249's chain fix, the
  `openai` 7.34.0 re-record, bug 251's narrowed code-block trim and the slash
  menu's SPA vertical as their own lanes, and bug 247's convergence (retire
  `PROTOTYPE_KEY_MONTH_THROWS`, regen the `memory_time` family) as a small.
  Pin at or past `25b8fab6a` (clean).

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
| `7417e9a1b` | 2026-10-13 | Release checklist 5: dead code sweep | **NO-PORT?** (classified 2026-10-14 from the hunks; 21 files, +80 / −338; `4.10.0-dev.177`, parent `83334b9a5`; the dirt the earlier probe recorded, committed). Every `lib/` hunk deletes an export with NO production caller: the `lib/memory/index.ts` (189) and `lib/memory/recall/index.ts` (47) BARRELS, `createMountWardrobeItem` / `deleteMountWardrobeItem` (`wardrobe-writes.ts`), `withMemoryTimeZone` + its `[Memory] Extraction clock zone resolved` DEBUG (`extraction/prompts/shared.ts` — no caller, as the extraction survey measured; the prompt bodies untouched), `MEMORY_CALENDAR_DAY_MS` (`time/calendar.ts`), `emptyBase` + two sync type aliases moved inline (`mount-index/sync`), `ProgressionIssue`, `EXTRACTION_CURSOR_PASSES`, `isModerationRefusal`; tests, `knip.json`, `DEAD-CODE-REPORT.md`, CHANGELOG, the refactor spec (one line), stamps. ⚠ **Oracle effect at a later pin:** any `harness/oracle` case importing the `@/lib/memory` or `@/lib/memory/recall` BARREL, or one of the deleted names, goes IMPORT-DEAD — measure at the next pin. | None in behaviour; the `83334b9a5` round's pin predates it. | **UNPROCESSED** — past the `83334b9a5` round's pin |
| `01f508d4e` | 2026-10-13 | Release checklist 6: prune leftover debug logging | **PORT — log lines only** (classified 2026-10-14 from the hunks; 9 files, +12 / −17; `4.10.0-dev.178`, parent `7417e9a1b`). DEBUG lines REMOVED: `[Transcript] Projecting chat transcript` (`lib/chat/transcript-projection.ts`), `[Sync] Reading disk file` (`lib/mount-index/sync/apply-disk.ts`), the SPA `console.debug('[useWardrobeItemActions] Duplicating wardrobe item', …)`. RENAMED: `markMailAlertsDelivered` → `Mail alerts marked delivered`, `discardLetter` → `Letter discarded` (`lib/post-office/mailbox.ts`, fields unchanged). | The `83334b9a5` round ports `markMailAlertsDelivered` with its PIN text (P4.D291 — the mail deferral); v5's transcript projection, the sync disk read and the Post Office letter discard carry the pre-commit lines where ported. A small catch-up (or rider) re-pins them. | **UNPROCESSED** — past the `83334b9a5` round's pin |
| `a42c282b8` | 2026-10-13 | Release checklist 7: qt-* theme classes | **PORT — one SPA class attribute** (classified 2026-10-15 from the hunks, §5.3; 6 files, +11 / −6; `4.10.0-dev.179`, parent `01f508d4e`). The message says 158 components were REVIEWED; exactly ONE ships a hunk: `components/memory/memory-card.tsx` — the archived card's "Superseded by a digest" `<button>` `className="text-primary hover:underline"` → `className="qt-link"` ("so themes control its color"). Everything else is `docs/CHANGELOG.md` (+5), the README badge, the three version stamps (`package.json`, `packages/quilltap/package.json`, the lockfile's two root `version` lines). No `lib/`, `app/api/`, schema, migration, dependency, `help/` or bug-file hunk. | v5 `apps/web/src/app/memory/memory-card.ts:70` (the P4.D276 memory-SPA card; last touched in the `01a83539d` round) carries the PRE-commit `class="text-primary hover:underline"` on that exact button, and `memory-card.spec.ts:253` asserts `classList.contains('text-primary')` — a one-token swap + one spec line. `.qt-link` already exists in the SPA (`styles/qt-components/_typography.css:157`, `_interactive.css:221/:227`; 19 other `class="qt-link…"` sites). The other `text-primary hover:underline` buttons in `memory-card.ts` (`:77` expand, `:114`, `:125` edit) and `memory-list.ts:143` are UNCHANGED in v4 — do not over-port. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `a8b2d2861` | 2026-10-13 | Release checklist 8: plugin self-containment; exclude cache hits in OpenAICompatibleProvider | **PORT — the `openai-compatible` provider's usage normalisation (BOTH send paths); a dependency move** (classified 2026-10-15 from the hunks, §5.3; 31 files, +257 / −95; `4.10.0-dev.180`, parent `a42c282b8`). The "audit" is prose — no hunk enforces plugin self-containment. The ONE source hunk: `packages/plugin-utils/src/providers/openai-compatible.ts` (+58 / −16) — NEW `protected normalizeUsage(usage: OpenAI.Completions.CompletionUsage \| null \| undefined): Pick<LLMResponse,'usage'\|'cacheUsage'>`: `cachedTokens = usage?.prompt_tokens_details?.cached_tokens ?? 0`; `promptTokens: Math.max(0, prompt_tokens − cachedTokens)`, `completionTokens` unchanged, `totalTokens: Math.max(0, total_tokens − cachedTokens)`; `cacheUsage: { cacheReadInputTokens: cachedTokens, cachedTokens }` ONLY when `cachedTokens > 0` (else the key is ABSENT); a NEW DEBUG line when `> 0`: `` `${providerName} usage: excluding prompt-cache hits from prompt/total` `` with `{ context: `${providerName}Provider.normalizeUsage`, promptTokens, cachedTokens }`. `sendMessage` spreads `...this.normalizeUsage(response.usage)` in place of the literal `usage` object (so `usage` stays ALWAYS present, all-zero on a missing block). `streamMessage`: `accumulatedUsage` is now the whole `chunk.usage` object (typed `CompletionUsage \| null`, no longer a three-key copy); the terminal chunk spreads `...normalizeUsage(accumulatedUsage)` + **`rawProviderUsage: accumulatedUsage`** ONLY when a usage frame arrived (previously `usage: undefined`, never `rawProviderUsage`). Tests: `__tests__/unit/plugins/local-provider-params.test.ts` +59 (four cases: 1000/50/1050 with 800 cached → 200/50/250 + `cacheUsage {800,800}`, stream twin + `rawProviderUsage` equals the raw block, no-cache → no `cacheUsage` key, cached 20 > prompt 10 → all 0). **Bundles:** `plugins/dist/qtap-plugin-{openai-compatible,deepseek,nanogpt}/index.js` each +49 / −15 (the inlined base class, verbatim); manifests openai-compatible 1.0.48 → **1.0.49**, DeepSeek 1.0.27 → 1.0.28, NanoGPT 1.2.9 → 1.2.10 — **behaviour changes for `openai-compatible` ALONE**: its `OpenAICompatibleEndpointProvider` subclass overrides neither `sendMessage` nor `streamMessage`, while DeepSeek and NanoGPT override BOTH and keep their own `extractCacheUsage` (`prompt_cache_hit_tokens`; `cache_read_input_tokens ?? prompt_tokens_details.cached_tokens`). **Dependency:** `@quilltap/plugin-utils` `^2.6.2` → `^2.7.1` in the root `package.json` and ALL 15 `plugins/dist/*/package.json`; the lockfile resolves `plugin-utils-2.7.1.tgz` from the registry (2.7.0 was a non-appearing publish; 2.7.1 is a version-only republish per `packages/plugin-utils/CHANGELOG.md`). No schema, migration, `help/`, `app/`, `lib/` or bug-file hunk. | v5's `crates/quilltap-core/src/model/decoders/chat_completions_sse.rs` `Flavor::OpenAiCompatible` arm of `build_usage` is `=> (None, None)` (no cache source) and its doc says "no `usage` inside … no cache usage"; the absent-usage-frame rule ("the plain base leave[s] `usage` undefined") and the no-`raw_provider_usage` rule are both now v4-stale for this flavor (the W4.7b decoders, P4.13/P4.14 `StreamMessage` rewrite, P4.122/P4.128 frames). `crates/quilltap-core/src/model/response_parse.rs` `ChatFlavor::OpenAiCompatible` ("the base — content / finishReason / usage / raw only", W4.7d) needs the same subtraction + conditional `cache_usage`; the shared `sub_floor` / `prompt_tokens_details.cached_tokens` readers already exist for ZAi / OpenRouterVision. `crates/quilltap-core/src/model/plugin_catch_log.rs` pins the OAC ERROR lines (`:173-178`) — the NEW DEBUG `… usage: excluding prompt-cache hits from prompt/total` / `context=OpenAICompatibleProvider.normalizeUsage` is a candidate capture pin. **Oracle:** `harness/oracle/fixtures/streams/chat_completions_sse/` has NO openai-compatible `cached_tokens` case (only `zai-cached`, `nanogpt-cache-*`, `openrouter-tools`) and `response-bodies.recorded.ndjson` none either — a port needs a NEW `openai-compatible-cached.wire` + response-body row; **the recorders (`record-stream-fixtures.mjs`, `record-response-bodies.mjs`) resolve `@quilltap/plugin-utils` from the PLUGIN dir's own `node_modules`**, so at the next pin that dir must `npm ci` to 2.7.1 or the recordings silently replay 2.6.3 behaviour (§5.2; memory note "pin node_modules drift"). `provider_sdk_version_guard.rs` reads plugin-utils via the SDK stamp — check the 2.7.1 bump does not trip it. The autonomous-room token budget (the enclave `step()` spend, P4.D79-era) is the consumer that motivated v4 — v5's budget arithmetic reads the normalised `StreamUsage`, so the fix lands there for free once the decoder moves. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `49f8906c3` | 2026-10-14 | Release checklist 10: backup/restore completeness; remap seat ids in new-account restore | **PORT — the pure `remapBackupData` gains four fields** (classified 2026-10-15 from the hunks, §5.3; 8 files, +201 / −6; `4.10.0-dev.181`, parent `a8b2d2861`). The "audited every table and column … all are backed up and restored" claim ships NO backup/restore code hunk — it is `__tests__/unit/lib/backup/restore-field-fidelity.test.ts` (NEW, 107 lines, mocked repos: the 4.10 chat / message / `memories.supersededAt` / `chat_settings.impersonationVoiceMode` + `wardrobeImageSettings` / `tfidf_vocabularies.textShapeVersion` columns), tests-only; the CHANGELOG records the deliberate NON-carry of the FTS index, `chats.transcriptVersion` and the Concierge refusal ledger (no hunk — a statement of existing behaviour). The ONE `lib/` hunk, `lib/backup/restore/uuid-remap.ts` (+41 / −1): NEW `const PARTICIPANT_ID_JSON_FIELDS = ['turnQueue','spokenThisCycleParticipantIds','cycleOrderParticipantIds'] as const` + `remapJsonIdList(value, remapper)` — a non-string returns as-is; `JSON.parse`; anything but an all-string array (or a parse throw) returns the ORIGINAL value untouched ("the turn manager already reads a malformed value as empty"); else `JSON.stringify(remapper.remapArray(parsed))`. Inside `remapBackupData`'s chat loop, immediately AFTER the `scenarioSource` remap (`545e299bb`'s line, `:349-351`) and BEFORE `remappedChat.participants`: `for (const field of PARTICIPANT_ID_JSON_FIELDS) if (field in chat) remappedChat[field] = remapJsonIdList(chat[field], remapper)` — an ABSENT key stays absent (the test asserts `not.toHaveProperty('turnQueue')`). The messages map's `remapArrayFields(…, ['attachments'])` → `['attachments', 'targetParticipantIds']` (a whisper's audience). Note the ORDER: the JSON-list pass runs before participants are remapped, which is fine because `UuidRemapper.remap` mints-or-reuses per id. `uuid-remapper.test.ts` +43 (two cases). No schema, migration, route, `help/` or bug-file hunk. | v5 `crates/quilltap-core/src/services/backup/uuid_remap.rs` (P4.9G6, the pure remapper; grown by P4.D205 / P4.D264) — the messages arm at `:850-864` is `chain(r, m, &["id","swipeGroupId","participantId"], &["attachments"])` and NO `turnQueue` / `spokenThisCycle…` / `cycleOrder…` handling exists; the proof is `backup_uuid_remap_equivalence` over the committed `harness/oracle/fixtures/uuid-remap-corpus.json` (19 lines) — the corpus holds `"turnQueue": "[]"` ×2 and `"cycleOrderParticipantIds": "[]"` ×2 and ZERO `targetParticipantIds`, so at the next pin the oracle output is BYTE-IDENTICAL for the current corpus (an empty list remaps to `"[]"`) — **the corpus must GROW** (a populated turn queue / cycle order / whisper audience, a malformed `'not json'`, an absent key) or the differential passes vacuously (memory note "Tier R vacuous-equal rows"). **Stacks on the current round:** P4.D293 ports `remapScenarioSource(chat.scenarioSource, …)` into the same loop (order §… `:539`), and this hunk sits directly under it — a rider on P4.D293's landed site, not a conflict. `preserve_order` + `shift_remove` rules of the module apply (a present key keeps its position; `JSON.stringify` of the remapped array is byte-level — v4 emits `["a","b"]` with no spaces, so the Rust side must serialise compactly). | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `75d094e63` | 2026-10-14 | Release checklist 12: CLI docs, completions and tooling; fix migrations status ids | **PORT (Tier R captured bytes: the main help line + zsh/fish templates; the vendored CLI README) — the `migrations status` fix is NO-PORT until v5 has a migration runner** (classified 2026-10-15 from the hunks, §5.3; 11 files, +101 / −16; `4.10.0-dev.182`, parent `49f8906c3`). (1) `packages/quilltap/lib/migrations-commands.js` (+22 / −4): NEW exported `extractMigrationId(fileContent)` — `const MIGRATION_ID(: T)? = '…'` + a `^\s*id:\s*MIGRATION_ID\b` reference wins, else the old `^\s*id:\s*['"]…['"]` literal, else `null`; `readSourceMigrations` uses it for `id` and the `description` fallback. Measured at the commit: **62 of 228** `migrations/scripts/*.ts` use the constant form — before this, those 62 vanished from `source` and every instance that had run them reported them `retired` (`status`'s `retired` / `retiredList`, `pending`'s complement). NEW `lib/__tests__/migrations-source-ids.test.js` (54 lines; also asserts > 100 scripts and every script yields an id). (2) `bin/quilltap.js` ONE line: `anchor-probe <characterId>    Probe whether episodic anchor lines suppress Memory Gate reinforcement` → `… suppress reinforcement at admission`; `lib/completion/zsh.template` `'anchor-probe:Probe whether episodic anchor lines suppress reinforcement at admission'`; `fish.template` `-d 'Probe anchor-line effect on admission'`; the bash template never carried the phrase (0 hits, no hunk). After the commit `git grep -i "memory gate" -- packages/quilltap` is EMPTY — `anchor-probe-command.js`'s own `--help` had already lost it (`93c5af6ad`-era, P4.D299's capture). (3) `packages/quilltap/README.md` +14 / −4: `docs ls -R/--recursive`, `--sort name\|time\|size\|links`, `-r/--reverse`, `--links`; `docs tree --long` ("accepted but reserved — it does not yet add columns"); `memories --limit N` (default 50), `--full-titles`, `grep -l/--paths-only`; `themes registry remove/refresh/keygen/sign` + `--key/--name/--output`; `instances list --names-only` / `docs list --names-only`. Docs only — no flag is ADDED to any command (the README catches up to existing parsers). CHANGELOG, badge, stamps. No schema, migration, `lib/`, `app/`, `help/` or bug-file hunk. | v5 `crates/quilltap-cli/src/help/main_help.txt:19`, `help/completion/zsh.template:46`, `help/completion/fish.template:64` carry the "Memory Gate" bytes — these are `include_str!` captures of v4's output at the pin (P4.D269 unit C re-captured `main_help.txt` at `01a83539d`; P4.D299 re-captures `src/help/*.txt` + `src/help/completion/*.template` at `83334b9a5`, which PREDATES this commit, so the round lands still-stale bytes by design) and `cli_differential.rs` / `completion_behavior` pin them → a three-line re-capture + Tier R regen at the next pin. (`help/anchor_probe_help.txt:7` ALSO says "Memory Gate" — that is anchor-probe's OWN help, which v4 changed EARLIER; P4.D299 owns it — verify it lands.) `migrations`: v5 `main.rs` `SUBCOMMANDS` lists `"migrations"` but `match sub_name` routes it to `not_yet_available` (no `migrations_cmd.rs`; the runner is deferred, CLAUDE.md) — nothing to port; when a v5 `migrations status` arrives it must read BOTH id forms from day one. `docs/v4/packages-quilltap-README.md` is the mirror of `packages/quilltap/README.md` (byte-equal to `4a38222e6` AND `01a83539d`; P4.D303 re-vendors it at `83334b9a5`) → re-vendor at the next pin. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `8489d08b9` | 2026-10-14 | Release checklist 13: documentation freshness; tidy feature specs | **PORT (ONE SPA hunk — the About page's four blurbs) + a `docs/v4/` re-vendor with MOVES and DELETES; the remainder NO-PORT?** (classified 2026-10-15 from the hunks, §5.3; 72 files, +357 / −1,793; `4.10.0-dev.183`, parent `75d094e63`). **The one user-visible code hunk:** `app/about/AboutView.tsx` (+8 / −4) — four `<span>` blurbs: *Aurora – Characters* "…pronouns, aliases, clothing records, personalities, multi-character…" → "…pronouns, aliases, personalities, subprompts and progressions, a four-tier wardrobe with folders, pictures and a wear ledger, multi-character…"; *The Salon – Chat* "…server-side rendering, and embedded tool messages" → "…server-side rendering, embedded tool messages, Inform notes from the wings, the Scenario Builder, and indexed message search"; *The Commonplace Book – Memory* "long-term memory with semantic recall, memory gate, proactive recall, and deduplication" → "long-term memory with semantic recall and proactive recall, active and archived shelves, consolidation into digests, and one per-character maintenance pass"; *The Concierge* "content classification with detection, auto-routing to uncensored providers, quick-hide integration, and a three-state…" → "refusal failover to the uncensored desk, an opt-in pre-screen, quick-hide integration, a Try Uncensored shield on every reply, and a three-state…" (tail unchanged). **Every other `lib/` / `app/api/` / `migrations/` / `eslint.config.mjs` / `__tests__` hunk is a COMMENT path rename** `docs/developer/features/<x>.md` → `features/complete/<x>.md` (measured: `inform-block.ts`, `user-narration-anchor.ts`, `builtin-refit-policy.ts`, `avatar-cache.ts`, `cache-key.ts`, `extraction/prompts/shared.ts`, `recall/tags.ts`, `recall/tuning.ts`, `scenario-builder.service.ts`, `scenario-builder/route.ts`, `collapse-duplicate-avatar-rolls-v1.ts` — the migration hunk is a comment; NO migration logic, NO DDL, NO `lib/schemas` change). **Docs:** 17 specs `git mv` to `features/complete/` (`avatar-configuration-cache`, `builtin-tfidf-memory-tuning`, `composer-smart-typography`, `composer-spellcheck`, `composer-text-replacement`, the six `concierge-overhaul*`, `llm_api_costs_breakdown`, `prompt-trust-and-anti-committee`, `recall-multiplier-retuning`, `recall-probe-set-runbook`, `salon-inform`, `scenario-builder`); FOUR DELETED — `features/MCP.md` (265), `distant_future_separate_api.md` (654), **`native-core-api-boundary.md` (322), `native-port-phase-0.md` (298)** (v4's copies of THIS port's early design notes); `docs/developer/API.md` +98 / −16 (documents `characters/[id]?action=depiction-guidelines` GET/PUT, wardrobe tier `?action=instructions` GET/POST, `link-image`, chat `rebuild-system-prompt` / `accessible-stores` / `update-tool-settings` / `render-conversation`, the Document-Mode action table, `mount-points/[id]?action=reindex` / `embed`, the remaining chat-settings fields; DROPS the nonexistent `groups/[id]?action=stores` / `linkStore` / `unlinkStore`); `docs/releases/4.10.0.md` reordered + "Repairs through bug 246"; `CLAUDE.md` glossary rows Ariel / Suparṇā / Brahma + the Carina link; README (one Concierge paragraph three states, memory as tiers, FTS search, etc.); `bugs.md` + `bug-62` / `bug-202` link paths ONLY (no provenance change → no convergence); `.claude/commands/*`. Stamps. | **SPA:** v5 `apps/web/src/app/screens/about/about-page.ts:326` (Aurora), `:330` (Salon), `:338` (Commonplace Book), `:346` (Concierge) carry the PRE-commit sentences byte-for-byte, and `about.spec.ts:116` asserts the old Concierge text — the About page is a static content port of `AboutView.tsx` (p4.9 About + Profile; last re-synced in P4.D128 unit 3 and P4.D156 unit 2, "the About page's widened sentences" — same shape, four sentences this time). **`docs/v4/` mirror (P4.D303 owns `docs/v4/**` at `83334b9a5`):** `docs/v4/developer/features/` holds the 17 to-be-moved specs at their OLD paths (36 entries + `complete/` 101) and the four to-be-deleted files, incl. `native-core-api-boundary.md` / `native-port-phase-0.md` — the re-vendor must MOVE and DELETE, not just overwrite (`git mv` + `git rm`, then the `find docs/v4 -type f` count check the P4.D303 recipe uses); `docs/v4/developer/API.md` (248,889 B at `4a38222e6`) refreshes; `docs/v4/releases/4.10.0.md` lands when `docs/releases/` is mirrored (currently 4.9.0–4.9.2 only — confirm the recipe's `docs/**` glob picks it up). v5's own 39 files that cite `docs/developer/features/<x>.md` in carried why-comments (`crates/`, `apps/web/src`, porting docs) are v5 prose, not a port obligation — leave or tidy at leisure. The deleted `native-*` specs are v4's copies; v5's authoritative versions live under `docs/developer/porting/` and are unaffected. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `052659624` | 2026-10-14 | docs(bugs): file bug 247 — a `when` phrase naming "constructor" as its month throws and fails the extraction job | **NO-PORT? — a bug FILING, not a fix; the CONVERGENCE row is owed to the fix commit, which does not exist yet** (classified 2026-10-15 from the hunks, §5.3; 2 files, +74 / −2; parent `8489d08b9`; no `lib/` / `app/` / `packages/` / `components/` / `help/` hunk, no version stamp). NEW `docs/developer/bugs/bug-247-month-name-prototype-key.md` (71 lines): **Status OPEN**; **Found** "2026-10-13, the v5 port's differential harness (P4.D289, the `memory_time` family at the `83334b9a5` pin); reproduced 2026-10-14 at `8489d08b9`"; **Provenance "Original to v4"** — the named-date branch has read month words from a plain object literal since the episodic-spine `resolveWhenPhrase` (`8bf3cb5f`), moved unchanged into `lib/memory/time/clock.ts` by `e0ac7493a`; **Defect site** `clock.ts:94-97` (`MONTH_NAMES` is a `Record<string, number>` literal inheriting `Object.prototype`) and `:160-165` (`MONTH_NAMES[monthFirst[1]] !== undefined` / `MONTH_NAMES[dayFirst[2]] !== undefined` accept the inherited `constructor` — measured the ONE all-lowercase own property of `Object.prototype` that `[a-z]+` can reach), the throw escaping `lib/memory/extraction/run.ts:308` because `resolveEpisodicAnchors` sits OUTSIDE `admitDraft`'s per-candidate `try` (which guards only `admitCandidate`); `applyEpisodicFallbackAnchors` (`admission/prepare.ts:72-106`) is the one safe caller. Severity Low (needs a model to emit `when` as `"constructor 5"` / `"5 constructor"` / `"on constructor 5"` / `"constructor 5th"` / `"constructor 5, 2026"`); a turn job then FAILS whole and the queue retries up to three times (`queue-service.ts:522`), re-spending the cheap-LLM call, the OTHER pass and the `turn` cursor advance never happening. `WORD_NUMBERS` / `UNIT_DAYS` are read the same way but harmless (`0 < fn` false; the unit regex is closed). Proposed fix (NOT shipped): `Object.create(null)` / `Map` / `Object.hasOwn`, and moving `resolveEpisodicAnchors` inside the guard. `bugs.md` +3 / −2: the status paragraph gains "**247** is **open**", the row `| 247 | … | 2026-10-13 | — | Low | … | Pinned (\`PROTOTYPE_KEY_MONTH_THROWS\`) |`. **Still OPEN at v4 HEAD `362426c8f`** (no `bugs/fixed/bug-247-*`; the 2026-10-15 `bugs.md` status line still says "**247** is **open**"). The row's "v5 status" cell quotes the pin by name. **The message's one behavioural sentence ("toISOString throws") is WRONG and corrected by `809e5104d`** — the throw is `formatToParts` in `zonedParts`. | v5 `crates/quilltap-harness/tests/memory_time_equivalence.rs:317` — `PROTOTYPE_KEY_MONTH_THROWS: &[&str] = &["constructor 5", "5 constructor"]`, pinned BOTH ways (each phrase must still throw in v4 on every zone / anchor; v5 answers `{ occurredAt: null, window: null }`), asserted as `throws_measured == throws_table` ("drifted — v4's throwing phrases"); the lane record is **P4.D289** (the memory programme's PURE leaf family, `status-log.md:179561`); v5's closed month table in the `memory_time` port (`quilltap_core::memory_scoring` / the clock module). ⚠ When v4 FIXES 247, that commit is the **CONVERGENCE** row and this pin RETIRES by measurement (§5.4) — note the bug file lists FIVE shapes where the pin holds two, so the fix's oracle regen should widen the measured set before retiring. These two filings change nothing to compare. | **UNPROCESSED — past the `83334b9a5` round's pin** *(2026-10-16: bug 247 FIXED upstream in `25b8fab6a` — the CONVERGENCE row below.)* |
| `809e5104d` | 2026-10-14 | docs(bugs): bug 247 — name the real throw site (formatToParts in zonedParts, not toISOString) | **NO-PORT?** (classified 2026-10-15 from the hunks; 2 files, +4 / −3; parent `052659624`; docs only). `bugs/bug-247-month-name-prototype-key.md` +3 / −2 and `bugs.md` +1 / −1: the root-cause stack corrected — `zonedTimeToUtcMs(year, Object, day, 0, tz)` → `Date.UTC` coerces the `Object` function to `NaN`, the first-pass guess is `NaN`, `offsetMs` → `zonedParts` hands `new Date(NaN)` to `Intl.DateTimeFormat#formatToParts`, which throws `RangeError: Invalid time value` before `iso()` is reached (`clock.ts:167` → `calendar.ts:152` → `:134` → `:120`, measured at `8489d08b9`); the `bugs.md` row's "`toISOString` throws" becomes "`zonedParts`' `formatToParts` throws". No code, no behaviour. | None beyond `052659624`'s — the same `PROTOTYPE_KEY_MONTH_THROWS` pin (P4.D289); v5's own code comment at the pin should name `formatToParts`, not `toISOString`, if it quotes the mechanism. Ratify together with `052659624`. | **UNPROCESSED — past the `83334b9a5` round's pin** *(2026-10-16: bug 247 FIXED upstream in `25b8fab6a` — the CONVERGENCE row below.)* |
| `bbbcd2388` | 2026-10-14 | feat(wardrobe): copy to folder, and ask about components when filing outfits | **PORT-NEW + PORT — NO schema / migration / DDL move (verified: the `lib/schemas/wardrobe.types.ts` +23 / −2 hunk is REQUEST-body Zod only)** (classified 2026-10-15 from the hunks, §5.3; 24 files, +872 / −137; parent `809e5104d`; version stamp in both `package.json`s + the lockfile's root lines + the README badge; no `migrations/`, no `generateDDL`, no stored `WardrobeItem` field, no vault document field). **Schemas:** NEW `WardrobeFolderComponentModeSchema = z.enum(['move', 'copy', 'none'])`; `moveWardrobeItemsSchema` gains `components: WardrobeFolderComponentModeSchema.optional()`; NEW `copyWardrobeItemsSchema = { itemIds: z.array(z.string().min(1)).min(1, 'At least one item is required').max(500), folder: WardrobeFolderInputSchema, components: z.enum(['copy', 'none']).optional() }`. **Server:** NEW `lib/wardrobe/folder-filing.ts` (199 lines, server-only) — `fileItemsToFolder(repos, userId, location, { action: 'move' \| 'copy', itemIds, folder, components? })` → `{ items, componentsCarried }`; `componentMode = components ?? 'none'`; `copy` + `move` → `FolderFilingRequestError('Copying an outfit cannot move its components — the original outfit still needs them')` (a **400** via `writeErrorResponse` → `badRequest(error.message)`); **the plain path (`move` + `none`) delegates to the EXISTING `moveItemsToFolder(location, itemIds, folder)` and returns `componentsCarried: 0` with NO new log line** — so `move-items` without `components` is behaviourally unchanged except the response gains the key and the INFO line gains two fields; every other combination: `location.resolveFolder(folder)`, `uniqueIds` (Set-deduped), `location.readItems(true)`, missing ids → `WardrobeItemNotInLocationError(missing)` BEFORE any write, `components = collectContainerComponents(requested.filter(isComposite), wardrobeItems)` minus ids already in the request; `move`+`move` → ONE `moveItemsToFolder(location, [...uniqueIds, ...componentIds], target)` returning `moved.slice(0, uniqueIds.length)`; copies (`components === 'copy'` → the components; `action === 'copy'` → the components THEN the requested items): `idMap` of `randomUUID()`, `remap(ids)` = `idMap.get(id) ?? id`, `takenTitles` seeded from the whole wardrobe and grown per copy, `nextCopyTitle(original.title, takenTitles)` (the Duplicate helper — `(copy)` titles), `mountPointId = location.writableMountPointId()`, one `now`; per copy `carryItemImages(repos, { mode: 'copy', sourceItemId, destinationItemId: id, destinationMountPointId, userId })` → `fileIdMap`, then `createItem(location, { ...original, title, folder: target, componentItemIds: remap(original.componentItemIds) }, { preserve: { id, createdAt: now, updatedAt: now, archivedAt: original.archivedAt ?? null, imageFileId: fileIdMap.get(original.imageFileId) ?? null, migratedFromClothingRecordId } })` (the pre-existing `preserve` option of `item-mutations.ts:101-125`; an archived original yields an archived copy); `move`+`copy` → `location.updateMany(requested.map(item => ({ id, patch: { folder: target, ...(isComposite ? { componentItemIds: remap(...) } : {}) } })), { requireAll: true })`; DEBUG `[FolderFiling] Filed wardrobe items into folder` `{ scope, mountPointId, action, componentMode, folder, requested, componentsCarried, context: 'wardrobe' }`. `lib/wardrobe/expand-composites.ts` +31: `collectContainerComponents<T extends CompositeNode & { id }>(composites: readonly T[], containerItems)` — MOVED from `app/api/v1/wardrobe/transfers/route.ts` (−27 there) and generalised from one composite to many (`visited` seeded with every composite id; `componentItemIds ?? []`); the transfers route now calls `collectContainerComponents([source.item], source.containerItems)` — same result for one composite. `lib/wardrobe/routes/wardrobe-route-factory.ts` +42: `handleFileItems(action, body, ctx, params)` shared by `handleMoveItems` and NEW `handleCopyItems` (`copyWardrobeItemsSchema.parse`); INFO `` `${logTag} ${action === 'move' ? 'Moved' : 'Copied'} wardrobe items to folder` `` with NEW fields `components: body.components ?? 'none'`, `componentsCarried: result.componentsCarried` beside `count: result.items.length`; response `successResponse({ items: serializeWardrobeItems(result.items, location.origin, repos), componentsCarried })`; `'copy-items': handleCopyItems` added to the POST action table (so `?action=copy-items` on all four tiers; the module doc lists it after `move-items`). **Client (v4 React):** `lib/hooks/use-wardrobe-folders.ts` — `WardrobeFolderComponentMode`, the mutation's `action` union + `'copy-items'`, `fileItems(mode, container, itemIds, folder, components?)` → `moveItems` / NEW `copyItems` (body `{ itemIds, folder, ...(components ? { components } : {}) }`); NEW `components/wardrobe/WardrobeComponentModeFieldset.tsx` (83) — `ComponentMode`, `defaultComponentMode(mode)` (`move` → `'move'`, `copy` → `'copy'`), radios for a move `Move the components along with it` / `Copy the components (originals stay behind)` / `Leave the components behind`, for a copy `Copy the components along with it` / `Copy the outfit alone`, the all-or-nothing footnote in two wordings (transfer: "Only components living in the same wardrobe travel…"; folder: "Only components filed in this wardrobe travel…"); `WardrobeFolderPickerDialog.tsx` +93 / −? gains `mode`, the fieldset when outfits are among the items (legend `This outfit bundles N component(s)` / `N of these are outfits, bundling M components between them`), `onSubmit(folder, components?)` with `components` set only when outfits were present, the current folder disabled ONLY for a move (`mode === 'move' && isSameFolder`), verb `Move` / `Copy`; `WardrobeTransferDialog.tsx` −? swaps its radio group for the fieldset; `WardrobeBrowser.tsx` `fileToFolder` (copy sends `components === 'move' ? undefined : components`), the select bar's `Copy to folder…`, toasts `A copy of "T" now hangs in …` / error fallback `The items would not be copied`; `wardrobe-item-row.tsx` `onCopyToFolder`; `useWardrobeItemActions.ts` `openFolderPicker(items, mode = 'move')`, `TransferRequest.mode`. **Help:** `help/wardrobe.md` +6 / −? (the `Copy to folder…` bullet, `Outfits ask after their components`, the select bar now `N selected · Move to folder… · Copy to folder… · Cancel`) and `help/project-wardrobe.md` 1 line — the help file COUNT is unchanged (131 at `4a38222e6`, 131 at HEAD). Docs: `API.md` (+`copy-items` row, `move-items` row rewritten), `features/complete/wardrobe-folders.md`, CHANGELOG. Tests: `folder-filing.test.ts` 207 (8 cases incl. the refused copy-moves-components and the refuse-before-write), route factory +40 (2 cases: `componentsCarried` 1; `Cloak (copy)` in `Formal` with a fresh id), the folders jsdom +53. **Message vs hunks: consistent** — every claimed behaviour has its hunk; the one thing the prose under-states is that `move-items`'s RESPONSE SHAPE changes for every caller (`componentsCarried` always present). | v5 core: `crates/quilltap-core/src/api/wardrobe_route_factory.rs` (**P4.D281 unit 2** — the five folder actions; `move_items_to_folder` at `:1057`, INFO `Moved wardrobe items to folder` at `:1067` pinned byte-wise incl. a `generalId=null userId=…` literal at `:1640`) — needs a `copy-items` arm, the shared file-items path, the two new INFO fields and `componentsCarried` on the wire; `api/types.rs` `WardrobeItemsMove` + a NEW copy arm and a `COLLAPSE_CENSUS` row; `quilltap-web/src/wardrobe_routes.rs` + `characters_routes.rs` `COLLECTION_POST_ACTIONS` literal `["instructions", "create-folder", "rename-folder", "delete-folder", "move-items"]` (P4.D281 unit 5); families `wardrobe_folders_routes_equivalence` (web, over v4's REAL four route files), `characters_wardrobe_route.rs`, `wardrobe_instructions_routes.rs`, `dispatch_wrong_type_census.rs` — the move-items rows' response bytes WILL trip at the baseline move (the extra key); `services/wardrobe_transfers.rs:410 collect_container_components` (**P4.D281 unit 3**) — v4's twin moved to `expand-composites.ts` and took a slice of composites; `services/wardrobe_item_mutations.rs` `create_item` + `CreateItemOptions.preserve` / `PreservedIdentity` (P4.D279 keystone, used by transfers), `services/wardrobe_item_images/carry.rs` (P4.D263 / P4.D281 — the copy mode), the Duplicate `(copy)`-title helper, `db/wardrobe_location.rs` (`resolve_folder` / `read_items` / `writable_mount_point_id` / `update_many` — P4.D279), `wardrobe_folders.rs` `WardrobeItemNotInLocation` (P4.D281). SPA: `apps/web/src/app/wardrobe/wardrobe-folder-picker-dialog.ts` (**P4.D285 unit 5**, SPA 0.5.840 — bug 229's shape), `wardrobe-transfer-dialog.ts` (**P4.D286** — its radio group becomes the shared fieldset), `wardrobe-browser.ts`, `wardrobe-item-actions.ts` (`openFolderPicker`), `wardrobe-folders.api.ts` (`move-items` literal; needs `copy-items`), the item row. Help: `help/wardrobe.md` + `help/project-wardrobe.md` re-vendor (p4.9i2 / **P4.D287**'s tree, byte-equal to `4a38222e6` today; count 131 → 131); `docs/v4/` API.md + the wardrobe-folders spec. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `b82037d75` | 2026-10-14 | fix(scriptorium): queue chunk embeddings from the pre-write read (bug 248) | **PORT — bug 248 FIXED the day it was filed; NOT a convergence (Original to v4, found on Ignite, `v5 status: Not assessed`); NO schema move** (classified 2026-10-15 from the hunks, §5.3; 11 files, +281 / −36; parent `bbbcd2388`; two production files, two tests, the bug file (`bugs/fixed/bug-248-render-embed-one-behind.md`, 81 lines), `bugs.md`, CHANGELOG, version stamps). **Provenance:** "Original to v4. The post-upsert re-read dates from Scriptorium Phase 1 (`a76240c27`, 2026-04-11); moving job handlers into the forked child made that read stale. Bug 17's fix (`62ab1bc8f`) made a content change clear the vector" — the defect is the Node job child's BUFFERED writes (a job cannot read its own writes), a mechanism v5's single-writer never had. Symptom: Ignite 108 of 1,634 `conversation_chunks` with `embedding IS NULL` after the upgrade's 98-job bulk re-render, 106 of them with an `embedding_status` row saying `EMBEDDED`; Friday's new chunks embedded 4–50 minutes late (at the NEXT render). **Hunks:** `lib/background-jobs/handlers/conversation-render.ts` (+71 / −?) — step 3 now reads `existing = repos.conversationChunks.findByInterchangeIndex(chatId, interchange.index)` BEFORE each `upserted = upsert({...})`, `needsEmbedding = payload.fullReembed \|\| !existing \|\| !existing.embedding \|\| existing.content !== interchange.content`, `chunkId = existing?.id ?? upserted?.id`, `if (needsEmbedding && chunkId) toEmbed.push(chunkId)`; NEW unconditional DEBUG `[ConversationRender] Chunks upserted` `{ jobId, chatId, interchangeCount, needingEmbedding: toEmbed.length, fullReembed: !!payload.fullReembed }`; step 4's gate becomes `toEmbed.length > 0` (was `result.interchanges.length > 0`), the post-upsert `findByInterchangeIndex` re-read loop and `embeddedCount` are DELETED, the enqueue iterates `toEmbed` with the same `{ entityType: 'CONVERSATION_CHUNK', entityId, chatId, profileId: defaultProfile.id }`; NEW DEBUG `[ConversationRender] No default embedding profile; chunks left for the startup reconcile` `{ jobId, chatId, pending: toEmbed.length }` in the no-default-profile arm. `lib/database/repositories/conversation-chunks.repository.ts` (+17 / −1) — `upsert`'s insert arm destructures `{ id: suppliedId, ...data }` and calls `_create(data, typeof suppliedId === 'string' ? { id: suppliedId } : undefined)` (the id kept OUT of the row data; the base repository mints one when none is supplied); NEW bare DEBUG `Creating conversation chunk` `{ chatId, interchangeIndex, suppliedId: <boolean> }`; `logger` imported. Tests pin: a changed chunk queued despite the stale vector (`chunk-1`, never the `synthetic-id`), a new chunk queued under the upsert's id (`new-1`), the interleave `['read', 'write', 'read', 'write']`, and the insert keeping / minting the id. **Under read-your-writes the per-chunk predicate is EQUIVALENT to the old one** (the upsert NULLs the vector exactly on a content change — bug 17 — so `!chunk.embedding` after the write ⇔ `!existing \|\| !existing.embedding \|\| content changed` before it), so the behavioural delta for v5 is: the read/write INTERLEAVE (a SQL-trace-visible order change), a chunk absent before AND unreturned by the upsert is skipped (was: skipped), the gate on `toEmbed` (no profile lookup when nothing needs embedding — the `findEmbeddingProfiles` read disappears for a no-op render), and the three new DEBUG lines. **The message's data claims ("108 on Ignite", "one render late") describe v4's child, not a hunk; v5 never had the lag.** | v5 `crates/quilltap-core/src/services/conversation_render_job.rs` (**P4.6BM unit 2** — the `CONVERSATION_RENDER` handler, `status-log.md:46281`): faithful to v4's PRE-fix `:46-98` — the upsert loop (`:166-186`, `db.write(... .upsert(&input, &new_id, &now_for_write))`) THEN `enqueue_embeddings` (`:217-262`) re-reading `find_by_interchange_index` and gating on `payload.full_reembed \|\| !chunk.has_embedding`, the whole block caught (`[ConversationRender] Failed to enqueue embedding, continuing` — the `an_enqueue_failure_warns_with_v4s_field_names` test); `db/conversation_chunks.rs:591 upsert(input, new_id, now_iso)` ALREADY takes the caller's id (the chunk-on-write round **P4.6BK**, `status-log.md:5580`; the bug-17 `embedding_remainder` family **P4.D51 / P4.D52**, `status-log.md:64431`), so the repository half is a log line only; `services/conversation_render_reconcile.rs` (P4.6BM unit 3 — unchanged by v4); `quilltap-host/src/spine.rs` job registration; `generators/refresh_archive.rs` / `services/chat_admin.rs` / `queue_service.rs` (`fullReembed` producers — unchanged); the `embedding_remainder_equivalence` harness family and any production-SQL-capture plan that pins the render job's statement ORDER. Port = the pre-write read + predicate + `existing.id ?? new_id`, the `toEmbed` gate, the three DEBUG lines (two `[ConversationRender]`, one bare `Creating conversation chunk`). | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `3f64d3a6c` | 2026-10-14 | docs(README): Updated README and broke out feature docs | **NO-PORT? (docs only — but it moves `docs/**`, so the mirror grows)** (classified 2026-10-15 from the hunks, §5.3; 5 files, +190 / −214; stamp unchanged at `b82037d75`'s `4.10.0-dev.185`, **parent `b82037d75` (bug 248 — "fix(scriptorium): queue chunk embeddings from the pre-write read", 11 files, +281 / −36) — NOT `8489d08b9`; that commit is outside this batch and needs its own row**). `README.md` −211 / +61: the whole "Core Features" section (AI Desktop → Image Generation, incl. the four `**bold**` accretion paragraphs — the per-character vault, archives, the Concierge's four-state description, "Memory protection now favors…"), "Where your data lives / What's stored / Backup options", "Architecture at a glance / Getting started / Contributing", "Tech Stack", "Support", the long Troubleshooting block REMOVED; NEW "Your First Ten Minutes", "A Tour of the House" (one paragraph each for the Salon, Aurora, the Commonplace Book, the Scriptorium, Prospero, the Concierge, the Host, Pascal, the Lantern, and a Librarian/Carina/Ariel line), "Providers", "Themes", "Troubleshooting & Support". NEW `docs/FEATURES.md` (119 lines, "A Tour of the House: Quilltap's Features in Full") holds the displaced sections with the "now" phrasing removed. **Corrects two of v4's own false claims — the ones CLAUDE.md's standing rule warns about:** every "SQLCipher" in the README is gone ("encrypted at rest with ChaCha20-Poly1305 (via SQLite3 Multiple Ciphers)"; Tech Stack "better-sqlite3-multiple-ciphers" with the parenthetical dropped) and "with WAL mode" is gone (TRUNCATE default, WAL opt-in); `CLAUDE.md`'s CLI note now reads "Databases are encrypted (ChaCha20 via SQLite3 Multiple Ciphers — not SQLCipher, despite the identifiers)". It does **NOT** touch `docs/developer/DATABASE_ENCRYPTION.md`, which still asserts SQLCipher. `CLAUDE.md` + `.claude/commands/update-documentation.md` gain the README / `docs/FEATURES.md` index rows. No code, schema, migration, `help/`, dependency or bug-file hunk; **NO version-stamp hunk at all** (no `package.json`, no badge line — `4.10.0-dev.185` is `b82037d75`'s stamp, measured; this commit is the first in the batch that does not bump). | v5 mirrors v4's `docs/**` under `docs/v4/` (+ `packages-quilltap-README.md`) but NOT the root `README.md` or `CLAUDE.md` — so the ONLY v5 effect is a NEW `docs/v4/FEATURES.md` at the next re-vendor (P4.D303's recipe: `git ls-tree … docs/` → `docs/v4/${f#docs/}`; the `find docs/v4 -type f` count +1) and `docs/v4/CHANGELOG.md`. The ChaCha20 correction is v4 converging on THIS port's Phase-0 cipher finding (`phase-0.md`; CLAUDE.md "The database cipher is ChaCha20/sqleet, NOT SQLCipher") — a documentation convergence worth a one-line note in `phase-0.md` / the standing rule ("v4's README and CLAUDE.md now say so too, as of `3f64d3a6c`; `DATABASE_ENCRYPTION.md` still does not"), not a code row. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `2432b9057` | 2026-10-14 | release: 4.10.0 | **NO-PORT? — the release squash** (classified 2026-10-15 from the hunks, §5.3). First parent `8fbf2afe0` = "release: 4.9.2" (2026-09-06), so `git diff 2432b9057^ 2432b9057 --stat -- lib app packages plugins` (1,013 files / +215,058 / −75,729) is the ENTIRE 4.10.0 dev cycle — 2,740 main commits, each already a row here or in §6 — and NOT new content; against main's pre-merge tip `3f64d3a6c` the squash differs in exactly FIVE stamp files (`README.md` badge, `docs/CHANGELOG.md`, `docs/releases/4.10.0.md`, `package.json` + `package-lock.json`: `4.10.0-dev.N` → `4.10.0`). No `lib/` / `app/` / `packages/` / `plugins/` hunk is new relative to main. **`release` tip = `2432b9057`** (`merge-base main release` = itself). **Bugfix measured by CONTENT (§4 step 2):** tip `489b228b6` "bugfix: started 4.10.0 bug branch" (2026-10-14, parent `1a2b2164c` the 4.9.2 bug-branch start); `git diff 2432b9057 bugfix -- lib app packages plugins` EMPTY, all paths = 3 stamp files (`4.10.0` → `4.10.1-bugfix.0`); `git diff main bugfix -- lib app packages plugins` = 26 files / +278 / −998 = exactly main's post-release `7458ff94e` + `362426c8f` seen backwards. **Bugfix carries ZERO unabsorbed content.** ⚠ §4 step 2's "still no `release: 4.10.0` squash" and CLAUDE.md's baseline bullet are now stale — re-probe BOTH branches as before, but the squash exists. | None (no code). The port's version strings are its own (`quilltap-core` 0.0.x etc.); nothing reads v4's `package.json` version. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `f573a7576` | 2026-10-14 | merge: 4.10.0 back into main | **NO-PORT?** (classified 2026-10-15 from the hunks). A two-parent merge (`3f64d3a6c` + `2432b9057`); `--stat` 5 files / +6 / −6 — the same five stamp files; the combined diff (`git show f573a7576`) is EMPTY. No code, no schema, no dependency move. | None. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `37246654d` | 2026-10-14 | dev: started 4.11.0 development | **NO-PORT?** (classified 2026-10-15 from the hunks). 4 files / +6 / −4: README badge `4.10.0` → `4.11.0--dev.0`, `docs/CHANGELOG.md` +2 (an empty `### 4.11-dev` heading), `package.json` + lock `4.11.0-dev.0`. No code. From here v4 stamps `4.11.0-dev.N`. | None. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `1e1712a12` | 2026-10-14 | docs(CHANGELOG): moved 4.9.x changes to v4 docs | **NO-PORT?** (classified 2026-10-15 from the hunks). 2 files / +57 / −57: the `### 4.9.2` section (bugs 124 / 125, both ABSORBED long since) moved verbatim from `docs/CHANGELOG.md` to `docs/CHANGELOG_V4.md`. No code. (The sibling `7458ff94e` moves 3,600 more lines the same way.) | `docs/v4/` mirror only, IF it carries `CHANGELOG*.md` — re-vendor with the next `help/` + `docs/v4/` lane (the `p4.9i2` / P4.D278 / P4.D287 shape). | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `7458ff94e` | 2026-10-14 | fix(wardrobe): mark a styled hair slot as overriding the natural hair | **PORT — a model-visible TEXT move on a ported pure function; the ±3,600 CHANGELOG hunk is a docs MOVE; NOT a convergence (no bug number)** (classified 2026-10-15 from the hunks, §5.3; 10 files, +3,630 / −3,606 — 7,200 of them `docs/CHANGELOG.md` → `docs/CHANGELOG_V4.md`; **parent `1e1712a12`, NOT `b82037d75`** — five commits sit between, incl. `2432b9057` `release: 4.10.0` and `37246654d` `dev: started 4.11.0 development`; this commit is the first `4.11-dev` entry; `4.11.0-dev.*` stamps in both `package.json`s + the lockfile + the README badge). **Code (18 lines):** `lib/wardrobe/slot-guidance.ts` +11 — NEW `export const HAIR_STYLING_PRECEDENCE_NOTE = "current styling — overrides the natural hair's texture and style in the physical description"` beside the three existing `HAIR_*` constants (`HAIR_SLOT_GUIDANCE`, `HAIR_PHYSICAL_BOUNDARY`, `HAIR_PHYSICAL_DESCRIPTION_NOTE` — unchanged); `lib/wardrobe/outfit-description.ts` +7 / −1 — in `describeOutfit`'s grouped-slots loop `const note = slotsForValue.includes('hair') ? \` (${HAIR_STYLING_PRECEDENCE_NOTE})\` : ''` appended: `` `- **${slotsForValue.join(', ')}:** ${value}${note}` `` — so `- **hair:** braided crown, silver pins (current styling — overrides the natural hair's texture and style in the physical description)`; the note rides ANY group containing `hair` (a multi-slot item equipped across `hair` + another slot puts it on the joined `**accessories, hair:**` line); the naked collapse, the empty-hair silence (rule 13 — no negative-space line) and the garment fallbacks are untouched (hair has no fallback, so an empty hair slot never carries the note). The doc comment gains one bullet. Reaches every `describeOutfit` caller with NO caller hunk: `lib/wardrobe/item-image-prompt.ts`, `lib/wardrobe/resolve-equipped.ts`, `lib/image-gen/appearance-resolution.ts`, `lib/image-gen/avatar-prompt.ts`, `lib/tools/handlers/wardrobe-handler-shared.ts`, `lib/memory/cheap-llm-tasks/image-scene-tasks.ts`, `lib/services/aurora-notifications/writer.ts` — i.e. image prompts, the wardrobe tools' output, Aurora's announcements, the Lantern, and (transitively, since the catalogue quotes the prompt) kept-photo catalogues, exactly the message's list; the message's "kept-photo catalogues" has no hunk of its own and is true only through the prompt text. Test: `outfit-utils.test.ts` two expectations now interpolate the constant. **Help:** `help/wardrobe.md` +3 / −1 — "Three consequences follow" → "Four", NEW bullet "**A styled coiffure outranks the natural hair.** … the hairdo is marked as the current styling, taking precedence over the natural texture and style…" (count 131 unchanged). **Docs:** the 4.10.0 CHANGELOG section (bug 248, the copy-to-folder entry, release checklists 10–13…) moves whole into `CHANGELOG_V4.md`, continuing `1e1712a12`'s split; the `4.11-dev` heading gains "Fix: styled hair slot read as contradicting the physical description". **NO schema / migration / dependency move.** | v5 `crates/quilltap-core/src/wardrobe.rs:309 describe_outfit` (**P4.D87 unit 1** — the hair slot's core half, `status-log.md:76258`; re-homed into the keystone by **P4.D279**): the `OrderedGroups` loop at `:72-73` `format!("- **{}:** {}", slots_for_value.join(", "), value)` needs the hair-group note; the `HAIR_*` constants block at `wardrobe.rs:144-169` (P4.D87; the duplicate strings folded at `status-log.md:90934`) gains a fourth constant; `generators/field_semantics.rs` (reads `HAIR_PHYSICAL_DESCRIPTION_NOTE` — unchanged). Callers whose prompt / tool-output pins WILL trip: `tools/wardrobe_wear.rs`, `tools/wardrobe_take_off.rs` (**P4.D280** — the wear text), `services/resolve_equipped.rs` (**P4.D282**), `services/appearance_resolution.rs`, `services/wardrobe_item_image_prompt.rs`, `services/image_scene_tasks.rs` (**P4.D283** — note P4.D283's review already re-shaped the appearance prompt to v4's struct), the Aurora writer; `tests/unreported_if_blank_slots.rs` and the tier-1 `describe_outfit` family (the P4.d5 ∥ P4.d6 ∥ P4.6ay round, `status-log.md:13025`) are the direct oracle pins. Help: `help/wardrobe.md` re-vendor (p4.9i2 / **P4.D287** tree). No SPA surface renders `describeOutfit`. | **UNPROCESSED — past the `83334b9a5` round's pin** |
| `936a34720` | 2026-10-15 | docs(features): spec Pascal wardrobe effects, trigger phrases, and composer slash commands | **NO-PORT? — docs-only, VERIFIED** (classified 2026-10-15 from the hunks; `git show --stat 936a34720 -- lib app packages plugins components help public migrations` → EMPTY). 5 files / +843 / 0: three NEW proposals — `docs/developer/features/pascal-trigger-phrases.md` (305; a `triggers` key + an off-by-default `chatSettings.customToolTriggers`, the scanner / runner at the two auto-RNG sites, the prerequisite one-run-service extraction — IMPLEMENTED by `362426c8f` an hour later, which moves it to `complete/`), `pascal-wardrobe-effects.md` (266; `effects[].wardrobe` wear / take_off / clear_slot through `wearItem` / `takeOffItem` / `applyDisplacement`, one `notifyWardrobeChanged` per run, audit in `pascalMeta.effects` — the checkout's UNCOMMITTED dirt, in flight), `composer-slash-commands.md` (262; `/` commands in the Salon composer over composer actions, built-in tools and Pascal's roster — supersedes Phase 4 of `composer-typeahead.md`; UNIMPLEMENTED) — + the catalogue + a `4.11-dev` changelog line. A planning input, not a port: two of the three specs become PORT-NEW rows when their code lands. | `docs/v4/` mirror (`docs/developer/features/`). Forewarning for: the Pascal server family (P4.6ay / P4.d10 / P4.6be / P4.D169 — `pascal/side_effects.rs`), the wardrobe keystone (P4.D279 `wear-ops`), the composer typeaheads vertical (the SPA composer lane). | **UNPROCESSED — past the `83334b9a5` round's pin** *(2026-10-16: the wardrobe-effects spec SHIPPED as `f0db8cbb8`, moved to `features/complete/`.)* |
| `362426c8f` | 2026-10-15 | feat(pascal): trigger phrases run custom tools when their words are said | **PORT-NEW + PORT — WITH A D23 MOVE (the NEXT re-dump after the NINTH, `4ae0c4ca6`; ONE new migration)** (classified 2026-10-15 from the hunks, §5.3; 58 files, ~+2,100 / −430; `4.11.0-dev.1` → `.3`; parent `936a34720`; 11 test files + the Almanack snapshot are ~a third). **SCHEMA / MIGRATION — flag loudly:** NEW `migrations/scripts/add-custom-tool-triggers-field.ts` (registered in `index.ts` import + `migrations[]` + re-export, three hunks; `id: 'add-custom-tool-triggers-field-v1'`; `introducedInVersion: '4.11.0'`; `dependsOn: ['add-custom-tools-field-v1']`; `shouldRun` = SQLite ∧ `sqliteTableExists('chat_settings')` ∧ `!sqliteColumnExists('chat_settings','customToolTriggers')`; `run` = `addColumnIfMissing('chat_settings', 'customToolTriggers', 'INTEGER DEFAULT 0')`, no backfill; INFO `Added customToolTriggers column to chat_settings table` — logged TWICE on a real add, once per-column inside the `if` with `{context}` and once after with `{context, columnsAdded, durationMs}` — ERROR `Failed to add customToolTriggers column`, `context: 'migration.add-custom-tool-triggers-field'`; result `message: 'Added customToolTriggers column to chat_settings table'` / `'Failed to add customToolTriggers column'`; `prettify.ts` label "Teaching Pascal which words to listen for" — NO-PORT per the standing prettify ruling). `lib/schemas/settings.types.ts` `ChatSettingsSchema` gains `customToolTriggers: z.boolean().default(false)` IMMEDIATELY after `customTools` and before `compositionModeDefault`, so v4's `generateDDL` renders a fresh `chat_settings` with `"customToolTriggers" INTEGER DEFAULT 0` between `"customTools" INTEGER DEFAULT 1` and `"compositionModeDefault" INTEGER DEFAULT 0` (v5's `fresh_schema.json` `main` has `customTools` at that very spot, index 20 — `chat_settings_seed.json` columns 20/21) — **`fresh_schema.json` must be RE-DUMPED from a pin at or past `362426c8f` (D23, never by hand), `chat_settings_seed.json` gains `customToolTriggers: 0`, and the migration is re-homed as an UNSTAMPED table-gated boot ensure beside the `customTools` one** — the `impersonation-voice-mode` / `wardrobe-image-settings` repair shape (`db/chat_settings_*_repair.rs`), differentially compared against v4's REAL migration (`table_info` + `sqlite_master`), and added to `restore_vintage_state`'s boot-align list (the memory note). `DDL.md` +1 `chat_settings` line, +`invokedBy` / `triggerPhrase?` prose on `chat_messages.pascalMeta` ("ride the JSON column with no migration"). `chat-settings.repository.ts` default row `customToolTriggers: false`. **No dependency move** (`package.json` / lock / `packages/quilltap/package.json` stamps only — NO CLI code hunk; Tier R untouched). **NOT a convergence** (no bug number). **(a) The definition — PORT-NEW on `QtapCustomToolSchema`** (`custom-tool.types.ts` +51): `MAX_TRIGGERS = 8`, `MIN_TRIGGER_PHRASE_LENGTH = 3`, `MAX_TRIGGER_PHRASE_LENGTH = 80`; `triggers: z.array(z.string().min(3).max(80)).min(1).max(8).optional()` placed after `chipLabel` / before `description`; a FOURTH superRefine `validateTriggers`: `trigger N has leading or trailing whitespace` (`phrase !== phrase.trim()`, `path: ['triggers', i]`) and `trigger N repeats trigger M (phrases match regardless of case)` (`toLocaleLowerCase` fold). `public/schemas/qtap-custom-tool.schema.json` +8 (`triggers`: array, `minItems 1`, `maxItems 8`, `uniqueItems`, items `minLength 3` / `maxLength 80`, the long description ending "Never sent to the model.") → v5's `apps/web/public/schemas/qtap-custom-tool.schema.json` byte-copy (the standing rule). The dice specimen `CUSTOM_TOOL_SPEC_DICE.json` gains `"triggers": ["make a saving throw", "roll to save"]` + a `Demonstrates:` sentence on the second outcome. `workbench.ts` `CustomToolLibraryEntry.triggerCount = definition.triggers?.length ?? 0` (a NEW wire key on the library response). `tool-draft.ts` +59: `ToolDraft.triggers: string[]` (round-trip `draftFromDefinition` / `definitionFromDraft` — emitted only when non-empty), a NEW `DraftIssue.where` arm `{ section: 'triggers'; index? }`, `validateDraftTriggers` (the loader's four rules as errors in the draft's own words — `at most 8 trigger phrases`, `a trigger phrase needs at least 3 characters`, `… is at most 80 characters`, `a trigger phrase must not begin or end with a space`, `"X" is listed twice (phrases match regardless of case)` — plus the SELF-ECHO warning `${what} contains the trigger "${phrase}" — a character who repeats Pascal's line will set this tool off again` over every outcome message, the chip label and, when `llmEnabled`, the consult prompt). **(b) The scanner — NEW `lib/pascal/trigger-phrases.ts`** (92, pure, client-safe): `MAX_TRIGGER_RUNS_PER_MESSAGE = 4`; `foldTriggerText = toLocaleLowerCase` (the mention ranker's fold — v5's `str::to_lowercase` byte-identity ruling applies); `triggersContainedIn(text, triggers)`; `scanTriggerPhrases(content, tools)` → `{ hits, dropped }`: strips Carina query lines FIRST (NEW `stripCarinaQueryLines` in `lib/chat/carina-parser.ts` +72 — `parseCarinaQuery`'s loop body extracted into `parseQueryLine(line)`, `stripCarinaQueryLines` filters EVERY line that parses as a query, `split(/\r?\n/)…join('\n')` — behaviour of `parseCarinaQuery` itself unchanged), folds, `indexOf` per phrase, ONE hit per tool (its earliest phrase), stable `sort` by index (ties keep roster order), `slice(0, 4)` / the rest `dropped`. Plain substring — "fire" matches "fireplace" — no boundaries. **(c) The runner — NEW `lib/pascal/trigger-runner.ts`** (241): `triggersEnabled(cs) = cs?.customTools !== false && cs?.customToolTriggers === true`; `runTriggeredCustomTools` NEVER throws (WARN `Trigger-phrase scan failed; no custom tools fired` `{context:'pascal.trigger-runner', chatId, speakerParticipantId, error}`); speaker perspective — a character seat loads `characters.findById` (`metadata`, `characterDocumentMountPointId`; WARN `Trigger-phrase scan skipped — speaking character not found` / `… unreadable`), the bare operator rolls as nobody (`{}` metadata, null mount); `resolveCustomToolRoster` per call, never cached (gates + `disabled` honoured by the roster); over-cap WARN `Trigger phrases over the per-message cap; later tools not fired` `{fired, dropped}`; `whisperTargetIds = speaker.participantId ? [it] : [userId]`; per hit `runCustomTool({ parameters: null, private: undefined, invokedBy: 'trigger', callerParticipantId: speaker.participantId, triggerPhrase: hit.phrase, nowMs })` — ONE `Date.now()` for the whole message; a refused run → DEBUG `Triggered custom tool refused` + `postProsperoCustomToolError({ toolName, reason, whisper, callerParticipantId: whisperTargetIds[0] })` and CONTINUE; `posted === null` → DEBUG `Triggered custom tool ran but its announcement did not land` + Prospero `the outcome could not be posted to the chat`; success → `onPosted(msg)` + INFO `Custom tool fired by trigger phrase` `{tool, phrase, speakerParticipantId, invokedBy:'trigger', state}`. **(d) The sites — NEW `lib/services/chat-message/trigger-phrase-sites.ts`** (107): `runUserMessageTriggers` (returns `[]` on `isContinueMode` or empty content; seat = `resolveRespondingParticipant`'s — a user-driven / impersonated CHARACTER seat speaks as that character; `existingMessages.push(...triggered)` — the in-memory splice so the responding model hears Pascal) and `runReplyTriggers` (the character's own perspective; NO splice — a chained next speaker re-reads the transcript). `orchestrator.service.ts` +25 calls `runUserMessageTriggers` AFTER the user message is persisted and the RNG detector, BEFORE the Carina user-message path (the dice land BEFORE the message; Pascal's announcement must FOLLOW it); `message-finalizer.service.ts` +26 calls `runReplyTriggers` after the saved reply (`cleanedResponse`), BEFORE the done event and the Carina assistant-markup path; both `onPosted = safeEnqueue(controller, encodePascalResultEvent(encoder, msg))` — the `pascalResult` SSE event (pre-existing at `streaming.service.ts:870`) now ALSO emitted from the finalizer. Swipes never reach either site (`regenerate-swipe.service.ts`) — fires nothing, matching the dice. **(e) The extraction — PORT (a refactor of two ported entrances):** NEW `lib/pascal/run-service.ts` (209) `runCustomTool(params) → { ok: true, result, posted, appliedEffects } | { ok: false, reason, whisper }` — cascade read ONCE (`params.chat ?? chats.findById`; `groupScope` = `{kind:'character'}` or `{kind:'none'}`; DEBUG `Custom tool run: state cascade unavailable, defaulting to {}`), `flattenProgressions(metadata, nowMs)`, `executeCustomTool` (`CustomToolRunError` → `{ok:false}` with DEBUG `Custom tool run refused` `{invokedBy}`; anything else rethrown), `applyCustomToolEffects` BEFORE the post, `buildPascalResultContent` + `postPascalResult` with the full `pascalMeta` (now `invokedBy: params.invokedBy`, `…(invokedBy === 'trigger' && triggerPhrase ? { triggerPhrase } : {})`, `callerParticipantId` when set), DEBUG `Custom tool dealt` `{tool, tier, invokedBy, state, effects, whispered, posted}`. `run-custom-handler.ts` −138 / +~30: the cascade / progress / execute / effects / post block REPLACED by one `runCustomTool({ …, whisperTargetIds: callerParticipantId ? [it] : null, invokedBy: 'llm' })`; `reportFailure(context, toolName, outcome.reason, outcome.whisper)`; the handler's OWN log lines (`run_custom: state cascade unavailable…`, its refused line) are GONE — the run service's replace them (a ported capture pin on those texts WILL trip). `app/api/v1/chats/[id]/custom-tools/route.ts` −173 / +~40 likewise → `runCustomTool({ chat, entry, parameters: body.parameters ?? null, private: body.private, characterId: body.asCharacterId ? perspective.characterId : null, metadata: body.asCharacterId ? perspective.metadata : {}, whisperTargetIds: [ctx.user.id], invokedBy: 'user' })`; the route's `Manual custom-tool run: state cascade unavailable…` line GONE (the run service's `Custom tool run: state cascade unavailable…` replaces it), while its `Manual custom-tool run refused` DEBUG line SURVIVES on the `{ok:false}` arm (`{context: HANDLER, chatId, tool: body.tool, reason: outcome.reason}`) followed by the Prospero post + `badRequest(outcome.reason)`; `callerParticipantId: null` passed explicitly. The handler likewise keeps `reportFailure(context, toolName, outcome.reason, outcome.whisper)` on `{ok:false}` and a `catch` for rethrown non-`CustomToolRunError`s (`whisper = isPrivate ?? defaultVisibility === 'whisper'`). **(f) `pascalMeta.invokedBy` widens** `z.enum(['llm','user'])` → `['llm','user','trigger']` + `triggerPhrase: z.string().optional()` in BOTH `MessageEventSchema` (`chat.types.ts`) and `ChatMessageRowSchema` (`chats-messages.ops.ts`) — JSON-column only; `.qtap` export schema NOT touched by this commit (the dirt touches it for `effects`). ⚠ LIVE-DATA: a v5 at the baseline parsing `pascalMeta` with the two-value enum would DROP / refuse rows v4 4.11-dev writes with `'trigger'` (restore / import / read) — the §1 hazard rule (dogfood only a copy taken BEFORE v4 ran past this) already covers it; and v4's boot ALTERs live `chat_settings`. **(g) Settings plumbing — PORT:** `app/api/v1/settings/chat/route.ts` PUT accepts `customToolTriggers?: boolean` (non-boolean → `Error('Invalid customToolTriggers value (must be boolean)')`, the route's 400 arm; GET echoes it); `help-settings-handler.ts` `fetchCategorySettings` adds `customToolTriggers` to the chat bag (the `help_settings` tool output GROWS — `help_tools_equivalence` will trip at the pin, the P4.d6 `customTools` precedent exactly); Almanack `types.ts` `FeatureConfigInfo.customToolTriggers: boolean`, `phase3-ledgers.ts` default `false` + `chatSettings?.customToolTriggers ?? false`, `render.ts` +1 line `- **Auto-Run on Trigger Phrases**: ${yesNo(...)}` under "Pascal's Workbench (Custom Tools)" (snapshot +1). **(h) SPA (React → Angular):** NEW `components/custom-tools/TriggerPhrasesField.tsx` (110; chip list, `aria-label="Trigger phrases"`, `Remove trigger phrase ${phrase}`, placeholder `abracadabra` / `At most 8 phrases`, Enter adds); `BuilderForm.tsx` mounts it; `WorkbenchLibrary.tsx` badge `✦ N phrase(s)` (title "Listens for trigger phrases — said in a chat with the setting on, they run this tool") when `triggerCount > 0`; `AutomationSettings.tsx` rewritten as a row table over `AUTOMATION_OPTIONS` (`autoDetectRng`, `customToolTriggers` — the latter `disabled: !customToolsOn`, `opacity-50 cursor-not-allowed`); `types.ts` option `label: 'Auto-Run Custom Tools on Trigger Phrases'`, `description: 'When anyone in a chat says a phrase one of Pascal's custom tools declares, run that tool automatically with its defaults. Requires Custom Tools.'`, `DEFAULT_CUSTOM_TOOL_TRIGGERS = false`, `AutomationKey`; `useChatSettings.ts` `handleCustomToolTriggersChange` → `patchChatSettings({ customToolTriggers }, 'Failed to update trigger phrase setting' ×2)`; `ChatTabContent.tsx` wires it. **(i) Help / docs:** `help/custom-tools.md` +30 (NEW section "Words that pull the lever — `triggers`": match rule, who can set one off, what Pascal never listens to, how it rolls, the four-per-message cap, echoes, "It must be switched on"; the limits paragraph; the Custom tools toggle paragraph + the Automation toggle), `help/chat-settings.md` +2, `help/pascals-workbench.md` +1, `help/rng-tool.md` +2; `API.md` (PUT field + the three-entrances paragraph on `?action=run`), `FEATURES.md`, `features/complete/pascal-custom-tools.md` (+item 16, the "Correction (4.11) — three entrances, one run service"), the spec moved to `complete/` with an As-built section. **(j) Tests-only / NO-PORT:** `registry.test.ts` +24 — an acyclic-dependency guard ("`add-custom-tool-triggers-field-v1` once named itself in `dependsOn`" — a dev-time slip, fixed before the commit); the `prettify` label. | **Pascal server family (P4.6ay units 1 / 3 / 4–9 / 12, P4.d10 unit 5, P4.6be unit 1, P4.d19, P4.D30, P4.D169):** `crates/quilltap-core/src/pascal/custom_tool_types.rs` (+ the SPA twin `apps/web/src/app/pascal/custom-tool-types.ts`; `pascal_custom_tool_definition_equivalence`), `pascal/custom_tools.rs` + `roster.rs`, `tools/run_custom.rs` (`pascal_run_custom_handler_equivalence`, `pascal_run_custom_equivalence` — the refactor moves v4's log lines and failure arms), `api/custom_tools.rs` + `quilltap-web/src/custom_tools_routes.rs` (`pascal_custom_tools_route_equivalence`), `pascal/side_effects.rs` / the Pascal + Prospero writers (`pascal_side_effects_equivalence`, `pascal_writers_equivalence`), `pascal/workbench.rs` (`pascal_workbench_equivalence` — `triggerCount` is a NEW wire key), `apps/web/public/schemas/qtap-custom-tool.schema.json` (byte-copy). **The chat spine (Phase 3 / P4.2 M2; `message_finalizer_tier3_equivalence`):** `services/orchestrator.rs`, `services/message_finalizer.rs` (two NEW call sites next to `rng_patterns.rs` and the Carina paths), the Event channel's `pascalResult` event (SSE + the Tauri pump). **Carina (P4.6ay-round Carina units; `carina_parser_equivalence`, SPA fixture `carina-parser.ndjson`):** `carina_parser.rs` gains `strip_carina_query_lines`. **Chat settings (P4.d6 cross-lane finding — the `customTools` precedent, 2026-07-17; P4.6an unit 3; the Settings vertical P4.6e):** `db/chat_settings.rs`, `api/settings.rs` (`bool_field` sibling), `services/provisioning/fresh_schema.json` + `chat_settings_seed.json` (D23), a NEW `db/chat_settings_custom_tool_triggers_repair.rs`-shaped ensure + `restore_vintage_state`, `tools/help.rs` `help_settings` (`help_tools_equivalence`), `almanack/phase3_ledgers.rs` + render (P4.37 unit 12, the `almanack-*` family), `apps/web/src/app/screens/settings/chat/automation-settings.ts`. **`pascalMeta` row parse** (`db/chats_messages*`, the `.qtap` import / export / backup carriers' enum). **Workbench SPA (P4.6bb, P4.D36 unit 4):** `apps/web/src/app/screens/custom-tools/builder-form.ts`, `workbench-library.ts`, a NEW trigger-phrases field. **`help/` 131 → 131 edited (`p4.9i2`)** + `docs/v4/` (API.md, DDL.md, FEATURES, features/complete). CLI Tier R: untouched (stamp only). | **UNPROCESSED — past the `83334b9a5` round's pin** *(2026-10-16: `f0db8cbb8` extends this commit's `run-service.ts` (the wardrobe announcement set) and `4877fc2da` registers the ELEVENTH move's migration right after this one — order the three together; one re-dump carries both moves.)* |
| `f0db8cbb8` | 2026-10-15 | feat(pascal): wardrobe effects — custom tools can dress and undress the rolling character | **PORT-NEW + PORT** (classified 2026-10-16 from the hunks; 44 files, +2,158 / −124; `4.11.0-dev.4`; the 2026-10-15 checkout DIRT, committed — the hunks match what §1 recorded of it). **PORT-NEW:** a custom-tool effect may be `{ wardrobe: { action: 'wear' \| 'take_off' \| 'clear_slot', items?, mode?, slot? } }` instead of `target` + `value` (`custom-tool.types.ts` +233: the union, `isWriteEffect` / `isWardrobeEffect`, `wardrobeGestureProblems` at load); the NEW applier `lib/pascal/wardrobe-effects.ts` (269 lines — item refs are templates rendered with the outcome placeholders, resolved by id then title in the rolling character's wearable pool, applied AFTER the value writes commit, one gesture at a time through `wearItem` / `takeOffWearable` / `applyDisplacement` with source `'tool'`, ONE `notifyWardrobeChanged`; unresolvable / archived-for-wear / slot-mismatched items skipped per item; no rolling character → no change); `pascalMeta.effects` entries targeted `wardrobe.wear` / `wardrobe.take_off` / `wardrobe.clear_slot` with a `WardrobeEffectRecord` `next` (row + event schema COMMENTS only — `target: z.string()` already admits them; NO DDL, NO migration); `run-service.ts` / `run-custom-handler.ts` / `tool-executor.ts` thread the turn's `pendingWardrobeAnnouncements` so a tool's change joins the turn's ONE Aurora announcement; `run-custom-tool.ts` — the preamble's side-effects sentence gains "or what they are wearing" and the roster's `Side effects:` line becomes `describeEffects` (`writes a, b; wears X; takes off Y (slot only); clears hair`) — MODEL-VISIBLE bytes; `tool-draft.ts` +159 (the Workbench draft check), `tool-vocabulary.ts`, `side-effects.ts`; `public/schemas/qtap-custom-tool.schema.json` +102; the SPA's Side effects card Kind select, the Proving Bench "would attempt", the run dialog. **PORT (a fix):** `wardrobe_take_off` on a COMPOSITE removed nothing (worn composites are stored as their leaves) — the NEW `takeOffWearable` (`wear-ops.ts` +73) takes a composite off by its component graph's leaves, a single garment by its slots; `loadCurrentSlots` exported. Help: `custom-tools.md` (+57), `pascals-workbench.md`, `wardrobe.md`; `CUSTOM_TOOL_SPEC.json`, DDL prose, `qtap-export.schema.json` (one description). **Depends on `362426c8f`** (the run service it extends). | **Pascal family:** `crates/quilltap-core/src/pascal/custom_tool_types.rs` + the SPA twin (`pascal_custom_tool_definition_equivalence`), `pascal/side_effects.rs` (`pascal_side_effects_equivalence`), `pascal/workbench.rs` (the draft check), `tools/run_custom.rs` (the roster description + preamble bytes — `pascal_run_custom_equivalence`), `apps/web/public/schemas/qtap-custom-tool.schema.json` (byte copy), the SPA Workbench / Proving Bench / run dialog (P4.6ay, P4.d10, P4.6be, P4.d19, P4.D30, P4.D169). **Wardrobe:** `crates/quilltap-core/src/wear_ops.rs` + `tools/wardrobe_take_off.rs` (P4.D279 keystone / P4.D280 — v5 SHARES the composite bug: it filters the item id out of `item.types`' slots, as v4 did), the Aurora announcement set on the chat spine (P4.D282). Help → `p4.9i2`. | **UNPROCESSED** — past the `83334b9a5` round's pin; order WITH `362426c8f` (it builds on that commit's `run-service.ts`). |
| `6069a7acd` | 2026-10-15 | fix(salon): a one-character chat no longer chains the character to the depth cap (bug 249) | **PORT** (classified 2026-10-16; 9 files, +171 / −7; `4.11.0-dev.5`). ONE production hunk: `shouldChainNext` (`turn-orchestrator.service.ts` +22) returns `{ chain: false, reason: 'user_turn' }` when `selectionReason === 'only_character'`, the picked participant is `turnState.lastSpeakerId`, `!effectiveAllLLM` and `!guards.neverPauseForUser` — with a NEW INFO line `[TurnOrchestrator] Lone character has answered; floor returns to the operator` `{ chatId, chainDepth, participantId }`. The selector is UNCHANGED. Bug 249's provenance: "Original to v4", introduced by `83f4368a8` (2026-05-24); `v5 status: Not assessed` — NOT a port filing, so NOT a convergence. | `crates/quilltap-core/src/services/turn_orchestrator.rs` (`should_chain_next` — the Phase-3 turn chain + the P4.d turn-skipping re-ports), `select_speaker.rs` (`only_character` — unchanged). v5 SHARES the runaway (a persona-less one-on-one chat chains to the depth cap). The turn-orchestrator oracle family and its log-line capture move. | **UNPROCESSED** — past the `83334b9a5` round's pin. |
| `4877fc2da` | 2026-10-15 | feat(salon): slash commands in the composer | **PORT-NEW + PORT WITH THE ELEVENTH D23 MOVE** (classified 2026-10-16; 50 files, +1,974 / −144; `4.11.0-dev.6`). **Schema:** `ChatSettingsSchema.composerSlashCommands: z.boolean().default(true)` between `composerUnicode` and `impersonationVoiceMode`; the migration `add-composer-slash-commands-field-v1` (`addColumnIfMissing('chat_settings', 'composerSlashCommands', 'INTEGER DEFAULT 1')`, table-gated, `dependsOn sqlite-initial-schema-v1`, registered AFTER `add-custom-tool-triggers-field-v1`, one INFO line on add + one ERROR on failure); the repository's default seed gains `composerSlashCommands: true`; `prettify.ts`'s label "Fitting the composer with a speaking-tube to the tool room". **Server:** `PUT /api/v1/settings/chat` gains a boolean arm (`Invalid composerSlashCommands value (must be boolean)`) threaded through `updateChatSettings`. **Client (PORT-NEW, SPA):** `lib/slash-commands/` (pure trigger / ranking / registry — `trigger.ts` 33, `ranking.ts` 76, `registry.ts` 191, `types.ts` 97; under the char-insert ESLint import restriction — a tier-1 candidate), `SlashCommandPlugin` on the shared typeahead shell, `useSlashCommandRunner` (zero-param tools run at once; others open `RunToolModal` / `CustomToolRunDialog` pre-selected via the new `initialTool`), the custom-tool dialog moved into `ChatModals`, `rng-request.ts` shared by `RngDropdown` and the `/d6`-style quick rolls, a `queryKeys.tools.byChat` read of `?chatId=…&includeSchemas=true`, the composer settings card. Help: `chat-settings-composer.md` (+48), `custom-tools.md`, `rng-tool.md`, `run-tool.md`; DDL / API prose. | **D23:** `fresh_schema.json` + `chat_settings_seed.json` + a NEW unstamped table-gated boot ensure in the `chat_settings_composer_repair.rs` shape (which already carries `add-composer-unicode-field-v1` and its prettify label — the closest twin) + `restore_vintage_state`'s boot-align list + the committed `*-main.db` pairs widened. **Server:** `crates/quilltap-core/src/db/chat_settings.rs`, `api/settings.rs` (the chat-settings PUT arms — P4.6e Settings vertical / the composer-toggle orders), backup / restore / `.qtap` chat-settings carriers. **SPA:** the composer's typeaheads (`apps/web/src/app/editor/char-insert/`, the composer typeahead orders), `chat/chat-composer.ts`, the Run Tool modal + Pascal run dialog, the RNG dropdown, `screens/settings/chat/` (the `composer-unicode-settings.ts` twin). Help → `p4.9i2`. | **UNPROCESSED** — past the `83334b9a5` round's pin; its re-dump rides the SAME `fresh_schema.json` move as `362426c8f`'s TENTH (one pin at or past `4877fc2da`). |
| `b93c2efda` | 2026-10-15 | docs: specs for the dependency safe batch and the Lexical 0.52 upgrade | **NO-PORT?** (classified 2026-10-16; 3 files, +319, docs only): two NEW specs under `docs/developer/features/` (`dependency-upgrade-safe-batch.md`, `lexical-0.52-upgrade.md`) + a CHANGELOG line. No code. Both specs are moved to `features/complete/` by `eb5d42338` / `b2793400f`. | None (v5 vendors only the `docs/v4/` paths a round needs; re-vendor at the next catch-up if the mirror tracks `features/complete/`). | **UNPROCESSED** — past the `83334b9a5` round's pin; NO-PORT ratification at the next unification. |
| `eb5d42338` | 2026-10-15 | chore(deps): dependency safe batch | **PORT** (a dependency move on the provider recorders; classified 2026-10-16; 32 files, +6,233 / −2,151; `4.11.0-dev.8`). Measured from the lockfile, not the message: **`openai` 7.30.0 → 7.34.0** (root), RE-BUNDLED into all ten `plugins/dist/*/index.js` (each `VERSION2 = "7.34.0"`, manifests / package.json patch-bumped — e.g. anthropic 1.0.61 → 1.0.62); the bundle diff carries the SDK's own changes — a URL-filename decode, the streaming accumulator now SUMMING `completion_tokens_details` / `prompt_tokens_details` across chunks, redacted query keys (`sig`, `signature`, `xgoogsignature`, `xgoogcredential`). `zod` 4.6.5 → 4.6.7, `katex` ^0.19.0 (rendering still through rehype-katex's nested 0.16 per the spec), test / type deps, `@vitest/mocker` removed (+ its `knip.json` ignore). `@quilltap/plugin-utils` stays 2.7.1, `@openrouter/sdk` 1.4.25. No `lib/` / `app/` hunk. | `crates/quilltap-harness/tests/provider_sdk_version_guard.rs` (`INSTALLED_ROOT_OPENAI_SDK = "7.30.0"` + the recorded-corpus stamps — the TRIPWIRE: at a pin past this commit the recorders must replay 7.34.0, and the corpora re-record with the new stamp); the OpenAI-flavor stream decoder + usage read (P4.13 / P4.14 `StreamMessage`, P4.141) — MEASURE whether v4 reads the SDK's accumulated `*_tokens_details` on any path v5 decodes itself (a usage-count convergence candidate). zod 4.6.7 → the Zod-message families (re-run, expect no move). | **UNPROCESSED** — past the `83334b9a5` round's pin; ride the next round's pin `npm ci --offline` (the pin's plugin dirs must resolve 7.34.0 — §5.1). |
| `b2793400f` | 2026-10-16 | chore(deps): Lexical 0.52; fix code-block Enter sending the message (bug 251) | **PORT** (small, SPA; classified 2026-10-16; 19 files, +981 / −293; `4.11.0-dev.9`). Lexical 0.43 → 0.52 in lockstep (nine packages), `jest.config.ts`'s ESM exemption, three typeahead / paste type fixes, `TABLE_TRANSFORMER`'s bare `regExpEnd` (tables built on import only) — all Lexical-internal, **NO-PORT** for v5 (ProseMirror, D17). **Bug 251** (`KeyboardPlugin.tsx`): `$codeBlockToEscape` accepts the element-point caret Lexical leaves after Shift+Enter (Lexical-only — ProseMirror's caret is always a text position), **AND the exit's trim NARROWS**: the old `text.replace(/\n\s*$/, '')` stripped EVERY trailing whitespace-only line (the leftmost `\n` whose remainder is all whitespace), the new code removes trailing whitespace-only text nodes then ONE line break — `"a\n  \n"` was `"a"`, is now `"a\n  "`. Bug 250 FILED (open; "v5 status: Not applicable"); bug 251 "Original to v4", "v5 status: Not applicable" — v4's own reading; the trim is applicable. `help/chats.md` gains a code-block paragraph. | `apps/web/src/app/editor/editing-commands.ts` `exitCodeBlockOnBlankLine` (v4's old `/\n\s*$/`, ported one for one — and its docstring's claim that the regex strips "one newline plus any trailing whitespace, not every trailing blank line" is FALSE for the old regex; the new v4 behaviour is what that sentence describes, so v5 converges by re-porting the trim and the claim becomes true); `rich-editor.ts`'s composer binding unchanged. Help → `p4.9i2`. | **UNPROCESSED** — past the `83334b9a5` round's pin. |
| `25b8fab6a` | 2026-10-16 | fix: month-word prototype keys (bug 247); orphaned typeahead menu (bug 250) | **CONVERGENCE** (bug 247) **+ NO-PORT?** (bug 250) (classified 2026-10-16 from the hunks; 17 files, +207 / −31; `4.11.0-dev.10`). **Bug 247 — this port's filing** (the harness's `PROTOTYPE_KEY_MONTH_THROWS`, filed `052659624` / `809e5104d`): `lib/memory/time/clock.ts` reads `MONTH_NAMES` / `WORD_NUMBERS` / `UNIT_DAYS` through a NEW `lookupWord` (`Object.hasOwn` — own keys only), so `constructor 5` / `5 constructor` (and `on constructor 5`, `constructor 5th`, `constructor 5, 2026`, `a constructor ago`, `two constructor ago` — v4's new test table) resolve to `{ occurredAt: null, window: null }` instead of throwing; `lib/memory/time/anchors.ts` `resolveEpisodicAnchors` wraps the phrase parse in `try` / `catch` → `fallbackIso`, so a parser throw never fails an extraction job (no log line on the catch). That is v5's answer already (its closed month table never reached the throw). **Bug 250** — a NEW `GuardedTypeaheadMenuPlugin` wrapping `LexicalTypeaheadMenuPlugin` (re-runs the consumer's `triggerFn` after each open, remounts when the trigger is gone); Mention / Char / Slash typeaheads render through it — Lexical's deferred React transition, v4's own reading "v5 status: Not applicable". `bugs.md`: 247 and 250 moved to `fixed/`. | `crates/quilltap-harness/tests/memory_time_equivalence.rs` — `PROTOTYPE_KEY_MONTH_THROWS` (pinned BOTH ways: v4 must still throw on every zone / anchor; v5 answers nothing) **trips by design** at a pin past this commit (`throws_measured` empties); retire the table and its doc comment by MEASUREMENT (§5.4), and grow the corpus with v4's five new phrases (the `ago` arm through `UNIT_DAYS` / `WORD_NUMBERS` included — confirm v5's tables answer nothing for each). v5's memory-time module (the P4.D289 calendar / scoring port) needs no code change unless the grown corpus says otherwise; the anchors catch has no v5 analogue (no throw path). Bug 250: v5's SPA typeaheads (`apps/web/src/app/editor/char-insert/`, ProseMirror) — none. | **UNPROCESSED** — past the `83334b9a5` round's pin. |

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

- **The `83334b9a5` memory-refactor + scenarios drift catch-up round
  (2026-10-15, baseline `4a38222e6` → `83334b9a5`; {P4.D289 → P4.D288} →
  {P4.D290 → {P4.D296 ∥ P4.D297 ∥ P4.D298}} ∥ P4.D291 ∥ P4.D292 ∥ P4.D293 ∥
  P4.D294 ∥ P4.D295, with P4.D299 ∥ P4.D300 ∥ P4.D301 ∥ P4.D302 ∥ P4.D303 —
  sixteen lanes):** the twenty rows retired. **NO-PORT-RATIFIED(P4.D303):**
  `4ce64fa1e` (the merge renumbering the picker bug to 229 — v5 already 229),
  `9ec4fa2e9` (the V4test dogfood — docs + one `memories status` line with
  no v5 home), `83334b9a5` (identical-output `successResponse` swaps).
  `97f0d8cb8` (bug 229, the folder picker) ABSORBED(P4.D285) — ported ahead
  of the baseline in the `4a38222e6` round (R-P). **ABSORBED:** `545e299bb`
  (scenarios from anywhere — the EIGHTH D23 move, `chats.scenarioSource` /
  `scenarioNotes`, the location + browser + source tracking) ABSORBED(P4.D288,
  P4.D293, P4.D294, P4.D295, P4.D302, P4.D303); `124c8aca3` (memory Phase 0,
  bugs 199–201 — the trailing-only recall, the gate's orphan widen, the chat
  wipe sparing manual / cold / digest rows) ABSORBED(P4.D288, P4.D291,
  P4.D298); `69d38dd60` (bug 227, the deferred mail flip + one trailing-
  section list) ABSORBED(P4.D291); `8050958be` (bug 228, reminted message
  ids on a fresh-id `.qtap` chat import) ABSORBED(P4.D294); `c2795733f` (bugs
  203–226, five migrations) ABSORBED(P4.D288, P4.D289, P4.D290, P4.D291,
  P4.D292, P4.D296, P4.D297, P4.D298); `e0ac7493a` (Phase A — scoring, the
  memory calendar, the settings move with its three DROP COLUMNs, the
  candidate, bugs 230–234) ABSORBED(P4.D288, P4.D289, P4.D293, P4.D298,
  P4.D300); `af48f6cc2` (Phase B — the index owns the vectors, ONE deletion
  path, the vector tables DROPPED after verify-and-copy — the human's MIRROR
  ruling) ABSORBED(P4.D288, P4.D292, P4.D293, P4.D298); `8af764514` (Phase
  C — admission in the parent, one combine, the lowercase `action` wire)
  ABSORBED(P4.D289, P4.D290, P4.D298); `ac6348c5f` (Phase D — extraction as
  one context and four passes, the cursor table, BOTH chat watermark columns
  DROPPED — P4.D265's chat-column ensure RETIRED by the human's ruling)
  ABSORBED(P4.D288, P4.D293, P4.D294, P4.D296); `93c5af6ad` (bugs 236–241:
  the manual create, admission's mixed-dimension neighbours, the fold OTHER
  date, the lock `port`, the chats-only import's memories, the submission
  ledger) ABSORBED(P4.D290, P4.D294, P4.D296, P4.D298, P4.D299, P4.D301) with
  bug 235 (`processSingleton`) NO-PORT by name (P4.D303) and bug 241's
  transport halves NO-PORT (R-AC); `584d28164` (Phase E — recall as a
  pipeline, delivery as the one access writer, the debug labels gone from the
  model-visible text, `debugMemoryLogs`, the replay's trace) ABSORBED(P4.D291,
  P4.D298); `4ae0c4ca6` (the TF-IDF tuning — the NINTH D23 move
  `tfidf_vocabularies.textShapeVersion`, the refit policy, per-provider
  admission thresholds, the lexical embedding text) ABSORBED(P4.D288,
  P4.D289, P4.D290, P4.D292, P4.D293) with bug 243 (the forked child's
  plugin race) NO-PORT by name (P4.D303); `104e986c7` (Phase F — ONE
  `MEMORY_MAINTENANCE` job, the housekeeping / consolidation jobs and the
  `memory-dedup` tool DELETED, the job-row migration) ABSORBED(P4.D288,
  P4.D297, P4.D300); `b045dd986` (Phase G — the Maintenance card + dialog +
  CLI, `?action=repair`, bug 242's `inChain`, bug 244's `patchSchemaOf` —
  v5's ruled `PARTIAL_DEFAULTS_CLOBBER` CONVERGED to `PATCH_PRESERVES_STORED`)
  ABSORBED(P4.D297, P4.D298, P4.D300, P4.D301); `48790fb73` (the refactor's
  close — three labels, the Friday backfill record) ABSORBED(P4.D300,
  P4.D303); `64743ee13` (bugs 245 / 246 — the `turn` cursor pass, the turn
  transcript, the bundles' `turnExtractionWatermarkMessageId`, the
  consolidation budget floor) ABSORBED(P4.D294, P4.D296, P4.D297). Deferred
  by name and carried in the orders' headers: R-W (extraction's Tier-3 loud
  refusals), the CLI `memories` family (R-O, OUT), P4.D291's Tier 2 #24
  recall-probe fixture, P4.D298's `create_project_explicit_wins`, the
  repository-fallback `?` reads the review listed for the follow-ups round.
  The SEVENTEEN commits past the pin (`a42c282b8` … `362426c8f`, incl. the
  `release: 4.10.0` squash and the TENTH D23 move) and the two the orders'
  commit recorded (`7417e9a1b`, `01f508d4e`) stay in §3; every regen, the
  whole sweep and every live-checkout guard ran from the pin
  `/tmp/qt-v4-pin-unify-83334b9a5` with its own `npm ci --offline` tree. The
  LIVE-DATA HAZARDS block RETIRED by measurement (§1). Round record:
  `status-log.md` → "The `83334b9a5` memory-refactor + scenarios drift
  catch-up round — UNIFICATION record".

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
