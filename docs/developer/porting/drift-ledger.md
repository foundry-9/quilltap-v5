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

- **Oracle baseline: `322a5488e`**: "Fix: custom-tool LLM consult can narrate
  a verdict the table already reached (bug 257)" (v4 main, 2026-10-20 16:50
  −0500, `package.json` `4.11.0-dev.53`). It was adopted when the
  `322a5488e` Inventory + Log + interrupts + custom-tool-grammar + bug-255
  drift catch-up round was unified (2026-10-27). All seventeen lanes landed:
  P4.D324 → {P4.D325 ∥ P4.D326 ∥ P4.D327 ∥ P4.D328 ∥ P4.D329 ∥ P4.D331 ∥
  P4.D337 ∥ P4.D340} ∥ P4.D330 ∥ P4.D332 ∥ P4.D333 ∥ P4.D334 ∥ P4.D335 ∥
  P4.D336 ∥ P4.D338 ∥ P4.D339. CLAUDE.md's Status bullet for the round and
  its "Oracle baseline" bullet both name `322a5488e`.
- **Checked:** 2026-10-27 (afternoon), by `/driftcheck` run on main right
  after the `322a5488e` round's unification (`a87b3b4b1`) — the §2 probe
  FAILED on 5 new `main` commits past the recorded `85c002822`. Fetched
  first; local equals remote on all three branches (`main` = `86cc25482`,
  `bugfix` = `489b228b6`, `release` = `2432b9057`). The 5 were classified
  directly from their hunks.
  - **Before that:** 2026-10-27 (morning), by `/driftcheck`
    (`15277d2ee`, run as the
    round's `/unify` started — the §2 probe had FAILED on 14 new `main`
    commits past the then-recorded `a0905454b`). Both branches re-probed
    locally AND against the remote after a fetch (`main` = `85c002822`,
    `bugfix` = `489b228b6`, `release` = `2432b9057` — local equals remote).
    That check classified the 14 directly; the 32 of 2026-10-26 were
    classified by five parallel readers. The unify itself regenerated every
    oracle from the detached pin `/tmp/qt-v4-pin-unify-322a5488e` (its own
    `npm ci --offline` tree: `openai` 7.34.0, `zod` 4.6.7, `@xterm/headless`
    6.0.0, `plugin-types` 2.8.0; the plugin `node_modules` linked from the live
    checkout, `plugin-types` 2.9.0 there — inert at the pin, type-only
    imports).
  - **Earlier:** 2026-10-26 (`a0905454b`, 32 rows); 2026-10-20 evening
    (`322a5488e`, seven rows); 2026-10-19 late evening (`e27fb6b34`) and
    morning (`a6292f169`); 2026-10-18 evening and morning; and the earlier
    checks the §6 bullets record.
- **v4 `main` HEAD at check: `86cc25482`** ("Fix: legible dialogue in user
  messages; Rains user text; Great Estate sidebar", 2026-10-27 10:16,
  `4.11.0-dev.97`) — **FIFTY-ONE commits past the baseline**, ALL
  UNPROCESSED in §3, two merges among them (`5e18ede6a`, `744596158`):
  - **The 32 of 2026-10-26** (`6fd2d5ffb` … `a0905454b`):
    - *Memory:* `6fd2d5ffb` the importance rubric (PORT, prompt text);
      `a5efdbb97` the inner circle in the frozen archive (PORT-NEW);
      **`8cb584eff` keystone memories WITH THE SEVENTEENTH D23 MOVE**.
    - *Storyteller + Pascal:* **`b15fe9404` Storyteller mode + declared tool
      effects WITH THE EIGHTEENTH + NINETEENTH MOVES** (a NEW table
      `storyteller_rounds`; `llm_logs.storytellerRoundId`); `332c650b9` a
      spec (NO-PORT?); `eb2d87788` every LLM call logs (PORT); **`839575d89`
      operator intervention WITH THE TWENTIETH MOVE**; `946a50a04`,
      `8ae8a127c`, `b02d902f3` riders on those two.
    - *Salon:* `036b32a78` bug 258; **`778b97b2b` the Novel template WITH THE
      TWENTY-FIRST MOVE + a boot-seed rewrite of `narrationDelimiters`**;
      `fa126bde8` bug 259; `40fab1c54` bugs 260 / 261; `f7a1789df` →
      `d68c1af88` → `9a88f862f` the referent ledger, closing echo and
      absent-character name chips (ONE lane); `1dd0a0c01` composer
      punctuation warnings; `cb94e6e99` the dead `debugLLMRequest` frame
      (NO-PORT?).
    - *Wardrobe / characters / Inventory / SPA:* `eb90d9ed8` wardrobe pronoun
      guidance; `99463fc56` the floating Brahma Console; `265b711dd`
      picture-cue guidance; `074e1e01a` `self_inventory`'s checklist +
      holdings; `b27e5089a` the `humanoid` flag; **`0276d10d9` "Refactor:
      Inventory chokepoints" — NOT neutral; it changes what the round just
      landed** (duplicate's 404 → a Zod 400, whole-handler error mapping,
      the cast's `type` filter dropped, publishes moved into the locations,
      the pick-up no-vault text); `4a25f7eab` / `505a524fa` release notes.
    - *Refactors + diagrams:* **`a8214b253` the Salon turn pipeline (bug 262;
      `done` frame key order)**; **`bdd09b75b` the context manager (bugs 263
      / 264 / 265 — prompt bytes move)**; `6588972d5` diagrams (two vault
      keys, an opt-in prompt note, FOUR client deps); `5e18ede6a` the merge +
      `a0905454b` (restored the deps) — both NO-PORT?.
  - **The 14 of 2026-10-27** (`9faed0c7e` … `85c002822`):
    - ⚠ **On surfaces this round just LANDED:** **`cc5f2347c` backups now
      carry the Log** (REVERSES the pin's "not carried" rule P4.D327 pins and
      P4.D331 documents — `data/log-entries.json`, restore `22n-quater`, the
      new-account remap, `deleteAll()` on replace / delete-all); **`85c002822`
      bugs 267 / 268** (over P4.D337's `pty-manager` port —
      `kickAllForChat` → `disposeChatSessions`, SIGHUP → SIGKILL, the
      orphan-transcript sweep; `help/ariel.md`, vendored by P4.D339);
      `c50cdddc6` (help only — `ariel.md` + `system-backup-restore.md`);
      `7d7e38d2f` (tests only — a toast-stacking pin on P4.D338's z-index).
    - *Server:* `026222176` bug 266 + pdfjs-dist 6.4 + `pdf-parse` REMOVED
      (a WARN line's text moves; ⚠ ONE IMPORT-DEAD oracle; `openai` 7.34.0 →
      7.45.0 in the same commit).
    - *SPA / client:* `9faed0c7e` the diagram lightbox; `2041bb875` the
      Automation checkbox class; `8bf856050` diagram trace lines pruned.
    - *CLI:* `40cc6ffda` `db --count` (verify-only).
    - *NO-PORT? candidates:* `3c9ae1fc3`, `8becfc6ce`, `66f8c4d41`,
      `539bb4012`, `744596158`.
  - **The 5 of 2026-10-27 afternoon** (`4b0d1cb04` … `86cc25482`; NO
    migration, NO D23 move, NO vault key, NO dependency move — the lockfile
    hunks are version stamps only):
    - *Server:* `4b0d1cb04` **bug 269** (v4-ORIGINATED, found live on
      Friday) — the NanoGPT image provider fits `size` / a NEW flat
      `aspect_ratio` body key to the model's detailed-catalog entry, fetches
      a cold catalog, and retries a shape-refused non-square once as a
      square (PORT on P4.D101's NanoGPT image dialect; v5 HAS the defect).
    - *SPA / client:* `2b2b190b6` composer prose mode (PORT-NEW — rides the
      UNPORTED punctuation lint `1dd0a0c01` + Novel template `778b97b2b`;
      on the Salon composer the round's interrupt pair just touched);
      `cce44e098` the legibility pass and `86cc25482` legible user dialogue
      (PORT — `qt-components` CSS + five vendored theme packs; the
      intervention / Storyteller halves ride their UNPORTED features).
    - *Process:* `dd200c695` (NO-PORT? code-wise) — **v4's bug catalogue is
      FROZEN at Bug 269; new v4 bugs are GitHub issues `#N`**. A convergence
      check now reads BOTH `docs/developer/bugs.md` (bugs ≤ 269) AND the
      issues (`gh issue list -R foundry-9/quilltap-server --label bug
      --state all`); a v4 bug this port files from now on is a GitHub issue
      via the Bug form — outward-facing, confirm with the human first (§4
      step 5).
    - **None lands on a surface the `322a5488e` round just ported**
      (Inventory, the Log, interrupts, the custom-tool grammar, the terminal,
      scenarios, `/photos`): the nearest are `2b2b190b6` (the composer — a
      new prop beside, not on, the round's raise-hand / stop) and
      `cce44e098`'s one `help/pascals-workbench.md` sentence (the
      hidden-field hand — rides UNPORTED intervention `839575d89`).
  - **No CONVERGENCE** in the 51: bugs 258–269 were all found by v4 itself;
    v5 status `Owed` (269: `Not assessed`) in every row.
  - **Three "Refactor" / "Merge" subjects are NOT neutral** (§5.3):
    `0276d10d9`, `a8214b253`, `bdd09b75b`; and `6588972d5`'s "nothing new is
    stored" is FALSE.
- **v4 `bugfix` tip at check:** `489b228b6` ("bugfix: started 4.10.0 bug
  branch", 2026-10-14) — `489b228b6..bugfix` EMPTY (local and `origin`).
  **`release` tip:** `2432b9057` (`release: 4.10.0`), unchanged. Re-probe
  BOTH branches every time.
- **Checkout at check:** branch **`main`**, tree **CLEAN** at `86cc25482`.
  HEAD is fifty-one commits past the baseline, so it poisons a regen by
  DRIFT.
- **Verdict: DRIFT PENDING — 51 commits (§3), all UNPROCESSED.**
- **Regen rule: PIN REQUIRED at `322a5488e`** (§5.1). The live root
  `node_modules` is at HEAD's tree (`plugin-types` 2.9.0 + the four diagram
  deps; `openai` 7.45.0 once the human installs past `026222176`), so the
  pin's own `npm ci --offline` rebuild is REQUIRED, not merely safe
  (measured 2026-10-26 and again at this unify: ~6 s offline).

### ⚠ LIVE-DATA — what stands at this baseline

- **A v5 at `322a5488e` reads and writes `chats.equippedInventory`,
  `characters.canManageInventory`, `terminal_sessions.cols` / `rows` and
  `log_entries`.** The first v5 boot of a pre-`4fbdad186` Friday copy ADDS the
  three columns (two Inventory column ensures + the geometry ensure — all
  UNSTAMPED, column-gated, v4's shapes) and CREATES `log_entries` through the
  structural pass (R-B); a copy v4 migrated sees NO-OP ensures. A custom tool
  with an Inventory / Log gate or effect, an any-of array or a `brief` LOADS;
  `edit_custom_tool` exists; the consult runs after the table (bug 257
  fixed); the Salon has raise-hand and stop; `qtap://self` is gone from every
  published address (bug 256); `/photos` excludes vaults; scenarios list by
  title; `terminal_read` renders the screen (bug 255 — screen sizes clamped
  at 500 × 200, a ruled divergence: vt100 preallocates every cell).
  `equippedInventory` and `canManageInventory` survive a backup / restore /
  `.qtap` round trip; the Log is NOT carried by a logical backup (the pin's
  rule — see `cc5f2347c` below).
- **The general rule BITES now:** a v5 build from BEFORE this round must not
  open an instance this round's ensures have altered — the owed `e8e17b2d9`
  dogfood pass runs on a copy with a PRE-round build, or with the union
  build.
- **Past `322a5488e` — THREE vault-key ERASURE hazards (MEASURED in v5's
  writers, 2026-10-26)** for a v5 sharing an instance with v4 at HEAD:
  - ⚠ **`humanoid` (`b27e5089a`):** v5's `render_physical_prompts_json`
    (`db/vault_character_write.rs`) serializes a FIXED five-key
    `PhysicalPromptsJson` — every v5 physical write ERASES a v4
    `"humanoid": false`; the P4.82 backfill would also write it a
    head-and-shoulders prompt v4 skips.
  - ⚠ **`canDrawDiagrams` / `diagramStyle` (`6588972d5`):** v5's
    `PropertiesJson` writes six keys, so any v5 properties write drops both.
  - ⚠ **`pronounGuidance` (`eb90d9ed8`):** v5's `build_wardrobe_item_file`
    rebuilds a wardrobe item from its struct, dropping the key.
  - v5's readers ignore unknown keys, so READS are safe; only writes erase.
  - **Probable, measure at port time:** `778b97b2b`'s every-boot seed
    re-encodes built-in templates' `narrationDelimiters` as JSON and inserts
    a Novel template with it NULL (v5's raw read may see `"*"`, may drop the
    NULL row); `8cb584eff`'s keystones are invisible to v5's maintenance (a
    v5 consolidation / demotion / fold / chat-wipe can touch a memory v4
    protects; a v5 backup omits the column).
  - **Non-corrupting:** the five newer migrations are additive with
    DEFAULTs (`memories.keystone` `NOT NULL DEFAULT 0`, so a v5 INSERT that
    does not name it is legal); `storyteller_rounds` cascades from `chats`;
    v5 never writes `turnMode`; `40fab1c54`'s backfill fills blob text v5
    ignores; `punctuationWarnings` reads back `true` by v4's default.
- **Past `a0905454b` — NO boot / schema hazard** (no migration, no column,
  no vault key). Non-corrupting: a v4-HEAD backup carries
  `data/log-entries.json`, which a v5 restore ignores (every Log line is lost
  on a v5 restore); a v5 `replace` restore / delete-all leaves `log_entries`
  untouched where v4 HEAD clears it; v4 HEAD's daily maintenance unlinks
  `logs/terminals/*.log` no `terminal_sessions` row names (v5's orphans too —
  intended; a live v5 PTY whose row exists is spared by name); a v4-HEAD
  terminal kill / chat delete ends shells v5 would leave running (bugs 267 /
  268 live in v5).
- **Past `85c002822` — NO boot / schema / vault hazard** (the 5 are a
  plugin wire fix, client CSS / themes and docs). Non-corrupting: a v5
  NanoGPT image job against a Qwen Image 3 profile still 400s (bug 269
  lives in v5).
- **The standing rule holds: no v5 build against live Friday.** Dogfood a
  COPY, and do not round-trip v4-written characters through v5 writes while
  comparing with v4.
- **Standing:** the extraction rate ledger is per-process; the `52d6e7ecd` /
  `07b8f0209` notes and the P4.D274 store-name plan probe on a copy stand.

### Schema / D23

- **LANDED this round: the THIRTEENTH to SIXTEENTH moves in ONE
  `fresh_schema.json` re-dump (R-A, P4.D324)** — `chats."equippedInventory"
  TEXT` (cid 66, between `equippedOutfit` and `characterAvatars`),
  `characters."canManageInventory" INTEGER` (after `canCreateOutfits`),
  `terminal_sessions."cols"` / `"rows"` `REAL` (the fresh shape is `REAL`,
  not the migration's `INTEGER`), and the seven-statement `llmLogs` block
  with `log_entries` + its three indexes. The re-dump was re-run from the pin
  at unification and is **byte-equal** to P4.D324's committed file (83 main
  / 31 mount-index / 7 llm-logs statements). The two Inventory columns and
  the geometry columns arrive on an existing instance through three
  UNSTAMPED, column-gated boot ensures carrying both v4 shapes, wired in
  `host.rs` and `restore_vintage_state`'s mirror-boot; `log_entries` through
  the structural pass (one `STRUCTURAL_TABLES` row). `migration_indexes.json`
  UNMOVED. `provisioning_equivalence` (1d) green on the union.
- **PENDING — the SEVENTEENTH to TWENTY-FIRST moves, all additive, all with
  v4 migrations** (each a boot ensure + the `restore_vintage_state`
  boot-align entry, by the 2026-10-19 ruling):
  - **17th (`8cb584eff`):** `memories."keystone"` — migration `INTEGER NOT
    NULL DEFAULT 0` APPENDED; `MemorySchema` places it between
    `consolidatedAt` and `createdAt` (TWO shapes likely) + the
    migration-only partial index `idx_memories_keystone`.
  - **18th (`b15fe9404`, `add-storyteller-mode-v1`):** `chats."turnMode"`,
    `chats."storytellerInviteFilter"`, `chat_messages."storytellerBid"`, and
    a **NEW TABLE `storyteller_rounds`** that generateDDL renders WITHOUT the
    migration's FK and index — a provisioning ruling is owed at planning.
  - **19th (`b15fe9404`):** `llm_logs."storytellerRoundId"` + its index —
    the second llm-logs-partition move (after `log_entries`).
  - **20th (`839575d89`):** `chat_settings."customToolIntervention" INTEGER
    DEFAULT 1`; the chat-settings seed artifact moves with it.
  - **21st (`778b97b2b`):** `roleplay_templates."unmarkedStyle"` +
    `"markdownAsterisks"` (two shapes) PLUS the boot-seed DATA rewrite of
    `narrationDelimiters` and a third built-in row.
  - **Not D23 (vault / JSON keys):** `physical-prompts.json` `humanoid`,
    `properties.json` `canDrawDiagrams` / `diagramStyle`, wardrobe
    `pronounGuidance`, `smartTypographySettings.punctuationWarnings`, the
    `systemKind` value `off-scene-left-behind`.
  - A further `fresh_schema.json` re-dump is owed at the next pin.

### Regen, dependencies, help

- **Dependencies at the baseline:** `openai` 7.34.0 (root + the ten
  `plugins/dist` bundles), `zod` 4.6.7, plugin-utils 2.7.1,
  `@xterm/headless` 6.0.0 (v5's twin is the `vt100` 0.18.1 crate, R-C);
  v5's `regress` crate backs the llm-logs `regexp()` scalar (R-I).
  `provider_sdk_version_guard` and `zod_version_guard` pin the JS side.
- **Past the baseline:** `6588972d5` adds FOUR client-only runtime deps
  (`mermaid`, `@hpcc-js/wasm-graphviz`, `dompurify`, `svg2roughjs` — a
  dependency ruling before diagrams are ported); `@quilltap/plugin-types`
  2.8.0 → 2.9.0 (`b15fe9404`); `026222176` REMOVES `pdf-parse`, moves
  `pdfjs-dist` 5.7.284 → 6.4.299 and **`openai` 7.34.0 → 7.45.0**
  (`provider_sdk_version_guard` reddens at any pin past it — measure the
  openai-compatible wire before re-pinning); `zod` UNCHANGED. ⚠
  **IMPORT-DEAD:** `harness/oracle/cases/file-content-extractor.test.ts:149`
  `require.resolve('pdf-parse')` dies at any pin ≥ `026222176` — a stage-0
  re-aim at the next `/setupphase`.
- **Help / docs:** `help/` is **135 files**, md5-identical to the pin
  (P4.D339); `docs/v4/` **502** with ZERO deferrals (the two Inventory specs
  landed at their `complete/` paths). Past the baseline: NEW
  `storyteller-mode.md`, `diagrams.md`; ~20 pages edited (`ariel.md` ×2,
  `system-backup-restore.md` ×2, `the-log.md`, `custom-tools.md` ×4,
  `memory-recall-relevance.md` ×3, `wardrobe.md`, `inventory.md`, …);
  `docs/releases/4.11.0.md`, `bugs.md` + `bugs/fixed/bug-266/267/268-*.md`.
  The afternoon's 5 add `help/image-generation-profiles.md` (+1 paragraph,
  bug 269), `help/chat-settings-composer.md` + `help/roleplay-templates.md`
  (prose mode), `help/pascals-workbench.md` (one sentence),
  `bugs/fixed/bug-269-*.md`, and `bugs.md`'s FREEZE (later v4 bugs live in
  GitHub issues, not `docs/` — nothing to vendor for them).
- **Themes / CSS:** v5 vendors `apps/web/public/themes/*` (theme versions
  equal to the pin's) and `apps/web/src/styles/qt-components/*.css`.
  `cce44e098` + `86cc25482` move both (art-deco, earl-grey, great-estate,
  madmans-box, old-school, rains); v5 has no `theme-storybook` (its mirror
  hunks are NO-PORT).

### Rounds, guards, state

- **The `322a5488e` round — UNIFIED 2026-10-27** (the §6 bullet; the round
  record in `status-log.md`; `phase-4.md`'s UNIFIED block). The twenty-two
  rows retired to §6.
- **The `e8e17b2d9` round — UNIFIED 2026-10-19** (the §6 bullet).
- **Next:**
  - The owed dogfood pass — BOTH queues (the `e8e17b2d9` round's and this
    round's §S.8), on a Friday copy with the UNION build.
  - The follow-ups smalls round (the ready rider branch
    `claude/import-chat-record-columns-f49fb0` first; this round's OPEN
    review items; the R-4 Brahma-stop RULING ITEM, ruled by the human
    in-round; the carried lists —
    `phase-4.md`'s NEXT).
  - **The 51-commit catch-up** (`/driftcheck` first), at a later pin
    (the afternoon's 5 add: bug 269 on the NanoGPT image dialect, prose
    mode WITH the punctuation-lint lane, the CSS / theme legibility pair as
    one SPA small):
    **first, by the 2026-10-26 ruling,** `0276d10d9`'s Inventory riders and
    the three vault-key writers (`humanoid`, diagrams, `pronounGuidance` —
    the LIVE-DATA hazards); then `cc5f2347c` (the Log in backups — flips
    P4.D327's "no `log_entries` file" pins and the restore census by design;
    v4 drift, not a v5 bug) and `85c002822` (bugs 267 / 268 over P4.D337's
    port); then the natural lanes: the D23 keystone (17th–21st moves in ONE
    re-dump; the `storyteller_rounds` ruling first); tool effects +
    Storyteller; Pascal intervention (+ its riders); keystones + the inner
    circle + the rubric; the referent-ledger trio; bugs 262–265 on the spine
    / budget; bugs 258–261 and 266; the Novel template; SPA smalls (Brahma,
    punctuation lint, `2041bb875`, diagrams — the dependency ruling first);
    help / docs re-vendor. The IMPORT-DEAD `pdf-parse` oracle needs its
    stage-0 re-aim.

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
| `6fd2d5ffb` | 2026-10-21 | Fix: memory importance scale no longer rates work commitments with family facts | **PORT — the importance rubric recalibrated in all four memory-writing prompts; NO D23 move, NO route; model-visible text only** (classified 2026-10-26 from the hunks; 13 files +836 / −23 incl. a 144-line test and the 560-line NEW spec `docs/developer/features/inner-circle-recall.md`; prod 5 files +103 / −18; `4.11.0-dev.54`; parent `322a5488e`). `lib/memory/extraction/prompts/shared.ts` NEW `WORK_IMPORTANCE_CEILING = 0.65` (rendered `toFixed(2)`) + `IMPORTANCE_WORK_CEILING_NOTE` ("Work, project, and tooling agreements — building, filing, fixing, or ruling on something — never score above 0.65, …") + `IMPORTANCE_FACT_NOT_TELLING_NOTE` ("Score the fact, not the telling. …"), both appended after every ladder (ceiling note, blank line, fact note; in SELF / OTHER the ceiling note follows `< 0.20  Do not extract.` with NO blank line). SELF / OTHER: the 0.90 anchor REPLACED by a 0.95 life/bond anchor + a 0.85 revelation / personal-commitment anchor; the 0.65 / 0.60 anchors gain the build/file/fix/rule clause; NEW 0.45 work-decision anchors; examples re-scored (SELF summarizer 0.85 → 0.65; OTHER cache-layout 0.85 → 0.55, rename-deferral 0.65 → 0.45) and a NEW 0.95 pregnancy example each. Episode (`episode.ts`): the `importance` field text becomes the 0.95 / 0.85 / 0.5 / 0.45 ladder + both notes. Consolidation (`cheap-llm-tasks/consolidation-tasks.ts` `CONSOLIDATION_SYSTEM_PROMPT`): the IMPORTANCE paragraph rewritten on the same ladder + both notes. Existing rows are NOT rescored. `help/memory-recall-relevance.md` +16. **v5 intersection:** `memory_extraction/prompts/prompt_text.rs` (SELF / OTHER / episode — P4.D296, P4.D270) and `consolidation_tasks/prompt_text.rs` (P4.D271); every tier-3 family byte-comparing those prompts moves. NOT touched by P4.D324–P4.D340, but P4.D339 vendors `help/` at the pin. **The message overstates:** only SELF and OTHER gained a 0.95 example; episode and consolidation got ladder text only. | UNPROCESSED |
| `a5efdbb97` | 2026-10-21 | Feature: group members get reserved full-text slots in recall | **PORT-NEW on the ported recall surfaces — a group "inner circle" step in the frozen archive; NO D23 move, NO route; model-visible (`## Those Closest to You` + full-text rows)** (classified 2026-10-26 from the hunks; 23 files +723 / −66 incl. four test files; prod 11 files +389 / −47; `dev.55`; parent `6fd2d5ffb`). NEW `lib/memory/recall/inner-circle.ts` `resolveInnerCircle(holderId, {excludeIds})` (group memberships via `groupCharacterMembers` / `groups.findByIdRaw`; tiers = RANK of the holder's distinct group sizes ascending; each candidate keeps its tightest group by size → name `localeCompare` → id; names via `characters.findNamesByIds(ids, {excludeArchived: true})` — a NEW option; sort tier → groupName → name → id; DEBUG `[InnerCircle] Resolved inner circle`), `innerCircleSignature`, `innerCircleAllowanceFor`. `recall/tags.ts` `INNER_CIRCLE_ARCHIVE_SHARE = 1/3`, `INNER_CIRCLE_ALLOWANCE = [3,2,1]` → `RECALL_TUNING_DEFAULTS.innerCircleShare` / `innerCircleAllowance`; the strict `RecallTuningInputSchema` gains both (`describeRecallTuning` appends them when non-default). `recall/archive.ts`: `getOrComputeFrozenArchive` returns `{anchors, circle}` (was `Memory[]`), resolves the circle EVERY call before the cache check, and keys the cache on `"${share}\|${allowance}\|${signature}"`; `computeFrozenArchive` gains step 3 (`cap = min(floor(size*share), size - chosen.size)`, `drawInnerCircle` via `findHotDigests`, tier-boundary break, `innerCircleTrimOrder`); anchors sorted by `id.localeCompare`; the compose DEBUG gains `circleMembers` / `circle` / `circleCap` / `filled`. `recall/render.ts` NEW `renderInnerCircle` (header `## Those Closest to You`, body `content \|\| summary`, no age tag); `assemble.ts` renders the circle first within `floor(archiveTokenBudget * innerCircleShare)`, anchors get the rest, joined `\n\n`; the trace gains `circleHeld` / `circleRendered` / `circleMembers` / `circleTokenBudget`. `RecallReplayResult.warnings`. CLI `recall-replay-command.js` help text gains the two knobs. help `groups.md` +8, `memory-recall-relevance.md` +10. **v5 intersection:** `services/frozen_archive.rs` (P4.D268 / P4.D291), `services/recall/{render,assemble,types}.rs` (P4.D291), `recall_tuning.rs` (P4.D267), `services/recall_replay.rs` + the CLI's captured `recall_replay_help.txt` (P4.D267 / P4.D269 / P4.D299), `db/characters_read.rs` `find_names_by_ids` (which ordered P4.D331 also calls — additive, same function). A cross-partition read (mount-index group membership) on a main-DB recall path. | UNPROCESSED |
| `8cb584eff` | 2026-10-21 | Feature: keystone memories | **PORT-NEW WITH THE SEVENTEENTH D23 MOVE — `memories."keystone" INTEGER NOT NULL DEFAULT 0` (`add-memory-keystone-v1`, main partition, additive, NO data rewrite) + a migration-only partial index `idx_memories_keystone`** (classified 2026-10-26 from the hunks; 56 files +1,211 / −156 incl. 12 test files; prod 32 files +636 / −122; `dev.56`; parent `a5efdbb97`). Migration: `dependsOn add-memory-tiers-v1`, `introducedInVersion 4.11.0`, runs when the column OR the index is absent; `addColumnIfMissing('memories','keystone','INTEGER NOT NULL DEFAULT 0')` (appended) + `CREATE INDEX IF NOT EXISTS "idx_memories_keystone" ON "memories" ("characterId", "aboutCharacterId") WHERE "keystone" = 1`; INFO `Added memories.keystone column` / `Created idx_memories_keystone`, result `Keystone memories in place (N changes)`, ERROR `Keystone-memory migration failed`; prettify `Setting keystones in the Commonplace Book, that certain facts never slip`. `MemorySchema.keystone: z.boolean().default(false)` between `consolidatedAt` and `createdAt` → generateDDL renders it in schema order (probably a nullable `INTEGER DEFAULT 0` — TWO shapes; verify at the re-dump); the index → a boot ensure (2026-10-19 ruling). `KEYSTONES_PER_SUBJECT_MAX = 5`. Repository: `create` defaults `keystone ?? false`; the list's `keystone` filter; `hydrateRawRows` maps number / bigint / null → boolean BEFORE `safeParse` (v5's P4.D265 hydrate needs the same or raw reads fail / drop rows); NEW `findKeystones` (hot only, `ORDER BY createdAt, id`) / `countKeystones`. Recall: present characters' keystones LIFTED into the circle (`tier: -1`), self keystones into the anchors before self digests; `renderArchive` / `renderHead` render a keystone `content \|\| summary`. Protection: `foldInto` writes no footnotes / empty patch onto a keystone survivor (`anchorPatch` extracted); consolidation skips keystones; `isDemotable` excludes them; `PolicyProtection` gains `'keystone'` FIRST (`sparedBy.keystone` first — the chat wipe, `MEMORY_REGENERATE_CHAT`, fold eligibility); `cascadeEmptiedDigests` KEEPS a keystone digest (DEBUG `Keystone digest kept though its last member retires`); scoring protection treats it like MANUAL. Admission edit: `keystone-refused` — `An archived memory cannot be a keystone; it belongs to the digest that replaced it.` / `A character may keep at most 5 keystones about one subject; unmark one first.`. Routes: PUT `keystone` (400 on refusal); create pre-checks then marks after admission (`The memory was saved, but not as a keystone: <msg>`); list `?keystone=true`. Carriers: `importMemories` `keystone ?? false`; `qtap-export.schema.json` memory `keystone`. SPA: badge, editor checkbox `Keystone — always remembered`, `Keystones only` filter. CLI `quilltap memories` `--keystone`, a NEW `keystone <id> [--off] [--write]` verb, completion templates. help ×3, `DDL.md`. **v5 intersection:** D23 re-dump + a boot ensure (join `restore_vintage_state`'s boot-align list); `db/memories_read.rs` (P4.D265 / P4.D269 / P4.D298); `services/frozen_archive.rs` + `recall/render.rs` (P4.D268 / P4.D291); `admission/fold.rs` (P4.D290); maintenance consolidate / demote / fold (P4.D271 / P4.D297); `policy_protection.rs`, `memory_retire.rs` (P4.D288 / P4.D297); scoring (P4.D289); `admission/update_memory.rs` + the memories routes (P4.D298); carriers (P4.D293 / P4.D294); memory SPA (P4.D276 / P4.D300). The `quilltap memories` CLI family is RULED OUT of v5 (`phase-4.md` R-AL) — the verb is NO-PORT, but v5's captured completion templates list memories verbs and will move. Collides with ordered P4.D324 (the round's ONE re-dump is at `322a5488e`) and P4.D327 (`qtap-export.schema.json`). **"keystone" here ≠ P4.D324's "Inventory KEYSTONE" lane title.** **The message overstates:** "every digest is cut before any keystone" holds only inside the circle; self keystones sit among id-sorted anchors with no priority. | UNPROCESSED |
| `036b32a78` | 2026-10-22 | Fix: a whisper no longer starts a turn chain of its own (bug 258) | **PORT on the ported send spine — v4-ORIGINATED bug 258 (v5 status Owed), NOT a convergence; NO D23 move, no model-visible text** (classified 2026-10-26 from the hunks; 17 files +667 / −14 incl. the 401-line `docs/developer/features/storyteller-mode.md` spec; prod 6 files +71 / −7; `dev.57`; parent `8cb584eff`). NEW `lib/services/chat-message/whisper-send.ts` `isOperatorWhisperSend(options) = continueMode !== true && (targetParticipantIds?.length ?? 0) > 0`. NEW `lib/chat/turn-control.ts` `waitForEarlierTurns(chatId)` (awaits every earlier registered stream's `settled` in insertion order; outside a registered stream waits on all; DEBUG `[TurnControl] Waiting for earlier stream(s) to finish` / `Earlier stream(s) finished` `{chatId, streams}`). `handleSendMessage`, inside `runWithTurnControl` and before `processMessage`: a whisper awaits earlier turns (DEBUG `[Orchestrator] Whisper send: single reply, no chain` `{chatId, waitedOn}`) and runs `executeTurnChain({singleTurn: options.singleTurn === true \|\| isWhisper})`. `help/chat-multi-character.md` "### Who Answers a Whisper". **v5 intersection:** `services/orchestrator.rs` `handle_send_message` / `execute_turn_chain` (`single_turn` exists from the autonomous-room port); the whisper dialog SPA (P4.9E2B U4). The wait half needs the turn-control registry ORDERED P4.D335 builds — land on or after it. | UNPROCESSED |
| `b15fe9404` | 2026-10-22 | Feature: Storyteller mode and declared tool effects | **PORT-NEW (Storyteller — a third Salon turn mode) + PORT (declared tool effects REPLACE `DESTRUCTIVE_TOOL_NAMES` with an execution-time ceiling) WITH THE EIGHTEENTH + NINETEENTH D23 MOVES — TWO migrations, both additive, no data rewrite, one a NEW TABLE, one in the LLM-LOGS partition** (classified 2026-10-26 from the hunks; 194 files +5,802 / −268; prod 172 files +5,433 / −244; `dev.58`; parent `036b32a78`). **D23:** `add-storyteller-mode-v1` (main; `dependsOn sqlite-initial-schema-v1`): `chats."turnMode" TEXT DEFAULT NULL` (NULL reads `'rotation'`; patch-only), `chats."storytellerInviteFilter" INTEGER DEFAULT 0`, `chat_messages."storytellerBid" TEXT DEFAULT NULL` (JSON), and a NEW TABLE `storyteller_rounds` (`id` PK, `chatId` NOT NULL, `triggerMessageId`, `status` NOT NULL, `bids DEFAULT '[]'`, `selection` / `closedReason` / `closedAt`, `createdAt` / `updatedAt` NOT NULL, FK `chatId → chats ON DELETE CASCADE`, `idx_storyteller_rounds_chat (chatId, status)`); `add-llm-logs-storyteller-round-id-column-v1` (llm-logs; `dependsOn add-llm-logs-autonomous-run-id-column-v1`; skips when the logs file is absent): `llm_logs."storytellerRoundId" TEXT DEFAULT NULL` + `idx_llm_logs_storytellerRoundId`. generateDDL renders every column (`turnMode` bare `TEXT`, `storytellerInviteFilter` between `isPaused` and `isManuallyRenamed`, `storytellerBid` before `pascalMeta`, `storytellerRoundId` after `autonomousRunId`) AND `storyteller_rounds` (its repository is in `RepositoryContainer`) but WITHOUT the FK and the index — those become boot ensures. **Effects (PORT):** NEW `lib/tools/effect-types.ts` (`none\|read\|write\|destroy`, `UNDECLARED_TOOL_EFFECT='write'`, `computedToolEffect`, `isWithinCeiling`, `lowerToolEffect`) + `lib/tools/effects.ts` (`BUILT_IN_TOOL_EFFECTS` from a per-tool `xxxToolEffect` export; four computed effects — `state`, `doc_open_document`, `edit_custom_tool`, `run_custom` — each with a per-ceiling note appended to the description by `applyEffectCeilingToTools`; the model-visible refusal ``Not run: `${name}` is a ${effect} call, and this turn allows only ${phrase}. Nothing was changed. Carry on without it.``). `destructive-tools.ts` DELETED: the autonomous-room gate lowers the ceiling to `write` — **the withheld set widens 3 → 5** (`doc_delete_blob`, `delete_annotation` newly withheld) — and the ceiling is enforced at EXECUTION in `executeToolCallWithContext`; `buildTools()` returns `customToolEffects` (`lib/pascal/tool-effect.ts`) + `pluginToolEffects`; triggers skip a tool above the ceiling; the Workbench library gains `effect`; `@quilltap/plugin-types` 2.9.0, MCP plugin 1.1.45 (per-server `effect`), curl 1.0.32. **Storyteller (PORT-NEW):** `lib/storyteller/*` (bid call `toolChoice:'none'`, logType `STORYTELLER_BID`; the BID/TIER/TOOLS/PLAN parser; tiers; invite probability `0.25+0.75×talkativeness`; the model-visible bid / plan sections under `[Turn note from the Salon — not spoken by any character]`; `STORYTELLER_BOARD_TIMEOUT_MS=45_000`, `STORYTELLER_CONCURRENCY_PER_PROFILE=4`; AsyncLocalStorage → `llm_logs.storytellerRoundId`; SSE `participantId` + `concurrent:true`); a dry repository scope (`lib/repositories/dry-scope.ts`, with the read/write rulebook moved VERBATIM into `method-classification.ts`); `buildContext` `purpose:'turn'\|'bid'` + `storytellerSection` LAST in the trailing list; orchestrator `holdForStoryteller` / `openRoundOnSend` (SSE `storytellerRound`, `storytellerBid`, `roundOpen`, `chainComplete` reason `'round_open'`) / `runStorytellerBid` / `handleStorytellerRun`; the finalizer writes `storytellerBid` and defers extraction / fold / danger to `runStorytellerRoundHousekeeping`; routes GET `?action=storyteller-round`, POST `?action=storyteller-run\|storyteller-ask\|storyteller-lie`, chat PATCH `turnMode` / `storytellerInviteFilter`, the chat GET wire gains both; realtime topic `storytellerRounds`; the replace restore TRUNCATES `storyteller_rounds` (never backed up); the `.qtap` schema gains `turnMode` / `storytellerInviteFilter` / `storytellerBid`; SPA `StorytellerBoard`, the sidebar's three-position mode radiogroup, the Inspector filter; help `storyteller-mode.md` NEW + six edits. **v5 intersection:** `services/tool_build.rs:690`'s `DESTRUCTIVE_TOOL_NAMES` filter + `tool_build_equivalence` (the 3 → 5 widening); the executor dispatch; `StreamMessage` `toolChoice` (P4.13/P4.14); `build_context.rs`'s trailing sections; the processMessage spine + turn chain; the trigger runner (P4.D305); the Workbench library (P4.D307); llm-logs D23 (P4.D49); realtime; the restore delete list (P4.9G); `public/schemas` (P4.D313). No v5 analogue for the dry scope or plugin tool effects. Rides ORDERED P4.D328 / P4.D331 (the new tools need `xxxToolEffect`), P4.D332 (`edit_custom_tool`'s computed effect), P4.D329 / P4.D333, P4.D334, P4.D335 / P4.D336 (`runWithTurnControl`, `chainComplete` reasons). **Comments claim what no hunk does:** `storytellerBid` "stripped from every transcript" (never copied, not stripped); `dry-scope.ts` names a nonexistent `bid.service.ts`; the llm-logs index comment's "per-run budget sum" is copy-paste. | UNPROCESSED |
| `332c650b9` | 2026-10-22 | Docs: spec for operator intervention in Pascal custom tools | **NO-PORT? — docs only, VERIFIED** (classified 2026-10-26 from the hunks; 2 files +498: NEW `docs/developer/features/pascal-operator-intervention.md` (492, "Not implemented") + CHANGELOG; no stamp; parent `b15fe9404`). Superseded by `839575d89`, which moves it to `features/complete/` with as-built notes. **v5 intersection:** none — ratify with the next `docs/v4/` re-vendor. | UNPROCESSED |
| `eb2d87788` | 2026-10-22 | Fix: every Salon LLM call reaches the LLM Inspector (#105) | **PORT — `llm_logs` row coverage on ported surfaces; NO D23 move** (`#105` is a PR number, not v4 bug 105) (classified 2026-10-26 from the hunks; 20 files +592 / −221; prod 13 files +376 / −208; `dev.59`; parent `332c650b9`). `streamMessage` logs exactly once per call through a `record()` closure: on `done` with `finishReason`, else from `finally` (a throw → `{content: partial, error}`; an abandoned stream → `error: 'The stream was closed before the provider finished'`; an exhausted stream without `done` → no error, DEBUG `Recording an LLM stream that ended without a terminal chunk`). `regenerateMessageAsSwipe` now streams through `streamMessage` (mints `newSwipeId` first as the log's `messageId` and the swipe id; cacheKey `buildCharacterCacheKey(character.id)`; `currentTurnStopSignal()`; `tools: []`; logs a `CHAT_MESSAGE` row where it logged none). `attemptRequestLimitRecovery` passes `userId` / `chatId` / `characterId` + a pre-minted `messageId`; `sendVisionRequest` takes `chatId` and logs `connectionProfileId` / `cacheUsage`; the greeting logs `connectionProfileId` / `finishReason` / `cacheUsage` / `rawProviderUsage`; cheap-LLM `core-execution` logs FAILED attempts (`{content:'', error}`) across the retry; the gatekeeper logs a failed send. **`/api/v1/images?action=generate` writes one `IMAGE_GENERATION` row per profile tried — and `IMAGE_GENERATION` is in `IMAGE_SPEND_LOG_TYPES`, so image-spend tallies now count them, failures included.** Inspector: 'Other' derived from `LLMLogTypeEnum.options`; new group members. `help/system-llm-logs.md`. **v5 intersection:** `StreamMessage` logging (P4.13/P4.14), regenerate-swipe, recovery + cheap-LLM core-execution (P4.141's timeout retry), the gatekeeper, `vision_request` (P4.D283, bug 197), the images route + spend types (P4.D283, bug 196), the Inspector SPA (P4.6as), the greeting (P4.4 unit 2). The `finally` wraps ORDERED P4.D335's `currentTurnStopSignal` seam; P4.D324 touches `initial-greeting`. | UNPROCESSED |
| `839575d89` | 2026-10-22 | Feature: operator intervention in Pascal custom tools | **PORT-NEW (+ PORT on the Pascal run path) WITH THE TWENTIETH D23 MOVE — `chat_settings."customToolIntervention" INTEGER DEFAULT 1` (`add-custom-tool-intervention-field-v1`, main, additive, default ON)** (classified 2026-10-26 from the hunks; 80 files +5,541 / −84; prod 55 files +3,435 / −74; `dev.60`; parent `eb2d87788`). Migration `dependsOn add-custom-tool-triggers-field-v1`; generateDDL renders it right after `customToolTriggers` from `ChatSettingsSchema` `z.boolean().default(true)`; its migration logs the `Added …` INFO TWICE. Format: per-parameter / per-effect `intervene?: boolean`; `INTERVENTION_FIELD_KEYS` (roll, value, llmPrompt, llmAnswer, outcome, message, state, chipLabel, title, visibility, announcer); `CustomToolInterventionSchema` (strictObject `{enabled, fields?}`) after `announceAsCharacter`; model-visible `.describe()`s; `qtap-custom-tool.schema.json` +62; `tool-draft.ts` round-trips. Engine: `executeCustomTool` records `draw` (`{form:'range',unit}` \| `{form:'dice',faces}`), `llmPromptRendered`, `tableOutcomeIndex`; accepts a replay draw, `RunPins` (incl. per-effect `EffectPin`) and a consult; `cryptoUniform` → `cryptoUnit` (same math); `poseLlmConsult`, `diceFromFaces`. Hold machinery `lib/pascal/intervention/` (a process-local registry, `KEEP_ALIVE_MS=15_000`, `SETTLED_MEMORY=200`, first resolve wins, no timeout, Stop vetoes; realtime `customToolInterventions`); veto texts `The tool did not run.` / `The table is closed for the moment; this tool did not run. Carry on without it.` (transparent when the caller seat is user-controlled or `systemTransparency`). run-service: `mayHold`, outcome `{ok:'vetoed'}`, `mayHoldRun` refuses in the job child / autonomous rooms / customTools off / intervention off; `pascalMeta.intervention` when something changed. Entrances: `run_custom` and triggers `mayHold:true`, the operator's `?action=run` `mayHold:false`. `runWithTurnControl(…, {keepAlive})`, `currentTurnKeepAlive()`. Routes `/api/v1/chats/[id]/custom-tools` GET `?action=interventions`, POST `?action=intervention-preview\|intervention-reask\|intervention-resolve`. The chat-settings PUT arm (`Invalid customToolIntervention value (must be boolean)`), the Almanack line `- **Hold Intervenable Rolls for the Operator**: …`, `help_settings`, realtime prefixes, export schema `pascalMeta.intervention`. SPA `CustomToolInterventionDialog` (731) + toggles. **Not in the message: the Workbench test-run route (`?action=test`, `successResponse({...result})`) now carries `draw` (+ `llmPromptRendered`) — a byte change on a route v5 ported (P4.D307).** **v5 intersection:** run-service + triggers (P4.D305), `run_custom` + the operator Run Tool, the chat-settings PUT arms + Almanack (the P4.D304 pattern), the Workbench library + test-run route (P4.D307), `pascalMeta` carriers, realtime. Depends on ORDERED P4.D335 (turn control) and rewrites what P4.D329 (`resolveEffects`), P4.D332 (`run_custom`, the custom-tools route), P4.D333 (the grammar twins + the schema file) and P4.D334 (BuilderForm / tool-draft) port at the pin. | UNPROCESSED |
| `946a50a04` | 2026-10-22 | Feature: drag storyteller bids into running order | **PORT-NEW — rides the UNPORTED Storyteller (`b15fe9404`); NO D23 move** (classified 2026-10-26 from the hunks; 18 files +385 / −102; prod 10 files +311 / −93; `dev.61`; parent `839575d89`). `StorytellerRunRequestSchema.keepOrder?: boolean` → `planStorytellerRun` takes the bids in de-duplicated `participantIds` order, skipping the rotation rank sort (DEBUG `Storyteller run keeps the operator's order`); the all-at-once Act/Destroy tail follows it. SPA: a sortable StorytellerBoard (mouse, long-press, keyboard), order badges, "Reset order". `help/storyteller-mode.md`. **v5 intersection:** none — port with `b15fe9404` as one unit. | UNPROCESSED |
| `778b97b2b` | 2026-10-22 | Feature: Novel roleplay template | **PORT + PORT-NEW WITH THE TWENTY-FIRST D23 MOVE — `roleplay_templates."unmarkedStyle" TEXT` + `"markdownAsterisks" INTEGER DEFAULT 1` (`add-roleplay-template-unmarked-fields-v1`, main, additive) — PLUS A BOOT-SEED DATA REWRITE: `narrationDelimiters` becomes JSON-encoded** (classified 2026-10-26 from the hunks; 34 files +1,458 / −211; prod 22 files +1,127 / −194; `dev.62`; parent `946a50a04`). Migration `dependsOn add-narration-delimiters-field-v1`, prettify `Setting aside a shelf for the unadorned novel`; `DDL.md` shows both columns APPENDED after `updatedAt` while Zod places them before `createdAt` — two shapes, re-dump from live generateDDL (verify whether `narrationDelimiters`' own line moves now that it is `.nullable().default('*')`). Schema: `WrapDelimiterSchema` `matchCurlyQuotes?` / `closeOptional?`; `RenderingPatternSchema.scope` adds `unmarked` / `literal` / `strip` and `className` is `.refine`d (`className is required for span-styling patterns`); NEW `UnmarkedStyleSchema`; `narrationDelimiters` nullable. NEW `roleplay-template-narration-codec.ts`: `encodeNarrationDelimiters = JSON.stringify` (so `*` is STORED as `"*"`, `null` as `null`), a lenient decoder; the repository overrides five read/write methods. **The every-boot seed `$set`s an ENCODED `narrationDelimiters` on existing built-ins and INSERTS a third built-in `NOVEL_TEMPLATE`** (model-visible `NOVEL_SYSTEM_PROMPT`, `[SYSTEM INSTRUCTION: NOVEL FORMATTING]`, seven rules + an example — quote verbatim from `lib/chat/novel-template.ts`). `annotations.ts` `generateRenderingPatterns(…, {unmarkedStyle, markdownAsterisks})` (prepends `strip` / `literal` asterisk patterns, appends an `unmarked` `[\s\S]` pattern; the curly-tolerant / close-optional wrap); the renderer NESTS inline rules up to `MAX_INLINE_NESTING = 3` (changes rendering for EVERY template); server post-process applies the unmarked class to direct block text; `template-prompt-hint.ts` gains three model-visible bullets; routes: POST allows `null` narration (`Narration delimiters are required (null for unmarked prose)`, `Invalid unmarked style: …`, `markdownAsterisks must be a boolean`), PUT regenerates when any of four fields changes, GET regenerates empty patterns. `qtap-export.schema.json`; SPA editor +263, the toolbar withholds Bold. **v5 intersection:** `db/roleplay_templates.rs` (P4.4u3 seeds — `narration_storage` writes a BARE `*`; the read is a RAW passthrough), `services/annotations.rs` (P4.6p), `template_prompt_hint.rs`, the roleplay-templates routes family; SPA `chat/render/*`, `screens/settings/templates/*`, the composer toolbar (p4.9l). ORDERED P4.D327 / P4.D339 (the export schema, help). | UNPROCESSED |
| `eb90d9ed8` | 2026-10-23 | Feature: wardrobe pronoun guidance | **PORT — a NEW vault frontmatter key `pronounGuidance` on wardrobe item `.md` files (no SQL, no migration) + model-visible tool params; ⚠ LIVE-DATA: v5's struct rebuild DROPS the key** (classified 2026-10-26 from the hunks; 39 files +895 / −36; prod 29 files +598 / −28; `dev.63`; parent `778b97b2b`). NEW `lib/characters/pronoun-presets.ts` (`STANDARD_PRONOUNS` — the four Aurora presets, now shared with the picker; `standardPronounSubjectOf`); NEW `lib/wardrobe/pronoun-guidance.ts` (`normalizePronounGuidance` — array or string, trim / lowercase / cut at `/`, known subjects only, de-duplicated, picker order; `fitsPronounGuidance`; `filterByPronounGuidance(items, pronouns, keepIds)` → `{items, hiddenCount}`). `PronounGuidanceEnum` on `wardrobeItemFieldsSchema` (REST create / update; an unknown value is a Zod 400) and `WardrobeItemSchema`; the parser spreads the key only when non-empty; the writer emits a YAML list after `default` / `replace`, before `archived`. Tools: `wardrobe_create` / `wardrobe_update` gain `pronoun_guidance`, `wardrobe_list` gains `pronoun_aware` (default true; keeps every worn id; WARN `Wardrobe list could not read the character's pronouns; listing unfiltered`), the list output gains `pronoun_guidance` + `hidden_by_pronoun_guidance`, rows ` [meant for: <labels>]`, the trailing `(N more item(s) are meant for characters with other pronouns; call wardrobe_list with pronoun_aware: false to see them.)`; `wardrobe_read` `  pronoun guidance: meant for <labels>`. The Green Room's "Let character choose" filters by pronouns (DEBUG `[applyOutfitSelections] Pronoun guidance held items back from the green room`). SPA: `PronounAwareToggle`, `WardrobePronounGuidanceField`. `help/wardrobe.md`. **v5 intersection:** `vault_overlay.rs` `parse_wardrobe_item_file` / `build_wardrobe_item_file` (P4.D279), `tools/wardrobe_list.rs` + `wardrobe_tool_table.rs` + `definitions/data.rs` (P4.D280 / P4.D262), `services/outfit_selections.rs` (P4.D282), the REST factory (P4.D281), the wardrobe SPA (P4.D285 / P4.D286), the Aurora pronoun picker, the `.qtap` carriers (P4.D284). No ordered order touches it. | UNPROCESSED |
| `8ae8a127c` | 2026-10-23 | Feature: min/max ranges in the Pascal intervention dialog | **PORT-NEW — rides the UNPORTED intervention (`839575d89`); NO D23 move** (classified 2026-10-26 from the hunks; 14 files +259 / −12; prod 7 files +143 / −8; `dev.64`; parent `eb90d9ed8`). `CustomToolRunResult` always carries `rollSpan` / `valueSpan` (`RunSpan {min, max, minExclusive?, maxExclusive?}`; range form `maxExclusive` unless `min===max`; `transformSpan` through multiply / offset / round; dice `{count+mod, count×sides+mod}`; spans ignore pins). NEW client-safe `intervention/spans.ts` (`withinSpan`, `describeSpan`, `toPrecision(4)`); `recomputeRun` warnings `The pinned draw lies outside what the table could deal (…).` / `The pinned value lies outside what the modifiers could reach (…).`. **Not in the message: the spans reach the Workbench test-run wire through `...result`** (P4.D307). **v5 intersection:** `executeCustomTool` (the ordered P4.D329 / P4.D333 lanes rewrite the same neighbourhood at the pin) + the test-run route. Fold into the intervention lane. | UNPROCESSED |
| `99463fc56` | 2026-10-23 | Feature: always-on floating Brahma Console | **PORT (SPA only) — no server, wire or schema change** (classified 2026-10-26 from the hunks; 24 files +949 / −408; prod 13 files +785 / −394; `dev.65`; parent `8ae8a127c`). NEW `BrahmaConsoleLauncher` (portalled into `document.body`, never disabled; "Open the Brahma Console" / "Hide the Brahma Console", `aria-pressed`); NEW `components/ui/floating-layer.ts` (`data-qt-floating-layer`); `FloatingDialog` `topmost` (Escape on the dialog element, stops propagation); `useClickOutside` ignores the floating layer; one `BrahmaConsolePanel` for tab + dialog; per-surface sessions (`quilltap:brahma-floating-last-id` vs `quilltap:brahma-console-last-id`); `openConsole` no longer resets `currentChatId`; the model picker on the roster, defaulting to the `isDefault` profile (sent as `connectionProfileId` on the existing POST); `--qt-floating-layer-z: 1000`. `help/brahma-console.md` rewritten (HelpChat direction text changes). **v5 intersection:** `apps/web/src/app/brahma/*` (P4.9I1B) + `shell/shell.ts`; check the z-1000 layer against ORDERED P4.D338's toast z-index rider. | UNPROCESSED |
| `265b711dd` | 2026-10-23 | Fix: wardrobe and inventory tools ask for a picture cue | **PORT (wardrobe half) + PORT-NEW (Inventory half — a post-pin rider on ORDERED P4.D328); NO schema move; model-visible** (classified 2026-10-26 from the hunks; 16 files +139 / −36; prod 6 files +75 / −23; `dev.66`; parent `99463fc56`). NEW `lib/tools/image-cue-guidance.ts` (`IMAGE_CUE_TARGET_CHARS = 200`, stated not enforced; `CUE_STYLE`); `wardrobe_create` / `wardrobe_update` `image_prompt` + `description` texts rewritten (`WARDROBE_DESCRIPTION_GUIDANCE`), the `wardrobe_create` description; Inventory `image_description` (`INVENTORY_IMAGE_DESCRIPTION_GUIDANCE`) + `inventory_create`'s description. **Behaviour fix:** `CONTENT_PARAM_MAP` gains `wardrobe_create: 'description'` — a `[[CREATE_WARDROBE_ITEM]]` body previously went to `content`, which the tool drops; v5's `text_block_parser.rs::content_param` has the pre-fix fall-through. The text-block prompt's Create Wardrobe Item / Create an Object lines + examples (`cue="…"`, `looks="…"`). help `wardrobe.md`, `inventory.md`. **v5 intersection:** `tools/definitions/data.rs` (P4.D280), `tools/text_block_prompt.rs` + `text_block_parser.rs` (Phase 3); the Inventory strings arrive as a delta on P4.D328's pin-time definitions. | UNPROCESSED |
| `4a25f7eab` | 2026-10-23 | Docs: draft 4.11.0 release notes | **NO-PORT? — docs only, VERIFIED** (classified 2026-10-26; 2 files +329: NEW `docs/releases/4.11.0.md` + CHANGELOG; no stamp; parent `265b711dd`). **v5 intersection:** the `docs/v4/releases/` mirror (v5's `self_inventory` reads release notes through an env seam); re-vendor with the next docs lane. | UNPROCESSED |
| `fa126bde8` | 2026-10-23 | Fix: every character gets every attachment in multi-character chats (bug 259) | **PORT — the 80,000-character rehydration cap REMOVED; v4-ORIGINATED bug 259 (v5 status Owed), NOT a convergence; NO D23 move** (classified 2026-10-26 from the hunks; 10 files +147 / −38; prod 1 file +13 / −33; `dev.67`; parent `4a25f7eab`). `context-builder.service.ts` deletes `REHYDRATED_ATTACHMENT_CHAR_BUDGET = 80_000`, the skip, and the WARN `Re-hydrated attachments exceeded the per-turn budget; some were not re-sent`; every unseen upload's fallback text is appended whole; the DEBUG `Re-hydrated user attachments from history` keeps `charactersUsed` as a running sum. `help/chat-multi-character.md` "### Everyone Reads What You Hand Round". **v5 intersection:** `services/message_context.rs` `REHYDRATED_ATTACHMENT_CHAR_BUDGET` (`:623`) + `rehydrate_user_attachments` (P4.D154, v4 bug 121) — delete all three together and re-pin the bug-121 differential's budget rows. | UNPROCESSED |
| `40fab1c54` | 2026-10-23 | Fix: doc tools can read, grep and search chat uploads (bugs 260, 261) | **PORT + PORT-NEW — v4-ORIGINATED bugs 260 / 261 (v5 Owed), NOT convergences; NO D23 move but DATA WRITES: ingest now fills `doc_mount_blobs.extractedText*` / `doc_mount_file_links.conversionStatus` for native-text uploads, and a NEW idempotent BOOT BACKFILL rewrites existing upload rows in the mount-index partition and enqueues embedding jobs** (classified 2026-10-26 from the hunks; 28 files +1,112 / −108; prod 11 files +495 / −99; `dev.68`; parent `fa126bde8`). `path-utils.ts` `detectBlobFileType` exported, `EXTRACTABLE_BLOB_EXTENSIONS` (`.md .markdown .txt .json .jsonl .ndjson .pdf .docx`). `store-file.ts` NEW `extractBlobText` (PDF / DOCX via the converter, native text as UTF-8; the WARN renamed `extractBlobText: text extraction failed` with key `blobType` (was `fileType`)); `storeMountFile` extracts whenever `extractText` is set. The project-store + user-uploads bridges flip `extractText: true` + `enqueueEmbedding: true`. `database-store.ts` `readDatabaseBlobText` / `listDatabaseBlobTexts`; `doc-mount-blobs.repository.ts` `listUnextractedByExtensions`. NEW `lib/startup/backfill-upload-blob-text.ts` (logger `Startup:UploadBlobTextBackfill`; INFO `Extracting text from uploaded blobs` / `Uploaded-blob text backfill finished`; four WARNs), chained after `scanAllMountPoints()`. `doc_read_file` falls back to the blob's text on ENOENT in database stores; `doc_grep` also searches blob texts; `doc_list_files` replaces `matchesGlob` with a real case-insensitive path glob (NEW `lib/doc-edit/path-glob.ts`) and its `pattern` description gains a model-visible sentence. help ×2. **v5 intersection:** `tools/doc_edit/text.rs` (`matches_glob` `:1510`, Phase 3 W4.1d3b), `services/mount_index/store_file.rs` (the `:441` pdf/docx gate, P4.6ah), the upload bridges, host boot after the mount scan. Collides with ORDERED P4.D338 (bug 256 in the same `text.rs`), P4.D332 (`text.rs:266`), P4.D340 (`store_file.rs:313`). | UNPROCESSED |
| `074e1e01a` | 2026-10-23 | Feature: self_inventory core-file checklist and holdings counts | **PORT (`self_inventory`) + PORT-NEW sub-part (`holdings.inventory` rides the UNPORTED Inventory); NO schema move; model-visible — EVERY default call's output changes** (classified 2026-10-26 from the hunks; 16 files +690 / −36; prod 6 files +487 / −23; `dev.69`; parent `40fab1c54`). `SELF_INVENTORY_SECTIONS` gains `vault.coreFiles` (after `vault.groups`) and `holdings` / `holdings.wardrobe` / `holdings.inventory` / `holdings.customTools`; a bare `vault` and the no-argument report now include both. The schema description text grows. `buildVaultCoreFilesSection` walks the nine `SINGLE_FILE_OVERLAY_PATHS` (case-insensitive), each `{path, exists, fileSizeBytes, uri (the bug-256 named form), holds}` from NEW `CORE_VAULT_FILE_SUMMARIES` (nine second-person strings — port verbatim); unavailable `mount_index_degraded` / `no_vault` / `error`; text `## Core Vault Files`, `<mount>: N of 9 present`, rows `- <path> [exists, <bytes>\|exists, empty\|MISSING] — <holds>` + `    uri: <uri>`, `(To fill in a missing or empty file, write it with doc_write_file at its uri.)`. NEW `self-inventory/holdings.ts` (each part fails soft): wardrobe garments / outfits / by tier; Inventory objects / kits / archetypes / carried here; custom tools (`Custom tools are switched off for this instance.` or the roster, names `localeCompare`). help ×2, `FEATURES.md`. **v5 intersection:** `tools/self_inventory.rs` (Phase 3; P4.15). The named URI waits on ORDERED P4.D338; `holdings.inventory` on P4.D324 / P4.D328; `holdings.customTools` on P4.D332's reshaped roster. | UNPROCESSED |
| `b02d902f3` | 2026-10-24 | Fix: Pascal hold dialog can re-ask a hidden consult | **PORT-NEW — a fix inside the UNPORTED intervention (`839575d89`); NO D23 move** (classified 2026-10-26 from the hunks; 11 files +119 / −21; prod 3 files +36 / −7; `dev.70`; parent `074e1e01a`). `handleInterventionReask` refuses only when the definition has no `llm` (NEW 400 `This tool consults no oracle`; the old `This run's consult is not open to intervention` is gone); the dialog's compact "The oracle" section. **v5 intersection:** none — fold into the intervention lane. | UNPROCESSED |
| `f7a1789df` | 2026-10-24 | Feature: referent ledger and closing echo in the trailing context | **PORT-NEW — two new trailing context sections; NO D23 move; model-visible** (classified 2026-10-26 from the hunks; 9 files +504 / −6; prod 2 files +338 / −1; `dev.71`; parent `b02d902f3`). NEW `lib/chat/context/referent-ledger.ts` (`REFERENT_LEDGER_WINDOW_LINES=6`, `MAX_PEOPLE=8`, `QUOTE_CHARS=220`, `CLOSING_ECHO_MAX_CHARS=1500`; a case-SENSITIVE `u`-regex name matcher, longest first; `sentenceAround`; `whereNamed`). Model-visible: `## Who is being spoken of` + its guidance paragraph + entries `- **Name** (subj/obj \| pronouns not recorded; in the room \| not in the room) — last named <where>: “quote”`; `## The line you are answering` / `What ${speaker} just said, which is what you are replying to:` (the partial form over 1,500 chars; `> ` prefixed). WARN `[ReferentLedger] Could not build the referent ledger; turn continues`. The trailing list becomes `[coreWhisper, recall, mail, progressions, inventory, inform, referentLedger, closingEcho, turnSkip, turnYield, storytellerSection]`. `help/salon-host-introductions.md`. **v5 intersection:** `services/build_context.rs` `trailing_context_sections` (inform from P4.D254); `speaker_names.rs` / `chat_predicates.rs` reusable. ORDERED P4.D324 (the Inventory section) and P4.D335 (`turn_yield`) rewrite the same list — land after both. Port with `d68c1af88` + `9a88f862f` as ONE lane. | UNPROCESSED |
| `d68c1af88` | 2026-10-24 | Change: absent-character cards move from the history to the referent ledger | **PORT — Host "spoken of" cards become record-only; NO D23 column but a NEW persisted `systemKind` value `off-scene-left-behind`** (classified 2026-10-26 from the hunks; 16 files +287 / −78; prod 6 files +139 / −56; `dev.72`; parent `f7a1789df`). `buildContext` stops pushing the freshly posted off-scene announcement as a `user` message. NEW `off-scene-kinds.ts` (`HOST_KIND_OFF_SCENE_LEFT_BEHIND`, `LEFT_BEHIND_INTRO_PREFIX='The Host observes that'`, `isOffSceneMentionIntroduction`); `isRecordOnlyMessage` + `extractVisibleConversation` (the compression-summary input) skip mention introductions; Continue Elsewhere's left-behind notice is written with `systemKind` `off-scene-left-behind`; absent listed people's cards are appended to the ledger under `Particulars of those not in the room — for accurate reference only. They are not present, cannot hear what is said here, and are not summoned by being named.`; an unseated persona counts as in the room under `operatorSpeaksWithoutSeat`. `PROMPT_ARCHITECTURE.md`, help. **v5 intersection:** `services/off_scene.rs` (W4.6b), `services/host_notifications.rs:1215`, `services/chat_continuation.rs` (P4.D233, bugs 171/172), `build_context.rs`'s `pending_off_scene_announcement` (`:1659`), the `is_record_only_message` family, `chat_tasks.rs`. An intermediate state `9a88f862f` partly rewrites — one lane with `f7a1789df`. | UNPROCESSED |
| `9a88f862f` | 2026-10-24 | Feature: name chips for absent characters replace the Host's introduction cards | **PORT + PORT-NEW — the per-turn Host off-scene posting DELETED; NEW `GET ?action=absent-mentions` + Salon name chips; NO D23 move** (classified 2026-10-26 from the hunks; 41 files +989 / −636; prod 22 files +663 / −359; `dev.73`; parent `d68c1af88`). `buildContext` deletes the off-scene scan / post block (its reads, the persona-exclusion DEBUG, `findMentionedCharacterIds`, `postHostOffSceneCharactersAnnouncement`, the WARN `[ContextManager] Failed to compute off-scene character introductions`) — only Continue Elsewhere's left-behind notice remains. DELETED `lib/chat/context/mentioned-characters.ts` + `findIntroducedOffSceneCharacterIds`. NEW `lib/chat/name-mentions.ts` (the ledger's matcher, shared), `lib/chat/absent-mentions.ts` (`presenceOf`, `spokenText`, `findAbsentMentions` — first naming message per absent candidate). `writer.ts` `OffSceneCardFields` / `offSceneCardFields()` (same card bytes). Route `actions/absent-mentions.ts` registered between `scenario-source` and `cost` (changes the unknown-action 400's key list); realtime invalidates `['chats', id, 'absent-mentions']`. SPA `MentionChip`, a rehype marker plugin, SalonView filters the mention rows. `API.md`, help rewritten. **v5 intersection:** `build_context.rs`'s off-scene seams (W4.6b; persona exclusion P4.D233), `mentioned_characters.rs` (Phase 1 B14 — becomes dead), `services/off_scene.rs`, the chat GET dispatcher's action list (ORDERED P4.D325 adds the Inventory actions to the same list), the realtime topic map, the Salon renderer (P4.D336). | UNPROCESSED |
| `b27e5089a` | 2026-10-24 | Add humanoid flag to physical descriptions for non-humanoid characters (#106) | **PORT — a NEW key `humanoid` in vault `physical-prompts.json` (no SQL, no migration) threaded through every image prompt and the identity stack; ⚠ LIVE-DATA: v5's 5-key writer ERASES `humanoid: false`** (classified 2026-10-26 from the hunks; 43 files +941 / −163; prod 27 files +532 / −155; `dev.76` (three stamps); parent `9a88f862f`). `CharacterVaultPhysicalPromptsSchema.humanoid` lenient (`false`/`"false"`, `true`/`"true"`, else absent); `renderPhysicalPromptsJson` ALWAYS writes a sixth key `"humanoid": primary?.humanoid ?? true` after `complete`; the scaffold too. Hydration sets `physicalDescription.humanoid = parsed ?? true` after `updatedAt` on EVERY vault character (the character read wire + export move); a physical write without `humanoid` keeps the vault's value; the character PUT accepts it and on change calls NEW `recompileCharacterSeats` (the subprompt fan-out folded into `system-prompt-compiler/character-fanout.ts`; its two WARN texts change + a new DEBUG). `NON_HUMANOID_IMAGE_DIRECTION`; `pickPhysicalDescription('head-and-shoulders')` order for non-humanoids; `figureGender*` null. The identity stack's `## Physical Appearance` gains "You have no humanoid body: no head, face, shoulders, hands or limbs. Never describe yourself as having or using one." (emitted even with no description). The avatar block, the wardrobe item picture, `sceneDescriptionParts`, story backgrounds, `craftImagePrompt` (` [not humanoid]` + a system-prompt rule), `updateSceneState`, `resolveAppearance`, the optimizer — each gains a non-humanoid branch (strings verbatim in the hunks). The head-and-shoulders backfill + its startup enqueuer SKIP a non-humanoid character. `CORE_VAULT_FILE_SUMMARIES['physical-prompts.json']` gains the flag. SPA Aurora "Humanoid" checkbox. `help/character-editing.md`. **v5 intersection:** `vault_overlay.rs::parse_vault_physical_prompts`, `db/{character_vault,vault_read_overlay,vault_character_write,vault_character_update}.rs`, `system_prompt.rs` (P4.D103 / P4.D164), `services/avatar_prompt.rs` (P4.D283), `wardrobe_item_image_prompt.rs` (P4.D263), `story_background_job.rs` (P4.D239), the appearance resolver (P4.D282 / P4.D283), `headshoulders_backfill_job.rs` (P4.82), `subprompts/fanout.rs` (P4.D163 / P4.D164), `pronoun_gender.rs`, the optimizer (P4.9K). Layers on top of ORDERED P4.D326's `sceneDescriptionParts` / avatar carrying text. | UNPROCESSED |
| `1dd0a0c01` | 2026-10-24 | Feature: punctuation warnings in the Salon composer | **PORT-NEW — a composer punctuation lint; NO column, NO migration: a new JSON key `punctuationWarnings` (default true) inside `chat_settings.smartTypographySettings`; not model-visible** (classified 2026-10-26 from the hunks; 24 files +1,356 / −11; prod 11 files +1,123 incl. a 392-line fixture; `dev.77`; parent `b27e5089a`). The column DEFAULT text is UNCHANGED; `ChatSettingsSchema`'s object default and the repository's create default gain the key, so new rows store four keys and every read / wire carries it. NEW pure `lib/punctuation-lint/engine.ts` (imports nothing; "travels to v5"; eight rules; `PUNCTUATION_RULE_MESSAGES`) + `fixtures/punctuation-vectors.json` — marked as **the v5 contract corpus**. A Lexical plugin (CSS Custom Highlight API), the settings toggle "Warn about punctuation that looks wrong", CSS, `help/chat-settings-composer.md`. **v5 intersection:** `SmartTypographySettings` in `db/chat_settings.rs` / `api/settings.rs` / `chat_settings_composer_repair.rs` (P4.D73); SPA `smart-typography/` (the engine + fixtures pattern), `smart-typography-settings.ts`, the ProseMirror composer (P4.D74). ORDERED P4.D336 edits `chat-composer.ts`. | UNPROCESSED |
| `505a524fa` | 2026-10-24 | Docs: 4.11.0 release notes corrections | **NO-PORT? — docs only, VERIFIED** (classified 2026-10-26; 2 files +8 / −3, `docs/releases/4.11.0.md` + CHANGELOG; no stamp; parent `1dd0a0c01`). **v5 intersection:** the `docs/v4/releases/` mirror; fold with `4a25f7eab`. | UNPROCESSED |
| `0276d10d9` | 2026-10-25 | Refactor: Inventory chokepoints (#107) | **PORT-NEW — a post-pin rider on the UNPORTED Inventory, NOT output-neutral despite "Refactor": it changes what ORDERED P4.D324 / P4.D325 / P4.D326 / P4.D328 / P4.D330 specify at the `322a5488e` pin; no schema or file-format move** (classified 2026-10-26 from the hunks; 55 files +1,286 / −719; prod 42 files +867 / −713; `dev.81`; parent `505a524fa`). (1) `?action=duplicate` parses NEW `duplicateInventorySchema` (`itemId is required`) BEFORE the location resolves → a missing `itemId` is a Zod 400 where it was a 404 `Inventory object` (contradicts P4.D325's duplicate-404 note). (2) `withInventoryErrors(handler)` wraps WHOLE handlers (route-factory create / instantiate / folder ops / move / copy / duplicate / item PUT / clear-override / detach / DELETE; the images route; transfers; the chat `inventory-equip` / `-unequip`) → refusals thrown by resolve / pool load / `findHome` / conversion map through `inventoryErrorResponse` instead of 500. (3) The chat `?action=inventory` GET and the reset-defaults gate use `seatedCharacterIds` (any participant with a `characterId` not `removed`) — dropping the `type === 'CHARACTER'` filter P4.D325 specifies. (4) Realtime: every `InventoryLocation` write publishes `inventory` (so operator REST writes now publish); handler-owned publishes removed (P4.D328's R-17); NEW `writeCarriedSet` publishes `chats/{id}` on every carried-set write incl. arrival + reset-defaults; the topic map adds `archetypeLists` / `equippedLists`, removes three query keys. (5) Text: `[InventoryEquip] Refused` gains `op`; the arrival WARN gains `op:'equip'` first; the pick-up no-vault error becomes `Character ${id} has no vault to hold a picked-up object` (P4.D324 pins the old text); `detachAll` resolves every location before writing + NEW WARN `[InventoryMutations] Skipped instances whose vault is no longer linked`. (6) `equipItems` / `unequipItems` / `pickUpItems` / `equipItemsHoldingLock` async; `InventoryNotAParticipantError` moved (same code / text / 400); `inventory_update`'s returned item resolved through `ctx.serverPool.resolve` (edge differences possible). (7) Intended neutral: `readDuplicateInventoryPaths` (P4.D324 names it), the pool's `Elsewhere` external-parent origin (P4.D324 says pin it), `kinOf`, `describeEquippedInventoryForImages` (P4.D326), `loadToolChat` (P4.D328) DELETED; `help/inventory.md` "keeps itself current". **v5 intersection:** none landed — every hit is in the ordered Inventory lanes; port as post-pin riders after them. | UNPROCESSED |
| `a8214b253` | 2026-10-25 | Refactor: Salon turn pipeline (#108) | **PORT — the processMessage spine split into phase modules (mostly a faithful move) carrying bug 262 (v4-ORIGINATED, found while refactoring; v5 Owed) + small observable shifts; NO D23 move** (classified 2026-10-26 from the hunks; 39 files +3,760 / −2,539; prod 22 files +2,705 / −2,530; `dev.83`; parent `0276d10d9`). `orchestrator.service.ts` 2,840 → 1,170 lines over `turn-holds` / `operator-post` / `turn-preparation` / `turn-tools` / `turn-outcome` / `turn-stream`; no model-visible string changes. **Bug 262:** the fair-rotation pause (`maybePauseForUserSeatTurn`) now records through the ONE `recordOperatorPost`: staged pending TOOL rows first (flips `pendingToolResultsPersisted`), auto-rolled dice as `rng` TOOL rows, links ONLY the chat's own files (NEW `findAttachedFiles`), the Librarian upload note, Pascal `runUserMessageTriggers` (possible consult spend), then Carina; still no danger flags (the message's claim that they are written is FALSE). Held posts (paused / storyteller) are recorded even with NO API key (was a throw + `userMessagePersisted:false`). `addLink` on the main path fails SOFT (WARN `[OperatorPost] Failed to link attachment to user message`). Log re-tags / merges / removals (`[OperatorPost]` / `[TurnStream]` / `[TurnPreparation]` loggers; the two hold INFOs merged into `[Orchestrator] Recording the operator post without a reply`; three WARNs removed so a throw there becomes an error frame; new DEBUGs). **SSE:** the five turn-outcome `done` frames go through NEW `encodeTurnDoneEvent` — key order `done,usage,cacheUsage,attachmentResults,provider,modelName,messageId,participantId,toolsExecuted,<flags>` (values identical); two use `safeEnqueue`. **v5 intersection:** `services/orchestrator.rs` `maybe_pause_for_user_seat_turn` `:861` (P4.D60 unit 2 — still the thin write; Carina deferred by `user_message_carina`), the main-path post handling `:1840–2075` (links with `?`); the P4.D186 paused hold. ⚠ v5 has NO non-test caller of `post_librarian_upload_announcement` (a pre-existing gap the reader found — measure). ORDERED P4.D335's cited v4 line ranges MOVED (`runWithTurnControl` → `turn-stream.ts`; settle / yield → `turn-outcome.service.ts`), as did P4.D328's (`:1015`, `:1758`) and P4.D332's roster site — behaviour at the pin unchanged. | UNPROCESSED |
| `cb94e6e99` | 2026-10-25 | Remove the unread debugLLMRequest SSE frame (#110) | **NO-PORT? — v4 deletes the `debugLLMRequest` SSE frame v5 never emitted** (classified 2026-10-26 from the hunks; 10 files +5 / −159; prod 5–6 files −77; `dev.84`; parent `a8214b253` — a parallel branch to the diagrams commit). Removed: `encodeDebugInfo`, `StreamDebugInfo`, `StreamChunkData.debugLLMRequest`, `TurnToolSlate.enabledToolOptions` (`determineEnabledToolOptions` keeps its export, no production caller). The frame was the only wire consumer of `builtContext.tokenUsage` (why `bdd09b75b`'s `tokenUsage` changes are unobservable). **v5 intersection:** none in production — `orchestrator.rs:2915` / `chat_events.rs:365` comments call it "unported"; `orchestrator_tier3_equivalence.rs:1325`'s drop of v4's frame goes VACUOUS at a later pin (retire as a rider); keep `pseudo_tool::determine_enabled_tool_options` (`pseudo_tool_prompts_equivalence`). | UNPROCESSED |
| `bdd09b75b` | 2026-10-25 | Refactor: decompose context manager into phase modules (#109) | **PORT — `buildContext` split into `lib/chat/context/turn-*.ts` carrying bugs 263 / 264 / 265 (v4-ORIGINATED, found while refactoring; v5 Owed); PROMPT BYTES CHANGE (fewer history rows ride; group compression cuts history); NO D23 move** (classified 2026-10-26 from the hunks; 42 files +3,215 / −2,199; prod 23 files +2,335 / −2,084; `dev.85`; parent `cb94e6e99`). Faithful otherwise (trailing order + separator, skip / yield bytes, whisper posting order, the compression gate, the narration anchor). **Bug 263:** `remaining = safeInputLimit − countMessagesTokens([block1, block2, block3?, timestampLine?, trailingDraft]) − reservedOutgoingTokens` where the trailing draft is the new user message + EVERY trailing section (Core, recall, mail, progressions, Inventory, inform, referent ledger, closing echo, skip / yield, storyteller); `countMessageTokens` = `estimateTokens(role)` + 4 per message, +3 per array; the scene note computed over every eligible row first; `selectRecentMessages` prices each row at `countMessageTokens + nameOverhead` (fewer rows fit EVERYWHERE); its keep-the-last fallback keeps `id` (the bug-95 anchor); the warning gains "trailing sections"; the `Message budget exhausted` WARN fields shrink. **Bug 264:** the multi-character branch slices `messagesWithParticipants` from the compression `windowStart` (NEW WARN `[ContextManager] Could not place the compression window in the cast history; keeping it whole`). **Bug 265:** `verbatimWindowStart` counts only `toVisibleConversationMessage` rows. Also: `isInWhisperScope` treats an EMPTY `targetParticipantIds` as untargeted (Commonplace sweep, the fold refresh, the standing-conversation dedup); sweep failures `logger.error` → `logger.warn`, the fold refresh's WARN renamed + a NEW INFO `[RelevantConversationsRefresh] Swept stale whispers`; a throwing `postCoreWhisper` now DROPS the Core section; memory-compression warning / log texts change (`Error during memories compression: …`, `[ContextManager] Compressed memories`, `getErrorMessage`); ONE clock read for the timestamp + scene time; ONE memoised transcript read (`[TurnTranscript]` DEBUG / WARN — the reader judged it unobservable for Core but did NOT prove it for `findLatestContextTransitionIndex`); `chat.types.ts` +37. help ×2, `PROMPT_ARCHITECTURE`. **v5 intersection:** `services/build_context.rs` (Phase 3 + the P4.D78–81 budget / reservation) has every pre-fix shape — the window walk `:2006–2019`, the multi-character branch `:2464+`, the budget `:2450–2600`; `message_selector.rs`'s bare `+4` `:69–75`. The 263 budget is defined over the trailing list ORDERED P4.D324 + P4.D335 extend — land after both (and after `f7a1789df`'s sections). | UNPROCESSED |
| `6588972d5` | 2026-10-25 | Feature: diagrams in the Salon (iteration 1) | **PORT-NEW — TWO NEW vault keys in `properties.json` (`canDrawDiagrams`, `diagramStyle`; no SQL, no migration), an opt-in system-prompt note, wire fields, and client-only rendering with FOUR NEW client dependencies; ⚠ LIVE-DATA: v5's six-key `properties.json` writer DROPS both keys** (classified 2026-10-26 from the hunks; 68 files +4,025 / −231; prod 46 files +1,687 / −140 + `package.json` +4 deps; `dev.84`; parent `a8214b253`). `canDrawDiagrams` (absent = `false`), `diagramStyle` (`'handDrawn'\|'clean'`, leniently `'handDrawn'`), appended after `canChooseOutfit` in the full writer, the RMW patch writer (`propsKeys` → 8) and the scaffold — so EVERY v4 properties write now writes both; `MANAGED_FIELDS`, the read overlay, `CharacterSchema`; DEBUG `[VaultOverlay] Writing diagram settings to properties.json`. Model-visible: `DIAGRAM_FORMATTING_INSTRUCTION` (`[FORMATTING: DIAGRAMS]` — copy verbatim from `system-prompt-builder.ts`) pushed right after `MATH_FORMATTING_INSTRUCTION` in block 1 ONLY when `canDrawDiagrams === true` (otherwise byte-identical; `IDENTITY_STACK_BUILDER_VERSION` unchanged). Wire: the character PUT accepts both; the list GET emits both; `getEnrichedCharacter` / `getCharacterDetail` emit `diagramStyle` after `archivedAt`; `.qtap` schema +2. Not updated: `CORE_VAULT_FILE_SUMMARIES`' `properties.json` line. Client: `lib/markdown/diagrams.ts` fences (mermaid / dot / graphviz / svg), renderers under a DOMPurify allow-list, the Aurora toggle. Deps (client-only): `mermaid ^12.2.0`, `@hpcc-js/wasm-graphviz ^1.30.0`, `dompurify ^3.4.17`, `svg2roughjs ^3.2.3` — the Angular renderer needs a dependency RULING. `help/diagrams.md` NEW (117). **v5 intersection:** the vault properties reader / writers (P4.6bh lineage): `vault_overlay.rs`, `db/{character_vault,vault_character_write,vault_character_update,vault_read_overlay,characters,characters_read,store_name_reconcile}.rs`, `api/characters.rs`, `services/character_enrichment.rs`; `system_prompt.rs:528` (the math note); `qtap_export/schema-key-order.json`; the vendored export schema. **The message is false:** "nothing new is stored" — `properties.json` gains both keys on every write. | UNPROCESSED |
| `5e18ede6a` | 2026-10-25 | Merge origin/main into main | **NO-PORT? — the merge of `6588972d5` with `cb94e6e99` + `bdd09b75b`; NO code resolved, version stamps only — but it silently DROPPED the four diagram dependencies** (classified 2026-10-26 from `git show --cc`; conflicts only in README / CHANGELOG / both `package.json`s / the lockfile; `dev.86`; parents `6588972d5` `bdd09b75b`). Each side's files came cleanly; the diagram note lands in block 1, which `bdd09b75b`'s budget measures. **v5 intersection:** none. | UNPROCESSED |
| `a0905454b` | 2026-10-25 | Fix: restore diagram dependencies dropped by merge | **NO-PORT? — restores exactly the four client deps the merge dropped** (classified 2026-10-26; 5 files +1,256 / −88; `git diff 6588972d5 a0905454b -- package.json package-lock.json` is the version line only; `dev.87`; parent `5e18ede6a`). **v5 intersection:** none beyond `6588972d5`'s dependency ruling. | UNPROCESSED |
| `9faed0c7e` | 2026-10-26 | Feature: diagram lightbox and Copy PNG | **PORT-NEW — a CLIENT-only rider on the UNPORTED diagrams feature (`6588972d5`); NO server, NO schema, NO dependency move** (classified 2026-10-27 from the hunks; 25 files +2,005 / −11; prod 14 files +1,487 / −6; `dev.88`; parent `a0905454b`). The lightbox (`DiagramLightbox.tsx` — wheel / pinch zoom about the pointer, pan, Fit, 100%, Escape returns to the scroll anchor), Copy PNG at 2× with a Transparent toggle (`export-png.ts`, re-lettering `foreignObject` labels), the drawing becomes a `role="button"` (Enter / Space / click; a text selection is not a click), an Enlarge toolbar button; `lib/clipboard-utils.ts` gains client `copyPngBlobToClipboard` (a promised `ClipboardItem` for Safari, Electron fallback); `_content.css` / `_variables.css` + the theme-storybook mirror; `help/diagrams.md` +15 ("The Viewing Room"). **v5 intersection:** none — v5 has no diagrams; fold into the diagrams SPA lane (the `6588972d5` row's dependency ruling gates it). | UNPROCESSED |
| `3c9ae1fc3` | 2026-10-26 | Docs: 4.11.0 release notes corrections, part 2 | **NO-PORT? — docs only, VERIFIED** (classified 2026-10-27; 1 file `docs/releases/4.11.0.md` +84 / −11; no stamp; parent `9faed0c7e`). **v5 intersection:** `docs/v4/releases/` is vendored — rides the next docs re-vendor with `4a25f7eab` / `505a524fa`. | UNPROCESSED |
| `2041bb875` | 2026-10-26 | Fix: Automation settings checkbox uses qt-checkbox (#111) | **PORT — ONE SPA class change; NO server** (classified 2026-10-27 from the hunks; 2 files +5 / −1; prod 1 line; no stamp; parent `3c9ae1fc3`). `AutomationSettings.tsx`'s checkbox `mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary` → `qt-checkbox mt-1`. **v5 intersection:** `apps/web/src/app/screens/settings/chat/automation-settings.ts` (P4.D307) still carries the OLD Tailwind classes — v5 HAS the visual defect; an SPA small. | UNPROCESSED |
| `8bf856050` | 2026-10-26 | Chore: prune diagram trace logging (#112) | **NO-PORT? — four client `console.debug('[diagrams] …')` lines removed from the UNPORTED diagram components** (classified 2026-10-27; 4 files +4 / −4; prod 3 files −4; no stamp; parent `2041bb875`). **v5 intersection:** none; the diagrams lane ports the post-prune shape. | UNPROCESSED |
| `8becfc6ce` | 2026-10-26 | Chore: dead code sweep for v4.11 release (#113) | **NO-PORT? — four unreferenced exports deleted; no behaviour move** (classified 2026-10-27 from the hunks; 6 files +38 / −43; prod 4 files −37; no stamp; parent `8bf856050`): the `components/inventory/index.ts` barrel, `resolveMemoryMaintenance` (`lib/memory/settings.ts`), `isComputedTool` (`lib/tools/effects.ts` — `b15fe9404`'s UNPORTED module), the `ProjectFileInfo` interface (`lib/tools/project-info-tool.ts`); `DEAD-CODE-REPORT.md` updated. **IMPORT-DEAD census (2026-10-27): `harness/oracle` imports none of the four.** **v5 intersection:** none. | UNPROCESSED |
| `7d7e38d2f` | 2026-10-26 | Test: regression tests for toast stacking and storyteller keepOrder (#114) | **NO-PORT? — tests only, VERIFIED** (classified 2026-10-27; 3 files +144; `__tests__/unit/lib/toast-stacking.test.ts`, `lib/storyteller/__tests__/plan-run-order.test.ts`; no stamp; parent `8becfc6ce`). **v5 intersection:** the toast pin covers `5a8338b30` — ⚠ a surface the `322a5488e` round just ported (P4.D338's toast z-index); the storyteller one is spec input for `b15fe9404`'s UNPORTED planner. | UNPROCESSED |
| `66f8c4d41` | 2026-10-26 | Bundle plugin-types in default-system-prompts and openai-compatible plugins (#115) | **NO-PORT? — v4 JS-plugin bundling only** (classified 2026-10-27; 9 files +16 / −17; prod 8 files; no stamp; parent `7d7e38d2f`): `@quilltap/plugin-types` no longer marked esbuild-external in two `plugins/dist` bundles (a bare `require` that only resolved through a host `node_modules`); manifests default-system-prompts 1.1.26 → 1.1.27, openai-compatible 1.0.49 → 1.0.50. **v5 intersection:** none — v5's providers are declarative manifests (no JS plugins; neither version string appears in v5). | UNPROCESSED |
| `40cc6ffda` | 2026-10-26 | Fix: quilltap db --count printed ANSI color codes under FORCE_COLOR | **PORT (verify-only) — `packages/quilltap/bin/quilltap.js`'s `--count` prints `String(row.count)`, so `console.log` no longer colours the number under `FORCE_COLOR`** (classified 2026-10-27; 6 files +10 / −6; prod 2 lines; `dev.89`; parent `66f8c4d41`). **v5 intersection:** the CLI Tier R `db --count` (P4.3; `crates/quilltap-cli/src/db_cmd.rs`) writes no ANSI — expected to match v4's post-fix bytes; Tier R runs without `FORCE_COLOR`, so no row should move (confirm at the catch-up). | UNPROCESSED |
| `539bb4012` | 2026-10-26 | Docs: CLI README lists memories keystone among server-backed verbs (#116) | **NO-PORT? — docs only, VERIFIED** (classified 2026-10-27; 2 files +6 / −2; `packages/quilltap/README.md` two lines; no stamp; parent `40cc6ffda`). **v5 intersection:** none — the `keystone` CLI verb is NO-PORT by R-AL (`8cb584eff`'s row). | UNPROCESSED |
| `cc5f2347c` | 2026-10-26 | Fix: backups now carry the Log (log_entries) (#117) | **PORT — ⚠ LANDS ON A SURFACE THE `322a5488e` ROUND JUST PORTED: it REVERSES the pin's rule that the Log is NOT carried by a logical backup; NO D23 move; `.qtap` still omits the Log** (classified 2026-10-27 from the hunks; 15 files +350 / −2; prod 8 files +102; no stamp; parent `539bb4012`; merged by `744596158`). Backup: `data/log-entries.json` via a NEW `LogEntriesRepository.findAllEntries()` (`SELECT … ORDER BY "createdAt" ASC`, `[]` while degraded, DEBUG `All log entries read`), DEBUG `Collected Log entries for backup {rowCount}`, manifest `counts.logEntries`. Restore: `parseBackupZip` reads it optional; step **22n-quater** after the chats re-creates each line under its own `id` + `createdAt` + `updatedAt`; while the logs DB is degraded WARN `Skipping Log restore — logs database is in degraded mode {rowCount}` + warning `Log entries were not restored because the logs database is in degraded mode`; per-row warning `Failed to restore Log entry: <msg>` + WARN; DEBUG `Restored Log entries {total, restored}`; `RestoreSummary.logEntries`; preview count. New-account: `remapFields(entry, ['id','chatId','characterId'])`. Replace / delete-all: `clearFormat3Entities` calls a NEW `deleteAll()` (INFO `Deleted all log entries {deletedCount}`). Help: `system-backup-restore.md` (+ the Log under *Logs & History*; + open Storyteller rounds under *Not Included*), `the-log.md` one sentence. Tests: a 4.11 `restore-field-fidelity` block naming every 4.11 data-model addition — a ready checklist for the catch-up's carriers lane. **v5 intersection:** P4.D327 (its item 25 + "assert NO `log_entries` file in any archive") and P4.D331 (the Log's repository + the `DDL.md` note) port the PIN's rule — correct at `322a5488e`, v4 drift once the baseline passes this commit: those assertions flip BY DESIGN. Also moves: `services/backup/collect.rs`, the restore driver + remapper + delete-all path, the restore census whitelist (`RESTORE_TS_MESSAGES`, TS + Rust — the new lines), the backup / restore oracle families and `restore_vintage_state`'s archives. | UNPROCESSED — ⚠ ported over the `322a5488e` round's LANDED Log carriers (P4.D327 / P4.D331 on main 2026-10-27): flips their "no `log_entries` file" pins by design |
| `026222176` | 2026-10-26 | Security: pdfjs-dist 6.4, in-house PDF text reader; fix PDF preview race (bug 266) | **PORT (one WARN text + the converter's spec) + NO-PORT? (dependencies, bundles, the SPA canvas race); NO D23 move** (classified 2026-10-27 from the hunks; 25 files +36,229 / −29,196; prod 13 files +35,646 / −28,773 — nearly all `public/pdf.worker.mjs` and eight re-built `plugins/dist/*/index.js`; `dev.90`; parent `40cc6ffda`, a parallel branch to `539bb4012` → `cc5f2347c`). `pdf-parse` REMOVED; `convertPdfBufferToText` reads with `pdfjs-dist/legacy/build/pdf.mjs` 6.4.299 directly — pdf-parse 2.x's line (4.6 pt) / cell (7 pt → `\t`) assembly, `\n\n-- N of T --\n\n` page markers, `enableXfa: false`, `page.cleanup()`, `loadingTask.destroy()` always, a NEW DEBUG `Extracted PDF text {pages, chars}`; the failure WARN unchanged. `file-content-extractor.ts`'s WARN `pdf-parse found no text, using native fallback extraction` → **`PDF reader found no text, using native fallback extraction`**. Bug 266 (v4-ORIGINATED, found live on `V4test`; v5 Owed): `FilePreviewPdf.tsx` cancels + awaits an in-flight render before re-rendering. Lockfile: `pdfjs-dist` 6.4.299, `@napi-rs/canvas` 1.0.13, **`openai` 7.34.0 → 7.45.0**, `@openrouter/sdk` 1.5.9, `next` 16.4.3; `zod` unchanged 4.6.7. **v5 intersection:** the WARN text is pinned at `crates/quilltap-core/src/generators/file_content.rs:341` / `:499` (P4.D253, bug 177), `ai_import.rs`, `common_helpers_selftest.rs`, `ai_import_tier3_equivalence.rs`, and the `character-wizard-tier3` / `ai-import-tier3` oracles — a byte move. v5's production `DocumentTextExtractor` is `RefusingTextExtractor` (P4.D253), so the reader's assembly is spec for a future host converter, not a behaviour move today. ⚠ **IMPORT-DEAD:** `harness/oracle/cases/file-content-extractor.test.ts:149` `require.resolve('pdf-parse')`. `provider_sdk_version_guard` (P4.D311, openai 7.34.0) reddens at any pin past this. The v5 SPA has no `pdfjs-dist` (no canvas preview) — bug 266's SPA half is a NO-PORT? candidate pending a look at the Files preview's PDF arm. | UNPROCESSED |
| `744596158` | 2026-10-26 | Merge remote-tracking branch 'origin/main' | **NO-PORT? — the merge of `026222176` with `cc5f2347c` (+ `539bb4012`); conflict ONLY in `docs/CHANGELOG.md`, no code resolved** (classified 2026-10-27 from `git show --cc`; `dev.90`; parents `026222176` `cc5f2347c`). **v5 intersection:** none beyond its two parents' rows. | UNPROCESSED |
| `c50cdddc6` | 2026-10-26 | Document terminal sessions as excluded from backups (#118) | **NO-PORT? (code) + help text — no production code** (classified 2026-10-27; 6 files +238 / −1; no stamp; parent `744596158`): `help/ariel.md` (one paragraph extended — sessions and transcripts stay out of a backup), `help/system-backup-restore.md` (+ an *Ariel's terminal sessions* Not-Included bullet), a release-checklist line, a NEW test `terminal-sessions-excluded.test.ts`, and a mock fix (`logEntries` in `delete-service-annotations.test.ts`). **v5 intersection:** ⚠ `help/ariel.md` is a page the `322a5488e` round just vendored (P4.D339, with P4.D337's `:27-30`) — re-vendor at the catch-up; confirm v5's backup also leaves `terminal_sessions` out (expected — the pin's rule). | UNPROCESSED |
| `85c002822` | 2026-10-26 | Fix terminal Kill ignored by bash; deleted chats leave shells running (#119) | **PORT — bugs 267 + 268 (both v4-ORIGINATED — 267 "by code reading", 268 "while fixing 267"; v5 Owed), NOT a convergence; ⚠ LANDS ON A SURFACE THE `322a5488e` ROUND JUST PORTED; NO D23 move** (classified 2026-10-27 from the hunks; 17 files +571 / −25; prod 8 files +291 / −19; `dev.91`; parent `c50cdddc6`). **Bug 268:** `ptyManager.terminate(id, graceMs = 3000)` — `kill(id,'SIGHUP')`, then SIGKILL if `meta.exitedAt` is still null (WARN `[PTY] Session ignored SIGHUP; sending SIGKILL {sessionId, graceMs}`); BOTH `/api/v1/terminals/[id]` kill and DELETE use it (were `kill(id,'SIGTERM')`); async `kickAllForChat` (which flushed Ariel first) REPLACED by sync `disposeChatSessions(chatId)` → terminate + drop each, INFO `[PTY] Terminated sessions of deleted chat {chatId, count}` / DEBUG `[PTY] No live sessions for deleted chat`; called from `ChatsRepository.delete` after the row is gone (WARN `Failed to terminate terminal sessions for deleted chat`). **Bug 267:** `TerminalSessionsRepository.sweepOrphanedTranscripts({isLive, graceMs, now?})` unlinks `<logs>/terminals/*.log` no row names (by id or resolved `transcriptPath`), not live, unwritten for `ORPHAN_TERMINAL_TRANSCRIPT_GRACE_MS` (1 h); INFO `Swept orphaned terminal transcripts {scanned, deleted}`; daily-maintenance step **6b** `orphan-transcripts` (`Orphaned-transcript sweep failed — continuing`) and a NEW `MaintenanceSweepSummary.orphanedTranscriptsSwept {scanned, deleted}` key. `help/ariel.md` rewritten (+2 paragraphs). **v5 intersection:** P4.D337 (bug 255) re-ported `pty-manager.ts` at the pin, reading `kickAllForChat` (`:625-646`), and P4.D339 vendors `help/ariel.md`. v5 HAS both defects: `crates/quilltap-host/src/terminal/mod.rs` `kill` sends `SIGTERM` only and `kick_all_for_chat` has no production caller (P4.1's PTY lane); `crates/quilltap-core/src/services/scheduled_maintenance.rs` has no orphan-transcript sweep — the new summary key moves the scheduled-maintenance differential. | UNPROCESSED — ⚠ ported over the `322a5488e` round's LANDED terminal port (P4.D337 on main 2026-10-27) + the vendored `help/ariel.md` (P4.D339) |
| `4b0d1cb04` | 2026-10-27 | Fix NanoGPT image sizes: fit each request to the model (bug 269) | **PORT — bug 269 (v4-ORIGINATED, found live on Friday from "no avatars arriving"; v4's v5 status `Not assessed`), NOT a convergence; NO D23 move, NO dependency move** (classified 2026-10-27 from the hunks; 11 files +698 / −15 incl. a 237-line test and the bug file; prod: `plugins/dist/qtap-plugin-nanogpt/image-provider.ts` +99 / −5 and a NEW pure `image-sizes.ts` 126 lines, the rebuilt `index.js`, plugin 1.2.11; lockfile hunk is the root version stamp only; parent `85c002822`). NEW `fitImageShape(supported_parameters, size, aspectRatio)` picks the nearest listed concrete size by aspect ratio (in the model's own `x` / `*` spelling) or, for a tier model (`1k` / `2k`), the nearest tier by long edge plus the nearest listed `aspect_ratio` (a NEW catalog field), posted as a NEW flat body key `aspect_ratio`; `generateImage` now reads the model's catalog entry first via NEW `catalogEntryFor` (refreshes a cold cache with the call's key — WARN `Could not refresh the NanoGPT image catalog; posting the size as asked`), DEBUG `Fitted the requested image shape to the model {model, requestedSize, requestedAspectRatio, size, aspectRatio}`; the `Posting NanoGPT image request` DEBUG now logs the POSTED `size` + `aspectRatio` (was `params.size`), and so does the `NanoGPT image request failed` ERROR; a non-square request refused on its shape (`503`, or `400` whose message matches `size|resolution|aspect|dimension|output`) retries ONCE as the fitted square (`1024x1024` with no catalog entry) with WARN `NanoGPT turned down a non-square image; retrying once as a square`. Help: `image-generation-profiles.md` +1 paragraph. **v5 intersection:** P4.D101 (the NanoGPT bundled provider) + the LoRA train — `crates/quilltap-core/src/model/image_dialects.rs` (`RealImageProvider::generate_image`, the `Posting NanoGPT image request` DEBUG at `:2086` logs `params.size` — a field move, pinned by `crates/quilltap-harness/tests/nanogpt_lora_wire_log.rs`), `model/nanogpt_catalog.rs` (the detailed-catalog cache — v5 has no forked job child, so the "cold in the child" premise differs: measure whether v5's cache is warm in the job path), `image_gen/params_builder.rs`. v5 HAS bug 269 (forwards `size` as given). `image-sizes.ts` is a tier-1 pure-function candidate. | UNPROCESSED |
| `2b2b190b6` | 2026-10-27 | Composer prose mode: narration/OOC preview and a no-dialogue warning | **PORT-NEW (SPA / client only) — rides the UNPORTED punctuation lint (`1dd0a0c01`) and the UNPORTED Novel template (`778b97b2b`); NO server, NO wire, NO schema, NO setting** (classified 2026-10-27 from the hunks; 21 files +787 / −51 incl. ~410 lines of tests + a 281-line `prose-vectors.json` contract corpus; `dev.94`; parent `4b0d1cb04`). `lib/punctuation-lint/engine.ts`: a `proseMode` option, a NEW rule `no-dialogue` (message `No dialogue yet — not a single opening quote in the whole draft. Meant to say this aloud? Put the speech in double quotes.`), `lintQuotes` returns the outermost dialogue runs, NEW `oocSpans` / `classifyParagraph` / `classifyProse` (narration vs `((ooc))` runs, quotes masked inside asides, scene breaks excluded). NEW `lib/punctuation-lint/prose-mode.ts` `isProseTemplate(template)` (unmarked narration + double-quoted dialogue). `PunctuationLintPlugin` publishes four prioritized CSS Custom Highlight names; `SalonView` → `ChatComposer` → `LexicalComposerWrapper` thread a `proseMode` prop; NEW `::highlight` rules + tokens in `_chat.css` / `_variables.css`. Help: `chat-settings-composer.md`, `roleplay-templates.md`. The preview ignores the `punctuationWarnings` toggle; the no-dialogue warning honours it. **v5 intersection:** the v5 SPA composer (`apps/web`, the Salon composer the `322a5488e` round's interrupt lanes just touched for raise-hand / stop) — a new input BESIDE, not on, that work; v5 has no punctuation lint and no Novel template yet, so this ports WITH `1dd0a0c01` + `778b97b2b` (one lane), and `prose-vectors.json` is a ready contract corpus for it. | UNPROCESSED |
| `cce44e098` | 2026-10-27 | Fix: legibility pass over the qt-* surfaces added since 4.9 | **PORT (SPA CSS + bundled theme packs) + riders on UNPORTED surfaces; NO server, NO behaviour** (classified 2026-10-27 from the hunks; 27 files +165 / −51; `dev.95`; parent `2b2b190b6`). Core `qt-components`: `_chat.css` — `.qt-chat-regenerating-plate-text` takes the bubble's ink (`--qt-chat-regenerating-plate-fg` → `--qt-chat-assistant-fg`), `.qt-danger-badge` / `-muted` and `.qt-concierge-mark` read `--qt-alert-error-fg` (the Concierge badge); `_variables.css` light `--qt-response-status-streaming-fg` 38% → 29% lightness; `_intervention.css` (note ink → `--qt-alert-warning-fg`; the hidden-field hand struck through instead of `opacity: 0.5`) and `_storyteller.css` — both UNPORTED features (`839575d89`, `b15fe9404`). Themes: art-deco, earl-grey, great-estate (`tokens.json` too), madmans-box (diagram pane — UNPORTED `6588972d5`), old-school, rains (`tokens.json` too) — `styles.css` + `theme.json` version bumps. `theme-storybook` 1.0.87 mirror (NO-PORT — v5 has none). Help: one `pascals-workbench.md` sentence (the struck-through hand — rides the UNPORTED intervention). **v5 intersection:** `apps/web/src/styles/qt-components/_chat.css` (`:164` the plate, `:3101` / `:3110` / `:3120` the badge + mark), `_variables.css:389`, and `apps/web/public/themes/*` (vendored at the pin's versions, P4.5 onward); the vendored `help/pascals-workbench.md` (P4.D339) gets the sentence only with intervention. | UNPROCESSED |
| `dd200c695` | 2026-10-27 | Docs: move bug filing to GitHub Issues; freeze catalogue at Bug 269 | **NO-PORT? (docs / infra) — BUT a PROCESS change for this port** (classified 2026-10-27 from the hunks; 5 files +140 / −38: NEW `.github/ISSUE_TEMPLATE/bug.yml` (Severity, Who it bites, Provenance Original/Pinned/Faithful/Inert, How found, v5 status, Symptom, Root cause, Why it survived, Fix site, How to verify), v4 `CLAUDE.md` "Filing bugs", `docs/developer/bugs.md` FROZEN header + "How these are filed" rewrite, `.claude/commands/update-documentation.md`, CHANGELOG; no stamp; parent `cce44e098`). v4's catalogue takes no entries past Bug 269; new bugs are GitHub issues cited `#N` and closed by `Fixes #N`. **v5 intersection:** no code. The CONVERGENCE check (§4 step 5) must now also read `gh issue list -R foundry-9/quilltap-server --label bug --state all`; a v4 bug this port files (as the dogfood passes have — bugs 57 … 185) is now a GitHub issue via the Bug form, outward-facing, so confirm with the human first; v5 code comments citing a future v4 bug use `#N`. `docs/v4/developer/bugs.md` mirror re-vendors at the catch-up. | UNPROCESSED |
| `86cc25482` | 2026-10-27 | Fix: legible dialogue in user messages; Rains user text; Great Estate sidebar | **PORT (SPA CSS + two bundled theme packs); NO server, NO behaviour** (classified 2026-10-27 from the hunks; 12 files +113 / −9; `dev.97`; parent `dd200c695`). `_chat.css` NEW rule `.qt-chat-message-user .qt-chat-message-content .qt-chat-dialogue { --qt-rp-fg: var(--qt-rp-user-dialogue-color, var(--qt-chat-user-fg)) }` (a NEW theme hook `--qt-rp-user-dialogue-color`); `themes/bundled/rains/styles.css` `--qt-chat-user-fg` `hsl(40 6% 98%)` → `hsl(var(--text-000))`; `great-estate/styles.css` +31 (light-mode left-sidebar icons / labels in cream); both `theme.json` version bumps; `theme-storybook` 1.0.88 (+55, the dialogue / narration / monologue / OOC chip family — NO-PORT). **v5 intersection:** `apps/web/src/styles/qt-components/_chat.css` (`.qt-chat-dialogue` at `:1948`) and `apps/web/public/themes/{rains,great-estate}` — one SPA small with `cce44e098`. | UNPROCESSED |

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
   **Since `dd200c695` (2026-10-27) that catalogue is FROZEN at Bug 269** —
   later v4 bugs are GitHub issues cited `#N` (`Fixes #N` in the fixing
   commit); check them too with `gh issue list -R
   foundry-9/quilltap-server --label bug --state all`.
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

- **The `322a5488e` Inventory + Log + interrupts + custom-tool-grammar +
  bug-255 drift catch-up round (2026-10-27, baseline `e8e17b2d9` →
  `322a5488e`; seventeen lanes, one stacking point `INVENTORY`):** the
  twenty-two rows `e8e17b2d9..322a5488e` retired. Round record:
  `status-log.md` → "The `322a5488e` … round — UNIFICATION record
  (2026-10-27)".
  - **`NO-PORT-RATIFIED(P4.D339)`:**
    - `cad017599`, the Measure Twice test (its seven cases mirrored by
      P4.D329 over the widened `pascal-run-custom` pair).
    - `cc57e6cbc`, the broader-gates spec (docs only).
  - **ABSORBED:**
    - `4fbdad186` (P4.D324, P4.D325, P4.D326, P4.D327, P4.D330, P4.D339; the
      archive keep was P4.D322's), the Inventory — the THIRTEENTH D23 move.
    - `e1872c94f` (P4.D324 [the flag column], P4.D328, P4.D329, P4.D330,
      P4.D334, P4.D339; `child-repositories-proxy.ts` NO-PORT), the Inventory
      tools + Pascal inventory support — the FOURTEENTH move.
    - `102a23648` (P4.D324 [bug 253], P4.D328, P4.D330; bug 254 was
      P4.D322's), the Inventory dogfood fixes + the roster shelf gate.
    - `1d0a31132` (P4.D338), scenario lists by title.
    - `a6292f169` (P4.D337, P4.D339), bug 255 — the FIFTEENTH move, `vt100`
      0.18.1 (approved 2026-10-21).
    - `7263105c6` (P4.D340 [the lint], P4.D338 [bug 256], P4.D330), the
      Inventory title lint + published vault addresses.
    - `406dbdd5a` (P4.D326, P4.D339), the bug-133 re-ruling.
    - `ea2cb60ab` (P4.D332, P4.D339), `edit_custom_tool`.
    - `680b71044` (P4.D338 [server], P4.D330 [client]), `/photos` without
      vaults + the album selector.
    - `cfa0cba29` (P4.D332), vault custom tools as the character's only.
    - `e4a60ad9e` (P4.D324 [the re-dump's `llmLogs` block], P4.D331, P4.D334
      [the effect UI], P4.D333 [the schema], P4.D339), the Log — the
      SIXTEENTH move, `regress` (approved 2026-10-21).
    - `a495faa26` (P4.D331), the Log's date bounds + regex errors.
    - `e27fb6b34` (P4.D330, P4.D339; theme-storybook NO-PORT), the Inventory
      icon.
    - `5a8338b30` (P4.D338), the toast z-index.
    - `2b5e8ee24` (P4.D335, P4.D336, P4.D339; theme-storybook NO-PORT), raise
      a hand / stop.
    - `c56fe278c` (P4.D332), `edit_custom_tool`'s `schema` action.
    - `a53024d3c` (P4.D333, P4.D329 [the aggregate], P4.D334, P4.D339), OR /
      any-of + `inventoryAggregate`.
    - `b8362beb5` (P4.D336), the stacked composer buttons.
    - `d88c1ce39` (P4.D330, P4.D339), the blank-description hint.
    - `322a5488e` (P4.D333, P4.D334, P4.D339), bug 257 — the table dealt
      before the consult.
  - P4.D339 also joins every row whose `help/` or `docs/` hunk it vendored
    (its order's HANDOFF table is the measured list).
- **The `e8e17b2d9` outfit-history + Workbench-picker + gallery + bug-252
  drift catch-up round (2026-10-19, baseline `25b8fab6a` → `e8e17b2d9`;
  nine lanes, one stacking point):** the fourteen rows through `e8e17b2d9`
  retired. Round record: `status-log.md` → "The `e8e17b2d9` … round —
  UNIFICATION record (2026-10-19)".
  - **`NO-PORT-RATIFIED(P4.D323)`:**
    - `fcab3f112`, the outfit-history spec (vendored at its final
      `features/complete/wardrobe-outfit-history.md` path).
    - `54bc17a3c` / `f5db10034`, the two Inventory specs — docs only; their
      vendoring DEFERRED by name to round (2) at their final `complete/`
      paths (R-Q; `docs/v4` is 494 tracked with those two deferrals).
  - **ABSORBED:**
    - `5cb0e0190` (P4.D319, P4.D320, P4.D323), the Workbench wardrobe picker.
    - `a05c6f755` (P4.D319, P4.D320, P4.D323), `{{character}}` +
      `announceAsCharacter`.
    - `83387d0e5` (P4.D315, P4.D323; the child-proxy `wardrobeHistory.settle`
      override NO-PORT), outfit-history storage — the TWELFTH D23 move,
      BOOT-CREATED through three ported ensures (the human's 2026-10-19
      standing ruling), `fresh_schema.json` UNMOVED.
    - `05005c562` (P4.D316; the `turn-containers.ts` module split for jest
      mocks NO-PORT), recording at the announcement.
    - `ebb480edd` (P4.D316, P4.D318), `wardrobe_rollback` /
      `wardrobe_history` (57 → 59 tools).
    - `8d8618819` (P4.D315 [GET], P4.D316 [POST], P4.D318 [tab], P4.D323),
      the history API + the History tab.
    - `7953da3a0` (P4.D317), the backup / restore / `.qtap` carriers.
    - `f46cf40a2` (P4.D315, P4.D323; v4's own `CLAUDE.md` +1 and the
      `BACKGROUND_JOBS_CHILD.md` settle-override prose NO-PORT), the
      model-visible summary fix + docs.
    - `4ae20977f` (P4.D315, P4.D316, P4.D317, P4.D318, P4.D323),
      `actorToolName` — the twelfth move's third migration, carried in the
      table's FINAL shape.
    - `1cfc9b504` (P4.D321, P4.D323; `packages/theme-storybook/**` NO-PORT),
      the Aurora gallery's Wardrobe Pictures.
    - `e8e17b2d9` (P4.D317, P4.D323), bug 252 — restore / import re-key the
      four character-keyed chat columns (v5 HAD the defect in both carriers).
- **The `25b8fab6a` Pascal-programme + composer-slash-commands + 4.10.0-
  release drift catch-up round (2026-10-17, baseline `83334b9a5` →
  `25b8fab6a`; eleven lanes):** the twenty-six rows retired.
  - **`NO-PORT-RATIFIED(P4.D313)`:**
    - `7417e9a1b`, the dead-code sweep (its oracle obligation repaired by
      P4.D314's `REAIM`).
    - `052659624` / `809e5104d`, the bug-247 filing.
    - `3f64d3a6c`, the README rewrite.
    - `2432b9057`, the 4.10.0 release squash.
    - `f573a7576` / `37246654d` / `1e1712a12`, the merge, the 4.11 start and
      the CHANGELOG move.
    - `936a34720` / `b93c2efda`, specs.
  - **ABSORBED:**
    - `01f508d4e` (P4.D310, P4.D309), the debug-line prune.
    - `a42c282b8` (P4.D308).
    - `a8b2d2861` (P4.D311), openai-compatible `normalizeUsage`.
    - `49f8906c3` (P4.D312), the new-account seat-id remap.
    - `75d094e63` (P4.D312, P4.D313; the `migrations status` half NO-PORT).
    - `8489d08b9` (P4.D308, P4.D313; the comment renames NO-PORT), the About
      blurbs + docs moves.
    - `bbbcd2388` (P4.D309, P4.D313), copy-to-folder.
    - `b82037d75` (P4.D310), bug 248.
    - `7458ff94e` (P4.D312, P4.D313), the hair note.
    - `362426c8f` (P4.D304, P4.D305, P4.D307, P4.D313; the prettify label
      NO-PORT), trigger phrases, the TENTH D23 move.
    - `f0db8cbb8` (P4.D306, P4.D307, P4.D313), wardrobe effects + the
      composite take-off.
    - `6069a7acd` (P4.D310), bug 249.
    - `4877fc2da` (P4.D304, P4.D308, P4.D307, P4.D313; the prettify label, the
      dialog MOVE and `requestRngDirect` NO-PORT), slash commands, the
      ELEVENTH D23 move.
    - `eb5d42338` (P4.D311; the non-v5 halves NO-PORT), openai 7.34.0 / zod
      4.6.7.
    - `b2793400f` (P4.D308, P4.D313; Lexical internals + the caret half
      NO-PORT), bug 251's trim.
  - **`25b8fab6a` CONVERGENCE (P4.D312):** bug 247, this port's filing;
    `PROTOTYPE_KEY_MONTH_THROWS` retired by measurement. The bug-250 half is
    NO-PORT (P4.D308, P4.D313).
  - **`5cb0e0190` stays in §3**, now classified from the hunks, with the
    eight commits after it.
  - The round record is in `status-log.md` → "The `25b8fab6a` … round —
    UNIFICATION record (2026-10-17)".

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
