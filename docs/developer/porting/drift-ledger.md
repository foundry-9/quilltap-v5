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

- **Oracle baseline: `25b8fab6a`**: "fix: month-word prototype keys (bug
  247); orphaned typeahead menu (bug 250)" (v4 main, 2026-10-16 07:07,
  `4.11.0-dev.10`). It was adopted when the `25b8fab6a` Pascal-programme +
  composer-slash-commands + 4.10.0-release drift catch-up round was unified
  (2026-10-17). All eleven lanes landed: P4.D314 → {P4.D304 → {P4.D305 →
  P4.D306}} ∥ P4.D309 ∥ P4.D310 ∥ P4.D311 ∥ P4.D312, with P4.D307 ∥ P4.D308 ∥
  P4.D313. CLAUDE.md's Status bullet for the round and its "Oracle baseline"
  bullet both name `25b8fab6a`, and they AGREE with this file (checked
  2026-10-17).
- **Checked:** 2026-10-18 (evening), by a standalone `/driftcheck`. Both
  branches re-probed locally AND against the remote (`git ls-remote origin`:
  `main` = `102a23648`, `bugfix` = `489b228b6`, `release` = `2432b9057` —
  local equals remote). The SIX new `main` commits were classified from the
  hunks (three parallel readers for the four code commits, the two docs
  commits by the checker; the readers' load-bearing claims spot-verified
  against v5: `build_context.rs`'s continue-mode progressions skip, the
  `progressions_continue_mode` tier-3 op, `ChatUpdate` lacking
  `pending_outfit_notifications`, `e1872c94f`'s migration registration, and
  `character_archive`'s keep-set). The eleven earlier §3 rows stand
  unchanged. CLAUDE.md's "Oracle baseline" bullet still names `25b8fab6a` —
  AGREES.
  - **Before that:** 2026-10-18 morning (`1cfc9b504`, two rows); 2026-10-17
    twice (the `25b8fab6a` unification's close probe, then a no-movement
    check at `da92651fa`); 2026-10-16 twice; 2026-10-15 (`/unify` of the
    `83334b9a5` round); and the earlier checks the §6 bullets record.
- **v4 `main` HEAD at check: `102a23648`** ("Fix: Inventory dogfood fixes,
  bugs 253/254, and the project roster gate", 2026-10-18 22:47,
  `4.11.0-dev.36`).
  - **SEVENTEEN commits past the baseline:**
    - `5cb0e0190`, the Workbench wardrobe picker (10-16 07:47).
    - `a05c6f755`, the `{{character}}` placeholder + character-announced
      custom tools (10-16 14:45).
    - The outfit-history programme, 10-16 21:57 → 22:44: `fcab3f112` the
      spec, `83387d0e5` phase 1 storage **WITH THE TWELFTH D23 MOVE**,
      `05005c562` phase 2 recording, `ebb480edd` phase 3 `wardrobe_rollback`
      / `wardrobe_history`, `8d8618819` phase 4 the API + History tab,
      `7953da3a0` phase 5 the backup / restore / `.qtap` carriers, and
      `f46cf40a2` phase 6 docs + a model-visible summary fix.
    - `4ae20977f`, outfit history records who did the dressing —
      `chat_wardrobe_history."actorToolName"` + a THIRD migration, widening
      the still-pending TWELFTH move (10-17 09:02).
    - `1cfc9b504`, the Aurora gallery's Wardrobe Pictures section + `GET
      /api/v1/characters/[id]/wardrobe-images` (10-17 15:51).
    - **NEW 2026-10-18 evening — the Inventory programme + bug 252,** all
      on 10-18 (v4 now lands squash-merged PRs #99–#102):
      - `54bc17a3c` the Inventory spec, `f5db10034` the inventory-tools +
        Pascal spec (docs only).
      - `e8e17b2d9` **bug 252** — restore / import re-key the four
        character-keyed chat columns (v5 HAS the defect).
      - `4fbdad186` **the Inventory** — objects as vault/store files,
        `chats.equippedInventory` (**the THIRTEENTH D23 move**), routes,
        equip, the context section, image phrase, carriers.
      - `e1872c94f` **nine `inventory_*` tools + Pascal inventory support**,
        `characters.canManageInventory` (**the FOURTEENTH D23 move**).
      - `102a23648` Inventory dogfood fixes + bug 253 + **bug 254** (v5 HAS
        it — progressions skipped in continue mode) + the roster shelf gate.
  - **Stamps:** dev.11 → dev.36 (`fcab3f112`, `54bc17a3c`, `f5db10034` stamp
    nothing; `83387d0e5` skips dev.13; the squashes skip dev.22–25 and
    dev.27–32).
- **v4 `bugfix` tip at check:** `489b228b6` ("bugfix: started 4.10.0 bug
  branch", 2026-10-14). It has not moved: `489b228b6..bugfix` is EMPTY, and by
  content `git diff 2432b9057 bugfix -- lib app packages plugins` is EMPTY.
  **`release` tip:** `2432b9057` (`release: 4.10.0`), unchanged. Re-probe BOTH
  branches every time.
- **Checkout at check:** branch **`main`**, tree **CLEAN** at `102a23648`.
  HEAD is seventeen commits past the baseline, so it poisons a regen by
  DRIFT.
- **Verdict: DRIFT PENDING — 17 commits (§3), all UNPROCESSED.**
  - **Classes:**
    - 2 PORT-NEW + PORT (`5cb0e0190`, `a05c6f755`).
    - 1 PORT-NEW **WITH THE TWELFTH D23 MOVE** (`83387d0e5`), widened by
      1 PORT **inside the same move** (`4ae20977f` — a column + a migration).
    - 1 PORT-NEW + PORT **WITH THE THIRTEENTH D23 MOVE** (`4fbdad186`, the
      Inventory — six hunks on ported surfaces).
    - 1 PORT-NEW + PORT **WITH THE FOURTEENTH D23 MOVE** (`e1872c94f`, nine
      tools + Pascal inventory — moves every `run_custom` description).
    - 3 PORT-NEW (`ebb480edd`, `8d8618819`, `1cfc9b504`).
    - 5 PORT (`05005c562`, `7953da3a0`, `f46cf40a2`, `e8e17b2d9` bug 252,
      `102a23648` bug 254 + Inventory fixes).
    - 3 NO-PORT? (`fcab3f112`, `54bc17a3c`, `f5db10034` — specs).
  - **No CONVERGENCE:** bugs 252 / 253 / 254 carry `bugs.md` hunks, but all
    three were found by v4 itself (v5 status **Owed** in v4's rows; none
    appears in v5's `status-log.md`, `dogfood-findings.md` or `CLAUDE.md`).
    v5 shares 252 and 254 today, so their rows are PORTs, not convergences.
  - **Production surface:** 88 files, +2,859 / −119 through `f46cf40a2`;
    `4ae20977f` + `1cfc9b504` add 56 files, +1,059 / −60; the six new
    commits add 347 files, +22,253 / −1,656 (tests, docs and SPA included —
    `4fbdad186` alone is 151 files, +13,094).
  - **Regen rule: PIN REQUIRED at `25b8fab6a`** (§5.1).

### ⚠ LIVE-DATA — what stands at this baseline

- **The copy-before rule RETIRES.** A v5 at `25b8fab6a` carries both 4.11
  `chat_settings` columns (two UNSTAMPED, table-gated boot ensures), reads
  `pascalMeta.invokedBy: 'trigger'` rows and `triggerPhrase`, and reads the
  `wardrobe.*` effect records. A Friday copy taken at any v4 point up to
  `f46cf40a2` opens.
- **The general rule stands:** a v5 build from BEFORE this round must not
  open an instance this round's ensures have altered (the P4.D288 note
  restated).
- **⚠ NEW 2026-10-18 — ARCHIVING A CHARACTER IN v5 DELETES ITS INVENTORY.**
  Once v4 runs past `4fbdad186` and anyone creates an inventory object, a
  character vault holds `Inventory/**/*.json` (+ `Inventory/images/`). v5's
  archive prune (`services/character_archive/service.rs`) keeps only the ten
  `KEPT_MANAGED_PATHS` + the `Wardrobe/` prefix (verified 2026-10-18), so a
  v5 **archive** of such a character DELETES its objects and their picture
  links — real data loss on an explicit action. **Do not archive characters
  from a v5 build on an instance v4 has given an Inventory** until the
  Inventory catch-up lands the `Inventory/` keep.
- **Past the pin — NO boot-blocking, NO ping-pong, NO NOT-NULL-insert hazard
  (classified 2026-10-17; re-checked through `102a23648` 2026-10-18). The
  `actorToolName`, `chats.equippedInventory` and
  `characters.canManageInventory` columns are all NULLABLE, each v4
  migration is column-gated, and v5's chat / character reads and writes
  name their columns explicitly.** Further non-corrupting notes for
  `4fbdad186` … `102a23648`: a v5 backup / `.qtap` export omits
  `equippedInventory` (a `replace` restore re-inserts chats with it NULL)
  and `canManageInventory` (an explicit `false` reads back as on); a v5
  new-account restore re-mints Inventory object ids in `files.linkedTo` and
  leaves each object's `imageFileId` stale; custom tools using `when` / gate
  `inventory` / `inventoryCount`, a `$inventory` operand or roll, or an
  `inventory` effect are REFUSED by v5's loader and drop off its roster;
  v5 turns / Carina / greetings omit `## Carried and equipped`; v5 still
  skips progressions on chained / nudged / autonomous turns (bug 254) and
  leaves new-account restores and re-keyed `.qtap` imports undressed (bug
  252). The `Inventory/` files themselves survive v5's mount-index backup /
  restore intact. Measure first at port time: that
  `store_name_reconcile.rs` `read_vault_owner_rows`' `SELECT *` tolerates
  the extra `characters` cell. A v5 at this baseline never reads, writes or
  re-runs `chat_wardrobe_history` or its two `migrations_state` rows. Its
  `chats` deletes cascade the timelines as v4's do. Two non-corrupting notes:
  1. **Outfit history does not survive a v5 round trip.** v5's backup omits
     `data/wardrobe-history.json`; its restore ignores it, and a `replace`
     restore's chat delete cascades the table empty. Its `.qtap` export omits
     the records, and its import skips the `wardrobe_history` kind. Outfit
     changes v5 makes go unrecorded; v4's next settle appends from the live
     slots.
  2. **`a05c6f755` definitions.** A tool with `{{character}}` in an effect
     expression is refused at load, so it drops off v5's roster.
     `announceAsCharacter` rows render as Pascal.
- **Standing:** the extraction rate ledger is per-process (a v4 and a v5
  sharing an instance each keep their own hourly count). The `52d6e7ecd` /
  `07b8f0209` notes and the P4.D274 store-name plan probe on a copy also stand.

### Schema / D23

- **LANDED this round (P4.D304): the TENTH + ELEVENTH moves in ONE
  `fresh_schema.json` re-dump from the pin.**
  - The two columns: `chat_settings."customToolTriggers" INTEGER DEFAULT 0`
    at cid 18 and `"composerSlashCommands" INTEGER DEFAULT 1` at cid 23
    (37 → 39 columns).
  - `chat_settings_seed.json` gains both.
  - v4's `add-custom-tool-triggers-field-v1` and
    `add-composer-slash-commands-field-v1` are re-homed as UNSTAMPED,
    table-gated FATAL boot ensures in v4's registration order, each compared
    against v4's REAL `MigrationRunner`
    (`chat_settings_4_11_columns_ensure_equivalence`).
  - `restore_vintage_state`'s boot-align list carries the chain.
    `zero_migrations_rerun` is widened, and `migration_indexes.json` is
    unmoved.
  - The two `*-main.db` pairs v4's oracle writes `chat_settings` into are
    widened through v4's real migrations.
- **PENDING — the TWELFTH move (`83387d0e5`, past the pin).**
  - **The table:** a NEW `chat_wardrobe_history` (`id`, `chatId` FK →
    `chats` ON DELETE CASCADE, `characterId` with no FK, `entryIndex`,
    `slots`, `slotsHash`, `source`, `actorCharacterId`, `isCurrent` DEFAULT 0,
    `createdAt`).
  - **Its two indexes:** UNIQUE `idx_chat_wardrobe_history_entry` and the
    partial UNIQUE `idx_chat_wardrobe_history_current` `WHERE "isCurrent" = 1`.
  - **Its two migrations:** `add-chat-wardrobe-history-table-v1` and
    `seed-chat-wardrobe-history-v1`. The seed is an additive data write: one
    `seeded` entry 0 per dressed `(chat, character)`.
  - ⚠ **v4's generateDDL never renders it** (the repository is a plain class).
    `dump-fresh-schema.ts` skips it, `dump-migration-indexes.ts` drops its
    indexes, and `provisioning_equivalence` stays GREEN. **The D23 tripwire
    will not fire; `zero_migrations_rerun_equivalence` is the family that
    should.**
  - **RULED (the human, 2026-10-19): BOOT-CREATED — and as a STANDING rule,
    not just for this table.** v5 creates the table through boot ensures
    ported from v4's three migrations (table → `actorToolName` column →
    seed, in v4's registration order), never from a v4-dumped
    `migration_tables.json`. A fresh instance gets it on its first boot
    (provisioning stamps no `migrations_state` row, so the ensures run). The
    seed ensure STAMPS its `migrations_state` row (its only once-gate — else
    v4 re-seeds); all three join `restore_vintage_state`'s boot-align list;
    `zero_migrations_rerun_equivalence` grows by three ids; each ensure is
    differentially compared against v4's REAL runner while v4 exists. The
    human's reason: "Someday there won't even be a v4 any more" — v5's schema
    must be self-sufficient, not provisioned from artifacts only v4 can
    produce. R-A stays overruled for every table; for a future table v4's
    generateDDL never renders, default to boot ensures without asking.
  - The full DDL is in the `83387d0e5` §3 row.
  - **WIDENED by `4ae20977f` (2026-10-17):** an eleventh column
    `"actorToolName" TEXT` (nullable, LAST in the shared DDL so a fresh table
    and an ALTERed one agree on column order) and a THIRD migration,
    `add-chat-wardrobe-history-tool-name-v1` (`dependsOn` the table
    migration; `shouldRun` = SQLite + table present + column absent;
    `addColumnIfMissing(..., 'TEXT')`). The seed now `dependsOn` BOTH (the
    shared INSERT spells the new column). Registration order: table →
    tool-name → seed. Port the table at its FINAL shape and carry all three
    ensures in that order; the 2026-10-19 boot-created ruling covers the
    column too.
- **PENDING — the THIRTEENTH move (`4fbdad186`): `chats."equippedInventory"
  TEXT`.** generateDDL-RENDERED (`ChatsRepository` is a
  `TaggableBaseRepository`): a fresh table puts it at cid 66 between
  `equippedOutfit` and `characterAvatars` with no DEFAULT; v4's
  `add-chat-equipped-inventory-v1` (`dependsOn add-equipped-outfit-field-v1`,
  column-gated, registered after `seedChatWardrobeHistoryMigration`) APPENDS
  it as `TEXT DEFAULT NULL` — two shapes, carry BOTH (the P4.D249 precedent).
  The D23 tripwire FIRES (`provisioning_equivalence` 1d +
  `zero_migrations_rerun_equivalence`); no ruling needed.
- **PENDING — the FOURTEENTH move (`e1872c94f`):
  `characters."canManageInventory" INTEGER DEFAULT NULL`.** generateDDL-
  rendered between `canCreateOutfits` and `characterDocumentMountPointId`;
  v4's `add-character-inventory-flag-v1` (`dependsOn
  add-character-wardrobe-flags-v1`, column-gated, registered LAST after
  `addChatEquippedInventoryMigration`) appends it. Two shapes again; ONE
  re-dump with the thirteenth.

### Regen, dependencies, help

- **Regen rule: PIN REQUIRED at `25b8fab6a`** (§5.1).
  - The pin's own `npm ci --offline` tree carries `openai` 7.34.0 and `zod`
    4.6.7.
  - The plugin dirs, symlinked from the live checkout, carry
    `@quilltap/plugin-utils` 2.7.1 in fourteen dirs. The lockfile diff
    `25b8fab6a..f46cf40a2` moves only the version stamp.
  - **Past the pin:** `05005c562` re-shapes `OutfitChangeContext` (a REQUIRED
    `provenance`; `4ae20977f` adds an optional `toolName` to it). The wardrobe / Pascal oracle cases that build one will
    need re-aiming at the next pin (an IMPORT-DEAD census at `/setupphase`).
- **Dependencies:** `openai` 7.34.0 at the root, re-bundled into the ten
  `plugins/dist` bundles; `zod` 4.6.7; plugin-utils 2.7.1. `provider_sdk_
  version_guard` and `zod_version_guard` pin them (P4.D311). No dependency
  moved past the pin.
- **Help / docs:** `help/` is **131 files at the pin**, md5-identical
  (P4.D313); `docs/v4/` has 491 files, byte-identical. Past the pin,
  `a05c6f755` and the outfit-history phases edit help pages (`custom-tools.md`,
  `pascals-workbench.md`, `wardrobe.md`, with new history / rollback text) and
  move the spec to `features/complete/`; `4ae20977f` edits `wardrobe.md`
  again and `1cfc9b504` adds a Wardrobe Pictures section to
  `character-gallery.md` (+13). `docs/developer/API.md` / `DDL.md` move too. The Inventory commits add `help/inventory.md` and
  `help/project-inventory.md` (**131 → 133 at v4 HEAD**) and edit
  `custom-tools.md`, `pascals-workbench.md`, `project-characters.md`,
  `projects.md`, `project-wardrobe.md`, `wardrobe.md`. Each catch-up round re-vendors what
  it absorbs.

### Rounds, guards, state

- **The `25b8fab6a` round — UNIFIED 2026-10-17** (the §6 bullet; the round
  record in `status-log.md`).
- **Next:**
  - The owed dogfood pass (both rounds, any copy up to `f46cf40a2`).
  - The follow-ups smalls round.
  - Then the drift catch-up — now 17 commits, too large for one round.
    Suggested split: (1) the outfit-history + Workbench + gallery round
    (`5cb0e0190` … `1cfc9b504` — the TWELFTH move's table boot-created per
    the 2026-10-19 ruling, no planning ruling left owed — and `e8e17b2d9` bug 252 riding it — it shares the
    restore / import carriers and the `bySlot` shape); then (2) the
    Inventory round (`4fbdad186` → `e1872c94f` → `102a23648`, the
    THIRTEENTH + FOURTEENTH moves in one re-dump; bug 254 can ride round 1
    as a one-guard PORT). The `Inventory/` archive keep (the live-data
    hazard above) is the most urgent single item — it could be a rider in
    round 1 if the human wants the hazard closed sooner.

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
| `5cb0e0190` | 2026-10-16 | feat(pascal): wardrobe picker in the Workbench | **PORT-NEW + PORT (one existing bench arm moves) — NO D23 move (verified: no `lib/schemas` / `migrations/` / `generateDDL` hunk)** (classified 2026-10-17 from the hunks, replacing the 2026-10-16 `--stat`-only candidate; 33 files, +2,300 / −82; `4.11.0-dev.11`; parent `25b8fab6a`). **PORT-NEW (server):** `POST /api/v1/custom-tools?action=preview` gains `wardrobeResolution`, spread LAST (after `gate` / `progress`), only when the fact sheet is the `{ characterId }` form AND the run resolved a non-`clear_slot` wardrobe gesture — NEW `lib/pascal/bench-wardrobe.ts` (119): `resolveBenchWardrobe` loads `loadWearablePool(repos, characterId, [], { operator: true })` (no project tier — the bench has no chat), `take_off` refs through `findInPool`, put-ons through `resolveWearable(pool, ref, mode ?? 'wear', slot)`; per item `{ ref, found: { id, title, types, origin, imageFileId } }` or `{ ref, found: null, reason: 'not_found' \| 'archived' \| 'slot', message }` (`wardrobeItemNotFoundMessage(null, ref)` for not-found); fail-soft WARN `Workbench preview: wardrobe resolution skipped — the wardrobe could not be read` and DEBUG `Workbench preview: wardrobe gestures resolved against the chair` `{ tool, characterId, found, missing }` (context `custom-tools.preview`); the wire types `PreviewWardrobeItemResolution` / `PreviewWardrobeResolution` (`workbench.ts` +34). **PORT-NEW (pure / client):** NEW `lib/wardrobe/wearable-ref.ts` (51 — `matchWearableTitle`, `isTemplatedWearableRef`, `resolveWearableRef`: id first, then trimmed case-insensitive title, templated refs never resolve); `tool-draft.ts` +37 (`WardrobeVantage` editor state, never serialized; `defaultWardrobeVantage` — a character vault beats group / project / General); `wardrobe-container.ts` +27 (`wardrobeReachLabel`: `<name> only` / `members of <g>` / `chats in <p>` / `everyone`); `use-wardrobe-vantage-items.ts` (54); the SPA's `WardrobeReferencePicker` (336), `WardrobeVantageSelect` (184), `SideEffectsSection` +319 (chip with title/id toggle, unresolved advisory never a draft error, uncovered slots flagged), `ProvingBench` +167 (would apply / would skip), `WorkbenchEditor` +50, `wardrobe-item-thumbnail` ±4. **PORT (an existing arm):** `benchCharacterId` adds `characterId.length > 0`, so `metadata: { "characterId": "" }` (which fails `MetadataInputSchema`'s `strictObject({ characterId: min(1) })` branch and lands in the record branch) is now a HAND-TYPED SHEET — 200 with the sheet as given — where it was hydrated into a 404 `Character`; preview AND audit share it. **Behaviour-neutral:** `WearablePool.findByTitle` → `matchWearableTitle` (the same trim / lower-case rule, own items first). Help: `custom-tools.md`, `pascals-workbench.md`, `wardrobe.md` (+5 / −3 across the three); spec `features/complete/pascal-wardrobe-effect-picker.md` (248), FEATURES / ROADMAP / CHANGELOG / README. Depends on `f0db8cbb8`. | `crates/quilltap-core/src/api/custom_tools.rs` — `resolve_metadata`'s doc comment PINS the old arm ("`{ characterId: "" }` … hydrating the empty id into a 404") and `harness/oracle/fixtures/workbench-route-cases.json`'s `preview-empty-character-id` row (`pascal_workbench_route_equivalence`) **trips BY DESIGN** at a pin past this commit; `custom_tool_preview`'s body key order (`p4_6ay_workbench_wire_contract`); `wearable_pool.rs` `find_by_title` (no move) and the `wear_ops` `find_in_pool` / `resolve_wearable` reads it reuses (P4.D279 keystone / P4.D306); `pascal/workbench.rs`. SPA: `apps/web/src/app/screens/custom-tools/{side-effects-section,proving-bench,workbench-editor,wardrobe-fields}.ts` (P4.D307), `apps/web/src/app/pascal/tool-draft.ts`, `apps/web/src/app/wardrobe/wardrobe-container.ts`, the wardrobe item loaders. Help → `p4.9i2`; the spec → `docs/v4/developer/features/complete/`. | **UNPROCESSED** — past the `25b8fab6a` round's pin (observed at its planning close probe 2026-10-16 07:47; classified from the hunks 2026-10-17). |
| `a05c6f755` | 2026-10-16 | feat(pascal): {{character}} placeholder; character-announced custom tools | **PORT-NEW + PORT — NO D23 move (verified: the `lib/schemas/chat.types.ts` +9 / −7 hunk is the `customAnnouncer` DOC COMMENT only; `announceAsCharacter` is a definition-FILE key, not a column)** (classified 2026-10-17 from the hunks; 33 files, +441 / −41; `4.11.0-dev.12`; parent `5cb0e0190`). **`{{character}}`:** `classifyPlaceholder('character')` → `{ kind: 'character' }`; `resolvePlaceholderValue` / `renderTemplate` (absent → renders as written + DEBUG `Custom tool message references {{character}} but nobody in particular rolled`); `executeCustomTool`'s `overrides.character` (trimmed, `\|\| undefined`) reaches the llm prompt, the outcome message, `chipLabel`, and `OutcomeSubjects` (`...(character ? { character } : {})` between `state` and `llm`) — so effect expressions (the grammar's `isKnownRef` now admits it; its refusal text is unchanged) and wardrobe item refs see it; `tool-vocabulary.ts` `character: boolean`; the draft audit never flags it. Entrances: `run-service.ts` `resolveCharacterName` (the entrance's copy, else `characters.findByIdRaw` — the RAW row, no vault overlay; DEBUG `Custom tool run: rolling character name unavailable, {{character}} renders as written` on a read failure); `trigger-runner.ts` passes `character.name` from `findById` (overlaid); the bench passes the seated character's `name` (`resolveMetadata` now returns `{ metadata, characterName? }`; a hand-typed sheet → none). **`announceAsCharacter`** (`QtapCustomToolSchema`, optional boolean between `defaultVisibility` and `parameters`; `KNOWN_TOP_LEVEL_KEYS` derives from the shape, so it grows): the run posts through `postPascalResult({ announcerCharacterId })` → the row carries `customAnnouncer: { kind: 'character', characterId }` BESIDE `systemSender: 'pascal'` / `systemKind` / `pascalMeta` — a combination no writer produced before; a run nobody made stays Pascal's; log fields `announcedBy: 'character' \| 'pascal'` on DEBUG `Custom tool dealt`, `announcerCharacterId` on INFO `[Pascal] Custom-tool outcome posted`. Pre-existing readers keyed on `customAnnouncer` now see Pascal rows (`chat-activity.ts` and `conversation-summary-vault-bridge.ts` skip them; `announcement-attribution` / `markdown-transcript` name the character) — no hunk there, a consequence. SPA: `getAnnouncementSenderName` (the Staff bar and collapsed chip name the announcer; `announcerNames` = off-scene cards overwritten by participants), the `getMessageAvatar` precedence comment, the BuilderForm toggle, the Insert value entry, the run-dialog reference row. `public/schemas/qtap-custom-tool.schema.json` (the property + four description strings). Help `custom-tools.md` +13, `pascals-workbench.md`; spec `pascal-custom-tools.md`. | **Pascal:** `crates/quilltap-core/src/pascal/{placeholders,custom_tools,expressions,tool_vocabulary,run_service,trigger_runner,custom_tool_types}.rs` (+ the SPA twins `apps/web/src/app/pascal/{placeholders,custom-tool-types,tool-draft}.ts`; v5's `KNOWN_TOP_LEVEL_KEYS: [&str; 16]` → 17), `api/custom_tools.rs` (`resolve_metadata`'s return), `services/pascal_writer.rs`; families `pascal_run_custom_equivalence`, `pascal_writers_equivalence`, `pascal_custom_tool_definition_equivalence`, `pascal_tool_vocabulary`, `pascal_workbench(_route)_equivalence`, `announcement_attribution_equivalence` (a Pascal row WITH a `customAnnouncer` — grow the corpus); `apps/web/public/schemas/qtap-custom-tool.schema.json` (byte copy). **Salon SPA:** `apps/web/src/app/chat/{system-message-labels,message-row,announcement-group,chat-view-model}.ts`; the Workbench builder / outcomes / run dialog (P4.6ay, P4.D30, P4.D169). Help → `p4.9i2`. **LIVE-DATA (definition files on the shared doc store):** a v4-authored tool whose effect `value` says `{{character}}` is REFUSED at load by a v5 at the baseline (`{{character}} is not a reference this grammar knows …`) — the tool drops off v5's roster; `announceAsCharacter` is stripped at run (v5 posts as Pascal) but the v5 Workbench's `unknownKeys` keeps it on save; `{{character}}` in a message / chip renders as written. | **UNPROCESSED** — past the `25b8fab6a` round's pin. |
| `fcab3f112` | 2026-10-16 | docs(wardrobe): spec for outfit history, rollback and the History tab | **NO-PORT? — docs-only, VERIFIED** (classified 2026-10-17 from the hunks; 4 files, +669; stamp UNCHANGED at `a05c6f755`'s `4.11.0-dev.12` — no `package.json` hunk; parent `a05c6f755`; `git show --stat fcab3f112 -- lib app packages plugins components help public migrations` EMPTY): NEW `docs/developer/features/wardrobe-outfit-history.md` (663 — corrected in place by `83387d0e5` / `05005c562`, moved to `complete/` with an As-built section by `f46cf40a2`), ROADMAP, CHANGELOG, `.claude/commands/update-documentation.md`. | None. The mirror takes the spec at its `features/complete/` path with `f46cf40a2` — vendor the FINAL file once. | **UNPROCESSED** — past the `25b8fab6a` round's pin; NO-PORT ratification at the next unification. |
| `83387d0e5` | 2026-10-16 | feat(wardrobe): outfit history storage (phase 1) | **PORT-NEW WITH THE TWELFTH D23 MOVE — a NEW TABLE + TWO migrations (one a DATA SEED); NOT destructive** (classified 2026-10-17 from the hunks; 22 files, +1,388 / −12; `4.11.0-dev.14` — the stamp SKIPS dev.13; parent `fcab3f112`). **Schema:** `chat_wardrobe_history` (`lib/database/backends/sqlite/chat-wardrobe-history-ddl.ts`, the single source shared with the tests): `id TEXT PRIMARY KEY`, `chatId TEXT NOT NULL REFERENCES "chats"("id") ON DELETE CASCADE`, `characterId TEXT NOT NULL` (no FK — tombstones), `entryIndex INTEGER NOT NULL`, `slots TEXT NOT NULL` (plain JSON, the `chats.equippedOutfit` shape — NOT a compressed column), `slotsHash TEXT NOT NULL` (`hashEquippedSlots`, 16 hex), `source TEXT NOT NULL`, `actorCharacterId TEXT`, `isCurrent INTEGER NOT NULL DEFAULT 0`, `createdAt TEXT NOT NULL`; UNIQUE `idx_chat_wardrobe_history_entry (chatId, characterId, entryIndex)` and partial UNIQUE `idx_chat_wardrobe_history_current (chatId, characterId) WHERE "isCurrent" = 1` (full DDL in the §1 notes). Migrations, registered AFTER `addComposerSlashCommandsFieldMigration`: `add-chat-wardrobe-history-table-v1` (`introducedInVersion 4.11.0`, `dependsOn sqlite-initial-schema-v1`, `shouldRun` = SQLite and table absent; INFO `Created chat_wardrobe_history table` / ERROR `Failed to create chat_wardrobe_history table`, context `migration.<id>`) and `seed-chat-wardrobe-history-v1` (`dependsOn` the first; `shouldRun` = table + `chats` exist with `equippedOutfit` + `updatedAt` — NO once-gate of its own beyond the runner's `migrations_state` and a per-pair skip; one transaction INSERTs entry `0`, `source 'seeded'`, `actorCharacterId NULL`, `isCurrent 1`, `createdAt = chats.updatedAt`, a `randomUUID` id, per `(chat, characterId)` of every non-null `equippedOutfit` (`normalizeEquippedSlots`; unparseable / array maps skipped); `reportProgress`; INFO `Seeded the chat wardrobe history from current outfits` `{ chatsScanned, entriesSeeded, pairsSkipped }`). Zod `chat-wardrobe-history.types.ts` (`WardrobeHistorySourceSchema` — seven sources; `ChatWardrobeHistoryRowSchema`; `WardrobeHistoryProvenance`, `WardrobeHistoryIntent`). **The repository is a PLAIN class** (`ChatWardrobeHistoryRepository`, `dbTarget 'main'`, NOT `AbstractBaseRepository` — no `ensureCollection`, so v4's generateDDL NEVER renders the table): `settle` (the one runtime writer — append at `max + 1` / `unchanged` on an equal hash / cursor move; the first-touch seed of an empty `0` for a pre-history seat; one synchronous transaction; never throws — WARN `Wardrobe history settle failed; the outfit change stands unrecorded`; `publishRealtime('chats', chatId)` on append / move; logger `child({ module: 'wardrobe-history' })`), `upsertRows` (restore / import — `ON CONFLICT("id") DO UPDATE`, slots re-normalized + hash RECOMPUTED, per-row WARN-and-skip), `deleteByCharacter`, `findEntries` / `findCurrent` / `findByChatId` / `findAll`. `cascade-delete.ts` drops a character's timelines after the wear fold (DEBUG `[CascadeDelete] Dropped outfit history`, ERROR `Failed to drop outfit history for character <id>`). The child proxy's `'wardrobeHistory.settle': 'write'` override (buffered; NO-PORT — v5 has no child, P4.D255 (5)). Two prettify labels. `docs/developer/DDL.md` +48. No settle CALLER yet (phase 2). | **D23 — the existing machinery is STRUCTURALLY BLIND to this move:** `harness/oracle/provision/dump-fresh-schema.ts` drives each repo's `count` / `findAll`, and this repo's `findAll` throws `no such table` → skipped, so `fresh_schema.json` will NOT carry it; `dump-migration-indexes.ts` keeps only indexes on tables `fresh_schema.json` creates (the `wardrobe_items` class — the `QT_REAL_BOOT_MASTER` cross-check filters on `freshTables.has(r.tbl_name)` too), so neither UNIQUE index lands; `provisioning_equivalence` (1d, LIVE generateDDL) stays GREEN — the D23 tripwire does NOT fire. The family expected to trip is `zero_migrations_rerun_equivalence` (v4's runner would run both ids on a v5-chained instance). Needs a RULING: P4.D255's overruled R-A (`migration_tables.json` dumped by v4's REAL runner — the first table that cannot keep a generateDDL shape) or a boot-created table (the `e5c6bd0c0` structural-pass precedent). Plus a boot ensure per migration, the seed STAMPED (P4.D255 R-B precedent), `restore_vintage_state`'s boot-align list, the committed `*-main.db` pairs. **Server:** a NEW `db/` repository on the writer task (single-writer: `settle` is a read-compare-write in ONE RW transaction); `services/cascade_delete.rs` (P4.D256's wear fold sits beside it); `wardrobe.rs` `hash_equipped_slots` (exists — `outfit_hash_equivalence`); the realtime `chats` publish. **LIVE-DATA:** none blocking (see §1 notes) — v5 at the baseline neither reads nor writes the table. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER it as ONE programme with `05005c562` … `f46cf40a2` (the table has no writer until phase 2). |
| `05005c562` | 2026-10-16 | feat(wardrobe): record outfit history at the announcement (phase 2) | **PORT** (classified 2026-10-17 from the hunks; 30 files, +376 / −124; `4.11.0-dev.15`; parent `83387d0e5`; NO schema move). `OutfitChangeContext` gains a REQUIRED `provenance` (`{ source, actorCharacterId? }`) and an optional `historyIntent` (rollback only) and now `extends WardrobeTurnContainers`; NEW pure `lib/wardrobe/turn-containers.ts` (`newWardrobeTurnContainers()` → `{ pendingWardrobeAnnouncements: Set, pendingWardrobeHistory: Map }`, `turnContainersOf`). `recordPendingWardrobeAnnouncement(repos, …)`: inside a turn the character joins the Set AND its intent REPLACES any earlier one in the Map (last wins; a Set without a Map settles at once); outside a turn the announcement is scheduled, THEN the history settled (neither waits on the other's success; WARN `Failed to settle wardrobe history`). `flushPendingWardrobeAnnouncements(context, repos?)` drains both containers — per character: settle (default intent `append` / `character` / self) THEN schedule, source context `orchestrator-turn-end`. Provenance per site: `?action=equip` → `operator`; the `wardrobe_*` tool table → `character` with `actorCharacterId` = the caller (also `equip_now` for a recipient via the shared create / archive handlers); Pascal `applyWardrobeEffects` → `pascal` with the rolling character. `applyOutfitSelections` settles `append` with its own `source` (`chat-start` / `participant-added` / `merge`) per character after the write, and a failed write now `continue`s past it. `createToolContext`, Brahma (one-shot + orchestrator) build their containers with the factory; `run-service` / `run-custom-handler` / `side-effects` / `tool-executor` thread both through `turnContainersOf`; `WardrobeToolContext` becomes `Omit<OutfitChangeContext, 'historyIntent'>`. Spec corrected in place. | `crates/quilltap-core/src/services/outfit_change_effects.rs` (notify / record-pending / `flush_pending_wardrobe_announcements` — the P4.D282 turn-end drain), `services/tool_execution.rs`, `services/orchestrator.rs`, `services/brahma_console/`, `services/outfit_selections.rs` + `services/chat_create.rs` (the chat-start / join / merge arrivals), `api/chat_outfits.rs` (equip), `tools/wardrobe_tool_table.rs` + `tools/wardrobe_shared.rs` + `tools/executor.rs`, `pascal/{wardrobe_effects,run_service}.rs`, `tools/run_custom.rs`; families `outfit_change_effects_tier3_equivalence`, `chats_outfits_tier2_equivalence`, `wardrobe_tools_equivalence`, the `pascal-wardrobe-effects` oracle, chat-create capstone. The child-proxy buffering and the module split for jest mocks are NO-PORT. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER with `83387d0e5` (no writer before this commit, no reader before `ebb480edd`). |
| `ebb480edd` | 2026-10-16 | feat(wardrobe): wardrobe_rollback and wardrobe_history tools (phase 3) | **PORT-NEW — TWO model-visible tools** (classified 2026-10-17 from the hunks; 32 files, +1,274 / −10; `4.11.0-dev.16`; parent `05005c562`; NO schema move). NEW `lib/wardrobe/history/rollback.ts` (183 — `rollbackWardrobe`, the ONE rollback: target = `index ?? current − 1`; refusals `no_history` / `at_first` / `out_of_range` / `already_current` with their sentences; `filterStaleItems` leaves off missing and archived ids unless the archived one is worn in that slot; ONE `commitEquippedOutfit` with `wornBundles: []`; `notifyWardrobeChanged` with a `cursor` intent; INFO `Rollback left a stale item off` per item, DEBUG `Rolled a wardrobe back`) and `describe-entry.ts` (95 — `resolveHistorySlots`, `staleItemsOf`, `summarizeHistorySlots`, `changedByLabel`'s seven arms). `wardrobe_rollback` (`index` int ≥ 0 optional) and `wardrobe_history` (`limit` int 1–100, default 20; the last `limit` entries + the current one if older, `gap_before_window`) — definitions, handlers (validation / failure texts, `formatWardrobe{Rollback,History}Results` bytes), gated by `canDressThemselves` at every site that lists `wardrobe_take_off`: `lib/tools/index.ts`, `plugin-tool-builder.ts`, the executor list + tool table (seven → nine), `GET /api/v1/tools` (`Return to an Earlier Outfit` / `Outfit History`, category `wardrobe`), simple-JSON prompt + parser aliases (`rollback_outfit`, `undo_outfit`, `outfit_history`), the legacy text-block prompt (`[[OUTFIT_HISTORY /]]`, `[[ROLLBACK_OUTFIT index="2" /]]`) + parser, `pseudo-tool.service.ts`, `streaming.service.ts` `buildTools`; Brahma + the Scenario Builder set both `false`; `ToolMessage.tsx` (`Earlier Outfit` / `Outfit History` rows, the rollback lines); the tool-definitions snapshot +30. | `crates/quilltap-core/src/tools/` — `definitions/data.rs` (`tool_definitions_equivalence`), `wardrobe_tool_table.rs`, `executor.rs`, `text_block_parser.rs` + its prompt, `simple_json_parser.rs` + its prompt, the plugin tool builder / `build_tools` options; `services/tools_inventory.rs` (`tools_inventory_equivalence`); `wardrobe_tools_equivalence`; the Brahma / Scenario Builder tool sets; SPA `apps/web/src/app/chat/tool-message.ts`. v5's "all 57 tools" count grows by two. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER with the programme (port `describe-entry` at `f46cf40a2`'s shape). |
| `8d8618819` | 2026-10-16 | feat(wardrobe): history API and the Wardrobe dialog's History tab (phase 4) | **PORT-NEW** (classified 2026-10-17 from the hunks; 22 files, +903 / −14; `4.11.0-dev.17`; parent `ebb480edd`; NO schema move). **Server:** `GET /api/v1/chats/[id]?action=wardrobe-history&characterId=…` (400 `characterId is required`; 404 `Chat`; no rows → `{ currentIndex: null, entries: [] }`; else entries `{ index, at, source, actorCharacterId, actorName, changedBy, isCurrent, summary, slots }` resolved against `loadWearablePool(repos, characterId, undefined, { chatId, operator: true })`, names via `findByIdRaw` (failure → null; self fallback `the character`); DEBUG `[Chats v1] Read wardrobe history`; ERROR → 500 `Failed to read wardrobe history`) and `POST …?action=wardrobe-rollback` `{ characterId, index }` (Zod 400 joined messages; operator provenance, ledger source `ui`; a refusal → 400 with rollback's sentence + INFO `[Chats v1] Wardrobe rollback refused`; success INFO `[Chats v1] Wardrobe rolled back`, body `{ equippedSlots, previousIndex, restoredIndex, skipped }`; 500 `Failed to roll back the outfit`); the action lists in `route.ts` / `get.ts` / `post.ts` / `actions/index.ts`. **Client:** query keys `wardrobe.histories(chatId)` / `wardrobe.history(chatId, characterId)`; the `chats` realtime topic refetches `wardrobe.histories(id)`; NEW `outfit-history-tab.tsx` (228 — newest first, *Wearing now*, *Wear again* with the stale-garment tooltip, the discard-staged-edits confirm), the dialog's third tab, `useStagedLiveOutfits` `isDirty` / `discard`. `docs/developer/API.md` +52. | `crates/quilltap-web/src/wardrobe_routes.rs` (the chat `?action=` dispatch + the `COLLAPSE_CENSUS` rows new routes need), `crates/quilltap-core/src/api/chat_outfits.rs` (the outfit / equip family — `chats_outfits_tier2_equivalence`), the chat action censuses; SPA `apps/web/src/app/wardrobe/{wardrobe-control-dialog,staged-live-outfits}.ts`, `apps/web/src/app/core/realtime-topic-map.ts`, `apps/web/src/app/chat/chat-keys.ts` (P4.D277 / P4.D285 wardrobe dialog; bug 198's refresh); `docs/v4/developer/API.md`. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER with the programme. |
| `7953da3a0` | 2026-10-16 | feat(wardrobe): outfit history in .qtap exports and backups (phase 5) | **PORT — the backup / restore / `.qtap` carriers** (classified 2026-10-17 from the hunks; 22 files, +611 / −8; `4.11.0-dev.18`; parent `8d8618819`; NO schema move). **Backup:** `data/wardrobe-history.json` (`findAll` — by `chatId`, `characterId`, `entryIndex`), manifest count `wardrobeHistory`, DEBUG `Collected outfit history for backup`. **Restore:** `parseBackupZip` reads it optional; step `22n-ter` after the wear ledger (`upsertRows`; failure warning `Failed to restore the outfit history: <msg>` + WARN `Failed to restore outfit history`; DEBUG `Restored outfit history` `{ total, restored }`); `RestoreSummary` / preview counts; the replace-mode clear list gains `chat_wardrobe_history` after `wardrobe_wear_stats`; `remapBackupData` (new-account) remaps `id` / `chatId` / `characterId` / `actorCharacterId` through the remapper, slot ids through `wardrobeItemIdRemap`, and RECOMPUTES `slotsHash`. **`.qtap`:** `streamChats` yields `{ kind: 'wardrobe_history', chatId, data: row }` per chat AFTER the informs and BEFORE the annotations (WARN `Failed to load outfit history for export`), counts `wardrobeHistory`, `QtapWardrobeHistoryRecord`, `ChatsExportData.wardrobeHistory`; the stream assembler collects the kind; NEW `import-wardrobe-history.ts` (127 — step `7f` after characters / chats / stores / the wear ledger: rows Zod-validated whole (`Dropped N malformed outfit-history row(s).`), a FRESH id each, `chatId` / `characterId` / `actorCharacterId` + slot item ids remapped, hash recomputed, index / cursor / dates kept, a `(chat, character)` pair already holding rows NEVER merged into; INFO `Imported outfit history` `{ incoming, written, droppedInvalid, skippedTimelines }`; failure warning `Failed to import the outfit history: <msg>`). `public/schemas/qtap-export.schema.json` +33. | `crates/quilltap-core/src/services/backup/{collect,staging}.rs`, `backup/restore/{archive,orchestrator}.rs` (the restore census — `RESTORE_TS_MESSAGES` in BOTH TS + Rust), `services/delete_all.rs` (the replace-mode main-table list, beside `wardrobe_wear_stats`), the new-account remap (`backup_uuid_remap_equivalence`), `services/qtap_export/` + `key_order.rs`, `services/quilltap_import/ndjson.rs` (today the unknown-kind arm SILENTLY skips `wardrobe_history` — the P4.D264 `wardrobe_wear` precedent), the import executor + `import_warning_text_guard`; `crates/quilltap-core/src/generators/qtap-export.schema.json` (byte copy); families `system_backup` / `system_restore(_state)` / `chat_export` / `system_export` / `qtap_import` / `system_import(_state)`. P4.D264 / P4.D272 / P4.D293 / P4.D294 shapes. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER with the programme. |
| `f46cf40a2` | 2026-10-16 | docs(wardrobe): outfit history docs, spec to complete (phase 6) | **PORT (one model-visible text fix) + docs** (classified 2026-10-17 from the hunks — the subject says docs, the hunks carry code (§5.3); 13 files, +149 / −19; `4.11.0-dev.19`; parent `7953da3a0`). `describe-entry.ts` `summarizeHistorySlots`: was `describeOutfit(values).replace(/\s*\n+\s*/g, '; ')` (kept the Markdown `- ` bullets and `**`), now per line strips a leading `- ` / `* ` and every `**`, trims, drops empties, joins `; ` — the `summary` of the `wardrobe_history` tool (MODEL-VISIBLE) and of `?action=wardrobe-history`. Help `wardrobe.md` (+29 / −4 — "Looking Back --- the Outfit Record", seven → nine tools, the `[[OUTFIT_HISTORY]]` / `[[ROLLBACK_OUTFIT]]` text-block tags, the `canDressThemselves` line, two front-matter tags). Docs: the spec MOVED to `features/complete/` with an As-built section, `BACKGROUND_JOBS_CHILD.md` (the settle override), ROADMAP, CHANGELOG, `update-documentation.md`, v4's own `CLAUDE.md` bullet (NO-PORT). | The v5 port of `describe-entry` is OWED by `ebb480edd` / `8d8618819` — port THIS shape only (no interim); `describeOutfit` itself unchanged (`describe_outfit_equivalence` does not move). Help → `p4.9i2`; `docs/v4/developer/BACKGROUND_JOBS_CHILD.md` + the spec at `features/complete/wardrobe-outfit-history.md`. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER with the programme. |
| `4ae20977f` | 2026-10-17 | feat(wardrobe): outfit history records who did the dressing | **PORT — INSIDE the still-pending TWELFTH D23 move: a NEW nullable column + a THIRD migration; NOT destructive** (classified 2026-10-18 from the hunks; 33 files, +343 / −42; `4.11.0-dev.20`; parent `f46cf40a2`). **Schema:** `chat_wardrobe_history."actorToolName" TEXT` appended LAST in `CHAT_WARDROBE_HISTORY_DDL` (so a fresh table matches an ALTERed one); the one INSERT grows to 11 positional params (`actorToolName` between `actorCharacterId` and `isCurrent`). NEW migration `add-chat-wardrobe-history-tool-name-v1` (`introducedInVersion 4.11.0`, `dependsOn add-chat-wardrobe-history-table-v1`; `shouldRun` = SQLite + table present + column absent; `addColumnIfMissing(table, 'actorToolName', 'TEXT')`; INFO `Added actorToolName column to chat_wardrobe_history` only when added; ERROR `Failed to add actorToolName column to chat_wardrobe_history`, context `migration.<id>`), registered BETWEEN the table migration and the seed; `seed-chat-wardrobe-history-v1` now `dependsOn` BOTH and inserts `null` for the column; prettify label `Ruling a column for the contraption that did the dressing`. **Zod:** `ChatWardrobeHistoryRowSchema.actorToolName: z.string().nullable().default(null)` (pre-column bundles / backups still parse); `WardrobeHistoryProvenance.toolName?`. **Repository:** `settle` writes `actorToolName = source === 'pascal' ? toolName ?? null : null` on both append arms (incl. the first-touch seed's entry 1); `normalizeRow` `?? null`; `insertParams` now `actorCharacterId ?? null`; `upsertRows`' `ON CONFLICT` UPDATE sets it. **Provenance:** Pascal `applyWardrobeEffects` passes `toolName`; NEW `lib/wardrobe/history/operator-provenance.ts` `operatorWardrobeProvenance(chat)` → `{ source: 'operator', actorCharacterId? }` from `findActiveUserParticipant(participants, activeTypingParticipantId, impersonatingParticipantIds)` (DEBUG `Resolved operator for an outfit change`, context `wardrobe-history`); `?action=equip` now reads `chats.findById` first (absent → bare `{ source: 'operator' }`), and `?action=wardrobe-rollback` uses it too. **Model-visible / wire text:** `changedByLabel` — operator with an actor → `the operator, as <name>` (self falls back to `selfLabel`; no resolvable name → `the operator`); pascal → `Pascal's <tool>` and `Pascal's <tool> (rolled by <name>)`, plain `Pascal` for pre-column rows; `wardrobe_history`'s handler now NAMES the actor for an operator entry even when it is the character itself (`&& row.source !== 'operator'`); `?action=wardrobe-history` entries gain `actorToolName` between `actorName` and `changedBy` (order `index, at, source, actorCharacterId, actorName, actorToolName, changedBy, isCurrent, summary, slots`). `qtap-export.schema.json` gains the property + a reworded `actorCharacterId` description; the `.qtap` export / import copy it as-is (comment-only hunks). SPA: `OutfitHistoryEntry.actorToolName`. Help `wardrobe.md` (±1), the spec's As-built section, `API.md`, `DDL.md`. | Folds into the outfit-history programme (`83387d0e5` … `f46cf40a2`) — port the table, the settle writer, `describe-entry` and the history wire at THIS shape, never the interim one. **D23:** the same blindness as `83387d0e5` (generateDDL never renders the table) — the provisioning ruling must cover the column and the third ensure, registered table → tool-name → seed; `restore_vintage_state`'s boot-align list and `zero_migrations_rerun_equivalence` grow by one id. **v5 reuse:** `crates/quilltap-core/src/participant_filters.rs` `find_active_user_participant` (Phase 1 turn manager) is the resolver twin; `api/chat_outfits.rs` (equip — the extra chat read), `pascal/wardrobe_effects.rs`; the backup / `.qtap` carriers (`7953da3a0`'s row) carry the column; `qtap-export.schema.json` byte copy. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER with the outfit-history programme. |
| `1cfc9b504` | 2026-10-17 | feat(aurora): wardrobe pictures section in the character gallery | **PORT-NEW — a read-only route + a gallery section; NO schema move** (classified 2026-10-18 from the hunks; 23 files, +716 / −18; `4.11.0-dev.21`; parent `4ae20977f`). **Server:** `GET /api/v1/characters/[id]/wardrobe-images` (`createContextParamsHandler`; 400 `Missing character id`; a thrown `Character not found…` → 404 `Character`; else ERROR `[Characters/WardrobeImages v1] Error listing wardrobe pictures` + 500 with the message; DEBUG `[Characters/WardrobeImages v1] Listed wardrobe pictures` `{ userId, characterId, total }`; body `{ entries, total }`). NEW `lib/wardrobe/gallery-images.ts` `listWardrobeGalleryImages`: `characters.findById` (null → throw `Character not found: <id>`; archived → `{ entries: [], total: 0 }` + DEBUG, pool never read); `loadWearablePool(repos, characterId, [])` (NO project tier, NOT operator mode); empty pool → empty without the file read; ONE `files.findByLinkedToAny(itemIds)`; keeps `category === 'IMAGE' && source === 'GENERATED'`; item = the first `linkedTo` id in the pool; entry `{ fileId, fileName, url, thumbnailUrl, mimeType, fileSizeBytes, width, height, createdAt: String(...), generationPrompt, generationModel, itemId, itemTitle, scope, locationLabel: wardrobeOriginLabel(origin) ?? 'Own wardrobe', itemArchived, isCurrentItemImage }`; sorted by `createdAt` string DESCENDING (a stable comparator — ties keep file-read order); DEBUG `[WardrobeGallery] Listed wardrobe pictures`. NEW `FilesRepository.findByLinkedToAny(ids)` (chunks of 500 through `findByFilter({ linkedTo: { $in } })`, de-duplicated by id in first-seen order, under `safeQuery` `Error finding files linked to entities`). **Client:** `WardrobeImagesSection` (collapsible third shelf under Avatar Rolls), `useWardrobeImages`, `GalleryGrid` / `GalleryImage` caption card (title + *(archived)*, location line), `EmbeddedPhotoGallery`, query key `characters.wardrobeImages(id)`, `_content.css` + the theme-storybook copy (+22 each). Help `character-gallery.md` +13; `API.md` +8. | **Server:** `crates/quilltap-web/src/characters_routes.rs` (a new collection route + its `COLLAPSE_CENSUS` row), `crates/quilltap-core/src/db/files.rs` (`find_by_linked_to` exists per-entity; the `$in` bulk read is new — its row order is v4's `findByFilter` order, measure it), `wearable_pool.rs` (the pool, P4.D279), `services/wardrobe_item_images/service.rs` (`wardrobe_image_url`; the thumbnail URL twin), `wardrobeOriginLabel` (P4.D285's SPA `wardrobe-container.ts` — a server-side twin is new). **SPA:** `apps/web/src/app/screens/characters/view/tabs/{gallery-tab,avatar-rolls-section}.ts` + `characters.api.ts` (the Aurora gallery — the Avatar Rolls vertical), `apps/web/src/app/images/image-detail-modal.ts` (P4.D277's shared full-screen frame: Copy / Download / Save to my gallery), the qt-components CSS. Help → `p4.9i2`. | **UNPROCESSED** — past the `25b8fab6a` round's pin; independent of the outfit-history programme (can be its own lane). |
| `54bc17a3c` | 2026-10-18 | Add Inventory design spec (Pascal's per-tier objects, archetypes, kits) | **NO-PORT? — docs-only, VERIFIED** (classified 2026-10-18; 3 files, +288; no `package.json` hunk — stamp stays `4.11.0-dev.21`; parent `1cfc9b504`; authored by a Claude agent in v4): NEW `docs/developer/features/inventory.md` (280), ROADMAP, CHANGELOG. `git show --stat 54bc17a3c -- lib app packages plugins components help public migrations` EMPTY. | None. The spec moves to `features/complete/inventory.md` with an As-built section in `4fbdad186` — vendor the FINAL file once. | **UNPROCESSED** — past the `25b8fab6a` round's pin; NO-PORT ratification at the next unification. |
| `f5db10034` | 2026-10-18 | Docs: Inventory tools and Pascal support design spec (#99) | **NO-PORT? — docs-only, VERIFIED** (classified 2026-10-18; 4 files, +201 / −3; no stamp; parent `54bc17a3c`): NEW `docs/developer/features/inventory-tools-and-pascal.md` (196), `inventory.md` ±1, ROADMAP, CHANGELOG. No production path. | None. Moved to `features/complete/` by `e1872c94f` — vendor the final file. | **UNPROCESSED** — past the `25b8fab6a` round's pin; NO-PORT ratification at the next unification. |
| `e8e17b2d9` | 2026-10-18 | Fix: restore and import keep chat outfits (bug 252) (#101) | **PORT — v4-ORIGINATED bug 252, and v5 HAS THE SAME DEFECT today in BOTH carriers; NOT a convergence (v4's `bugs.md` row: found by reading `remapBackupData` beside the outfit-history remap; v5 status **Owed**; absent from v5's porting docs). NO schema move, NO route, NO model-visible text** (classified 2026-10-18 from the hunks; 13 files, +687 / −6, +387 of it tests; `4.11.0-dev.21` → `dev.26`; parent `f5db10034`). **The defect:** four chat columns keyed `{ [characterId]: … }` were copied through unchanged by `remapBackupData` (new-account restore — every character gets a fresh id) and by the `.qtap` `reconcileRelationships` (where `idMaps.characters` maps to a fresh / existing id): restored chats wear nothing, lose their per-chat outfit avatars and Courier checkpoints, and the outfit history disagrees with the live outfit. **NEW pure `lib/chat/character-keyed-fields.ts`:** `CHARACTER_KEYED_CHAT_FIELDS = [equippedOutfit, characterAvatars, courierCheckpoints, pendingOutfitNotifications]` (rekey order; `compiledIdentityStacks` / `commonplaceSceneCache` deliberately excluded); `remapCharacterKeyedChatFields(chat, { characterId → string\|null, wardrobeItemId, fileId, messageId })` → `{ patch, unreadable, droppedEntries }` — null / missing skipped, JSON strings parsed, non-objects / unparsable → `unreadable` and left alone, a `null` character remap DROPS the entry; `equippedOutfit` entries via `normalizeEquippedSlots` rebuilt with `bySlot` (a four-slot bag gains `hair`), slot ids via `wardrobeItemId`; `characterAvatars[*].imageId` via `fileId`, `courierCheckpoints[*].lastResolvedMessageId` via `messageId` (only when strings); `pendingOutfitNotifications` re-keyed only. **Restore (`uuid-remap.ts`):** module logger `backup:uuid-remap`; `characterId` → `backupCharacterIds.has(id) ? remapper.remap(id) : null` (a character NOT in the backup is DROPPED, no mint); `wardrobeItemId` → `wardrobeItemIdRemap.get(id) ?? id`; `fileId` / `messageId` → `remapper.remap`; the pass sits AFTER `scenarioSource`, BEFORE `PARTICIPANT_ID_JSON_FIELDS`; `Object.assign` keeps key positions; WARN `Chat character-keyed columns unreadable; left as given` `{ chatId, fields }`, DEBUG `Remapped character-keyed chat fields` `{ chatId, fields, droppedEntries }`. **Mint order MOVES** (courier message ids and unseen avatar file ids now mint at the chat, before turn-manager JSON / participants / messages) — counting-remapper corpora shift wherever a fixture carries these columns. **Import (`reconcile.ts` + `execute.ts`):** a 5th `options.wardrobeItemIds` from the existing `buildImportedWardrobeItemIdMap`; after the scenario-source re-point, before the tags; `characterId` → `idMaps.characters.get(id) ?? id` (no drop); `fileId` → `idMaps.docMountFileLinks.get(id) ?? id`; `messageId` identity (already reminted at create); ANY readable non-null column sets `hasUpdates`; WARN `Chat character-keyed columns unreadable; left as imported`, DEBUG `Remapped character-keyed chat fields` (no `droppedEntries`). No `warnings[]` text → `import_warning_text_guard` and `RESTORE_TS_MESSAGES` do not move. Docs: `bugs.md` 252, `bugs/fixed/bug-252-restored-outfit-keys-stale.md`, CHANGELOG. §5.3: the squashed message names a `remapEquippedOutfit` that does not exist (superseded). | **Restore:** `crates/quilltap-core/src/services/backup/uuid_remap.rs` `remap_chat` (~l. 852) copies all four through today — same defect; thread `build_wardrobe_item_id_remap`'s map (exists) + the backup's character-id set; the pass between the scenario-source block and the `PARTICIPANT_ID_JSON_FIELDS` loop (mint order); the module's FIRST tracing target (`backup:uuid-remap`). **Import:** `services/quilltap_import/reconcile.rs` (chat loop ~l. 377) + `wardrobe_wear.rs::build_imported_wardrobe_item_id_map` (exists) via `quilltap_import/mod.rs`; `id_maps.doc_mount_file_links` exists; **`db/chats.rs` `ChatUpdate` LACKS `pending_outfit_notifications`** (verified 2026-10-18 — `ChatCreate` has it). **Shared:** a new twin of `character-keyed-fields.ts` over `wardrobe.rs::normalize_equipped_slots` + a `bySlot` twin, with its own tier-1 family. **Families:** `backup_uuid_remap_equivalence` (corpus carries none of the four — grow it: dropped phantom, JSON-string, unreadable, four-slot, mint-order rows), `system_restore(_state)` new-account rows, `system_import(_state)` / `system_import_execute` (the P4.D294 remint plant seeds `courierCheckpoints` — moves on regen), `qtap_import_equivalence`. Port at `7953da3a0`'s `slotsHash` / `bySlot` shape; `4fbdad186` extends the list with `equippedInventory`. | **UNPROCESSED** — past the `25b8fab6a` round's pin. Natural rider for the outfit-history round (shares the restore / import carriers). |
| `4fbdad186` | 2026-10-18 | Feature: The Inventory — objects a character carries (#100) | **PORT-NEW WITH THE THIRTEENTH D23 MOVE (`chats."equippedInventory"`, nullable, one additive migration; NOT destructive, no index) + SIX PORT hunks on ported surfaces; NO model-visible tool** (classified 2026-10-18 from the hunks; 151 files, +13,094 / −1,022; `4.11.0-dev.26` → `dev.33`; squash of #100, phases A–I + review; parent `e8e17b2d9`). **Schema:** `equippedInventory: JsonSchema.nullable().optional()` in BOTH `ChatMetadataSchema` and `ChatMetadataBaseSchema` after `equippedOutfit` — generateDDL renders `"equippedInventory" TEXT` at cid 66 (chats DDL 103 → 104 lines); `add-chat-equipped-inventory-v1` (`introducedInVersion 4.11.0`, `dependsOn add-equipped-outfit-field-v1`, column-gated, `addColumnIfMissing('chats','equippedInventory','TEXT DEFAULT NULL')` — APPENDED; INFO `Added equippedInventory column to chats` / ERROR `Failed to add equippedInventory column to chats`; registered LAST after `seedChatWardrobeHistoryMigration`; prettify `Fitting each chat with a croupier's ledger of what everyone is carrying`). Value `{ [characterId]: string[] }` sorted / de-duplicated. `patchOnlyFields()` adds it (never on a whole-row `_update`); ONE writer `setEquippedInventory` (DEBUG `Equipped inventory write`), `getEquippedInventory` (`{}` fallback), `normalizeEquippedInventory`. `LLMLogTypeEnum` + `INVENTORY_ITEM_IMAGE`; `IMAGE_SPEND_LOG_TYPES` gains it. **The objects are FILES, not tables:** one `.json` per object under each tier's `Inventory/` (character vault, Quilltap General, project / group official stores), `JSON.stringify(out, null, 2) + '\n'` in `INVENTORY_FILE_KEYS` order, `sanitizeFileName(title \|\| name \|\| stem \|\| 'object-<id8>').json`, pictures under `Inventory/images/<itemId>/`, folder depth 8 with `images` reserved; `lib/schemas/inventory.types.ts` (`kind item\|kit`, `name` 1–200, `title`, `description` ≤4000, `imageDescription` ≤2000, `equippedByDefault`, `maxEquipped` 1–999 default 1, `canCopy`, `isArchetype`, `parentId`, `itemIds` ≤200, `emoji` ≤16, `svg` ≤16 KiB, `imageFileId`, `metadata`, `archivedAt`; instances SPARSE — inheritable fields omitted). **Server PORT-NEW:** ONE route factory for `/api/v1/{characters,groups,projects}/[id]/inventory(/[itemId])` + `/api/v1/inventory(/[itemId])` (collection GET / folders / archetypes; POST create / instantiate / duplicate / create-, rename-, delete-folder / move- / copy-items; item GET / dependents / PUT / clear-override / detach / DELETE `?force`); `POST /api/v1/inventory/transfers`; `/api/v1/inventory/[itemId]/images` (generation SYNCHRONOUS — `Still-life product photograph of …` through `withArtDirection`); refusal codes `ARCHETYPE_IN_USE` / `ARCHIVED_DEPENDENTS` / `NOT_COPYABLE` / `CAP_EXCEEDED` / `INVALID_PARENT` / `KIT_NESTING` / `KIT_STRANDED` / `NOT_EQUIPPABLE` / `INVALID_SHAPE` / `NOT_A_PARTICIPANT` / `INVALID_SVG` (allowlist SVG sanitiser); chat GET `?action=inventory`, POST `inventory-equip` / `inventory-unequip` / `inventory-reset-defaults` (plan-then-write, kit expansion, archetype members INSTANTIATED as vault files, cap check before any write, per-chat lock, realtime `chats`); Pascal's `*<Name> sets down … and takes up …*` (`systemKind 'inventory-change'`, no `pascalMeta`, INFO `[Pascal] Inventory change posted`); `refreshAvatarForAppearance` (`refreshAvatarForOutfit` aliased). **Model-visible:** the trailing `## Carried and equipped\n- <Title (name)\|name>: <description>` section (after progressions, before inform; kits and undescribed skipped; continue mode skipped — reversed by `102a23648` bug 253); Carina `[question, progressions, inventory]` joined by `\n\n---\n\n`; the greeting user turn `<instruction>\n\n---\n\n<sections>`; `carrying <imageDescription, …>` in appearance resolution, the story background, `buildExpansionContext` and the avatar prompt. Arrival defaults at chat create / seat add / merge write `{charId: []}` EVEN WITH NO INVENTORY. Carriers: new-account restore step `22f-quater` repoints objects' `imageFileId`; `remapLinkId` keeps object ids VERBATIM in `files.linkedTo`; `equippedInventory` joins bug 252's `CHARACTER_KEYED_CHAT_FIELDS`; `.qtap` export spreads it, import creates it. `docMountDocuments.findManyInFolderContaining` (raw `instr`). **PORT hunks:** (1) the appearance-sanitizer classify text `${phys} ${cloth}` → `[phys,cloth,carry].filter(Boolean).join(' ')` — moves the moderation bytes whenever physical or clothing is EMPTY, inventory or not; (2) chat create / seat add / merge DB state (`{charId: []}`); (3) the archive prune keep-set adds `Inventory/`; (4) the new-account remap (`remapLinkId`, the repoint); (5) the wardrobe lift into generic `item-folders.ts` / `item-folder-ops.ts` + `imagesFolder` / `ext` options — strings preserved byte-for-byte (no behaviour); (6) `IMAGE_SPEND_LOG_TYPES`. Client: the Inventory dialog, a new `inventory` workspace tab kind (`The Inventory`, icon `dice`), the editor / transfer / folder dialogs, Carrying-now pane, Prospero `InventoryCard`, the ParticipantCard button, SaveImageDialog `inventory` target, `system-message-labels` `'inventory-change'` (Pascal `medium`), shared `components/item-browser/*`. Help NEW `inventory.md` (+149), `project-inventory.md` (+38) → **133 files**, plus four +1 cross-links; `API.md` +39; spec → `features/complete/inventory.md`; `qtap-export.schema.json` +8. §5.3: "patch-only, one writer" holds for `_update` only (`chats.create` writes it via import / restore); the message never mentions the classifier-join byte change or the empty arrival entries. | **D23:** `fresh_schema.json` re-dump — SEEN by `provisioning_equivalence` 1d + `zero_migrations_rerun_equivalence`; a boot ensure carrying BOTH shapes vs v4's REAL runner, on `restore_vintage_state`'s boot-align list; `db/chats_read.rs` (explicit column list), `db/chats.rs` (create INSERT, named `update`, a new `set_equipped_inventory` on the writer task); `chats_tier2_equivalence`, the committed `*-main.db` pairs; `services/qtap_export/schema-key-order.json` (`equippedInventory` between `equippedOutfit` and `characterAvatars`); the `generators/` schema copy. **PORT hunks:** `services/appearance_resolution.rs` (~l. 484, `appearance_sanitize_gate_tier3_equivalence`); `services/chat_create.rs` (`chat_create_capstone_equivalence`, `initial_greeting_equivalence`), `services/chat_participants.rs` / `api/chat_cast.rs` (`chat_cast_routes_equivalence`, `chats_participants_tier2_equivalence`), `services/chat_merge.rs`; **`services/character_archive/service.rs` `KEPT_MANAGED_PATHS` / `prune_empty_folders` (`character_archive_tier2_equivalence`) — the LIVE-DATA hazard**; `services/backup/uuid_remap.rs` `remap_link_id` + `restore/orchestrator.rs` (a step-22f-quater twin; `RESTORE_TS_MESSAGES` both sides); `services/llm_logging.rs` `IMAGE_SPEND_LOG_TYPES`, `almanack/phase6_wire_records.rs`; `wardrobe_folders.rs`, `vault_overlay.rs`, `db/vault_wardrobe_write.rs`, `services/wardrobe_image_bridge.rs` / `wardrobe_item_images/` (the generic lift — `wardrobe_folders_equivalence`, `wardrobe_folder_writes_tier2_equivalence`, `vault_projection_recursive_tier2_equivalence`); `services/outfit_change_effects.rs`. **PORT-NEW homes:** a new `inventory` module; `db/doc_mount_documents.rs`; `services/build_context.rs` (`build_context_tier3_equivalence`), `carina_query.rs`, `initial_greeting.rs`, `avatar_prompt.rs` / `character_avatar_job.rs`, `story_background_job.rs`, `tools/generate_image.rs`, `services/pascal_writer.rs`; `crates/quilltap-web` inventory / transfers / images routes + the chat actions, each with `COLLAPSE_CENSUS` rows. **SPA:** `apps/web/src/app/wardrobe/*` (the lift), `workspace/core/{tab-meta,reducer}.ts`, `core/realtime-topic-map.ts`, `chat/system-message-labels.ts`, `chat/sidebar/participant-card.ts`, `screens/prospero/project-detail.ts`. Help → `p4.9i2` (131 → 133). Tools: none. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER after `e8e17b2d9` (extends its keyed-field list). Keystone of the Inventory round. |
| `e1872c94f` | 2026-10-18 | Feature: Inventory tools and Pascal inventory support (#102) | **PORT-NEW WITH THE FOURTEENTH D23 MOVE (`characters."canManageInventory"`, nullable, one additive migration; NOT destructive) + NINE model-visible tools + the Pascal inventory grammar + PORT on ported Pascal / tool-site / Almanack / realtime surfaces** (classified 2026-10-18 from the hunks; 131 files, +7,933 / −341; `4.11.0-dev.33` → `dev.35`; squash of #102; parent `4fbdad186`). **Schema:** `CharacterSchema.canManageInventory: z.boolean().nullable().optional()` after `canCreateOutfits` → generateDDL `"canManageInventory" INTEGER DEFAULT NULL` before `characterDocumentMountPointId`; `add-character-inventory-flag-v1` (`dependsOn add-character-wardrobe-flags-v1`, column-gated, appends; INFO `Added canManageInventory column to characters` / ERROR `Failed to add canManageInventory column to characters`; registered LAST after `addChatEquippedInventoryMigration`, verified; prettify `Granting characters leave to rummage through their own bags`); the characters PUT key; `qtap-export.schema.json` character property; `ChatsRepository.findChatIdsCarryingInventoryId` / `removeEquippedInventoryId`. `KNOWN_TOP_LEVEL_KEYS` unchanged. **Tools:** `inventory_list` (`equipped?`, `kind?`, `folder?`, `tier?`, `include_archived?`), `_read` (`ref`), `_equip` / `_unequip` (`refs` 1–8), `_pick_up` (`ref`, `title?`, `equip?` default true), `_create` (`name`, `title?`, `description?` ≤4000, `image_description?` ≤2000, `emoji?` ≤16, `max_equipped?` 1–999, `metadata?`, `equip?`), `_update` (refine `Name at least one field to change`), `_archive` (`ref`, `archived`), `_give` (`ref`, `to`); ONE `inventory-tool-flags.ts` (on unless `canManageInventory === false`); OFF for Brahma, the Scenario Builder, Carina and Help chat (a NEW positional `canManageInventory` in `buildTools` after `canCreateOutfits`); listed after `wardrobe_archive` in the plugin builder, `BUILT_IN_TOOLS` + ONE `INVENTORY_TOOL_TABLE` (no character → `The inventory tools need a character to act for.`), `GET /api/v1/tools` category `inventory` (`List Inventory` … `Give Object`; unavailable `Character does not have inventory management enabled`), simple-JSON aliases (`list_inventory`, `inventory`, `read_inventory`, `equip_item`, `unequip_item`, `pick_up`, `give_item`), nine text-block tags (`[[LIST_INVENTORY /]]` … `[[GIVE_ITEM ref= to=]]`, 19 name aliases; only `archived` / `equip` / `max_equipped` coerced), `determineTextBlockToolOptions` gains an 8th argument; the snapshot +310. Failures `Inventory Error: …` (`No object "<ref>" is in your inventory or within reach. Use inventory_list to see what there is.`, the shared-store sentence, `<display> [<id>] — <state\|archetype>, <tier>`); ONE `resolveInventoryRef` (id → title → name, own before shared); `pickUpItems` checks the cap before instantiating; give moves the file FIRST, then `dropCarriedEverywhere`; turn containers gain `pendingInventoryAnnouncements`, drained by `flushPendingInventoryAnnouncements` after the wardrobe flush (moved by `102a23648`); a give posts `*X hands the Y to Z.*`. **Pascal:** `InventoryRefSchema` `{ $inventory: "<ref>.metadata.<key>", fallback }` in every operand union; a WHOLE roll may be `$inventory` (fallback must be dice — `a roll drawn from $inventory needs a dice-notation fallback, like "1d4"`); `when.inventory` / `when.inventoryCount` (templated keys) and gate `inventory` / `inventoryCount` (LITERAL keys — `a gate's inventory key must be a literal object reference; …`); `{{inventory.<ref>.title\|name\|description\|state}}` / `.metadata.<key>` placeholders; the `effects[].inventory` gesture `{ action: equip\|unequip\|pick_up, items[1–8], equip? }` after the wardrobe gestures, record `inventory.<action>`; `collectToolVocabulary` APPENDS `inventoryReads`, `equips`, `unequips`, `picksUp` after `clearsSlots` on the `references` wire; preview / audit `inventory: MockInventorySchema.nullish()`. **PORT (ported behaviour that moves):** the `RUN_CUSTOM_PREAMBLE` side-effects sentence gains `, what they are wearing, or what they carry,` in EVERY `run_custom` description; `describeRoll` / `describeWhen` / `describeEffects` arms + a `    Reads: your inventory for …` line; FIVE loader refusal sentences reworded (the two `must test something: …` variants, the gate's `must test at least one metadata key, progress field, or inventory object`, `an effect is exactly one of a write (target + value), a wardrobe gesture, or an inventory gesture`, `an effect needs target + value, wardrobe, or inventory`); the Almanack `- **May manage their inventory**: N` line after `May create outfits`; `REALTIME_TOPICS` + `inventory`. NO-PORT: the job child's overlays. Client: Aurora Profiles "Inventory Management" select, nine `ToolMessage` rows, the run dialog, Workbench `InventoryEffectFields` / picker / chips / "From an object" roll / Mock inventory. Help `custom-tools.md` +68, `inventory.md` +31, `pascals-workbench.md` +15; `qtap-custom-tool.schema.json` +313; `CUSTOM_TOOL_SPEC.json`; spec → `features/complete/`. §5.3: the message's give order ("after removeEquippedInventoryId drops it from every chat") is wrong — move first, then `dropCarriedEverywhere`. | **Schema:** `fresh_schema.json` (ONE re-dump with `4fbdad186`), a column-gated boot ensure (mirror-boot list), `db/characters.rs` / `characters_read.rs` / `api/characters.rs` / `db/vault_character_update.rs` / `quilltap_import/characters.rs` / the restore orchestrator's character field list; `db/store_name_reconcile.rs` `read_vault_owner_rows` (`SELECT *` — measure); `characters_mutations_equivalence`, `characters_slim_tier2_equivalence`. **Tools:** `tools/definitions/` (`tool_definitions_equivalence`; `TOOL_DEFINITIONS.len()` 61 → 63 with `ebb480edd` → **72**; "all 57 tools" → 57 + 2 + 9), `tools/executor.rs` + a new inventory tool table, `text_block_parser.rs` / `_prompt.rs`, `simple_json_parser.rs` / `_prompt.rs` (`pseudo_tool_parsers_equivalence`, `pseudo_tool_prompts_equivalence`), `services/tool_build.rs` (`tool_build_equivalence`, `pascal_build_tools_roster`), `services/pseudo_tool.rs`, `services/tools_inventory.rs` (`tools_inventory_equivalence`), `services/orchestrator.rs`, `services/outfit_change_effects.rs`, the four off-sites (`brahma_console/`, `scenario_builder/`, `carina_query.rs`, `help_chat/`). v5's `tools/self_inventory.rs` is a different tool — no collision. **Pascal:** `pascal/{custom_tool_types,placeholders,custom_tools,tool_gate,roster,tool_vocabulary,side_effects,run_service,trigger_runner,workbench}.rs`, `tools/run_custom.rs` — `pascal_custom_tool_definition_equivalence`, `pascal_custom_tools_execution_equivalence`, `pascal_simulate_equivalence`, `pascal_roster_equivalence`, `pascal_tool_vocabulary_equivalence`, `pascal_side_effects_equivalence`, **`pascal_run_custom(_handler)_equivalence` (the preamble moves for every roster)**, `pascal_workbench(_route)_equivalence`, `pascal_custom_tools_route_equivalence`. **Other:** `almanack/{phase3_ledgers,render,types}.rs` (`almanack_tier2_equivalence`, `almanack_render_equivalence`), `realtime/types.rs` (`realtime_topics_equivalence`, `realtime_publish_sites_guard`). **SPA:** `pascal/tool-draft.ts`, `screens/custom-tools/*`, `chat/tool-message.ts`, the run dialog, `screens/characters/view/tabs/defaults-tab.ts`, `core/realtime-topic-map.ts`, `core/core-contract.ts`. Help → `p4.9i2`; both public schemas re-vendored. | **UNPROCESSED** — past the `25b8fab6a` round's pin; ORDER after `4fbdad186` (flag + D23 ensure as a keystone, then tools ∥ Pascal ∥ SPA). |
| `102a23648` | 2026-10-18 | Fix: Inventory dogfood fixes, bugs 253/254, and the project roster gate | **PORT (bug 254 — v5 HAS IT) + PORT-NEW folded into the Inventory programme (bug 253, the roster shelf gate, dogfood fixes); NOT a convergence (253 / 254 found by v4's own Inventory dogfood; v5 status **Owed**). NO schema move** (classified 2026-10-18 from the hunks; 45 files, +850 / −284; `4.11.0-dev.35` → `dev.36`; parent `e1872c94f`). **Bug 254 (`lib/chat/context-manager.ts`):** `buildProgressionsSection` was built only when `!isContinueMode` — v4's ORIGINAL guard, ported faithfully — and is now built on EVERY turn (Continue, nudge, every chained speaker after the first, every autonomous-room turn), riding the trailing-only user message; the `isContinueMode` doc comment is rewritten; the Core whisper keeps its skip. Model-visible: the progressions report now reaches chained / nudged / autonomous turns on each progression's cadence. `PROMPT_ARCHITECTURE.md` + `features/complete/character-progressions.md` reworded. **Bug 253:** `buildInventorySection` ungated the same way (unported section — port it ungated). **Roster:** `lib/projects/roster-access.ts` is COMMENT-ONLY (±4 — the roster now gates "files, shared wardrobe and inventory shelf"); `projectRosterAdmits` and the policy UNCHANGED. NEW `lib/inventory/roster-gate.ts` (`isCharacterGesture` — provenance `!== 'operator'`; `projectShelfOpenTo` — a throw means CLOSED, WARN `[InventoryRoster] Roster lookup failed; the project shelf stays closed`; `isOnClosedShelf`); consumers Inventory-only (`loadInventoryContext` DEBUG `projectShelf: 'none' \| 'open' \| 'closed by roster'`; `assertShelfReach` before the cap in `equipItems` / `pickUpItems`); refusal `OFF_ROSTER` / 409 — model-visible `"<name>"[, …] is\|are on the project's shelf, and you are not on this project's character roster. Ask the user to add you to the roster in the project's Characters card.`; INFO `[InventoryEquip] Refused`. **Inventory fixes:** the inventory flush moves into a `try … finally` after every save branch (Pascal's line now posts AFTER the saved turn; the wardrobe flush unchanged, still before); `equip-effects.ts` `looksPlural` (`some <name>`) and `indefiniteArticle` (`/^(uni\|use\|usu\|uti\|eu\|one\|once)/i` → `a`); the pool's project-tier origin `name` from `projects.findById` (fallback `'Project'`, WARN `[InventoryPool] Project name lookup failed`); SPA fixes; the Workbench `WardrobeVantageSelect` `browsing` prop (default `'wardrobe'` — wording unchanged). Help `inventory.md`, `project-inventory.md`, `project-characters.md` +24, `projects.md` ±4; `bugs.md` + two `bugs/fixed/` files. §5.3 / unmeasured: "arrival defaults are ungated" holds only if arrival uses `operator` provenance or bypasses `assertShelfReach` — measure at port time. | **Bug 254:** `crates/quilltap-core/src/services/build_context.rs` (~l. 2907–2943 — drop the `is_continue_mode` guard and its "finishing its own sentence" comment; fix the "NO continue-mode gate, unlike progressions above" comment and the `BuildContextInput.is_continue_mode` doc); **`build_context_tier3_equivalence` op `progressions_continue_mode` (`harness/oracle/fixtures/build-context-tier3.json`) PINS the skip today and flips red on the regen** (verified 2026-10-18); measure `orchestrator_tier3`, `regenerate_swipe_tier3` and the enclave autonomous tier on chained legs. `trailing_context_sections` already lists progressions. **Roster:** `crates/quilltap-core/src/project_roster_access.rs` header ("seven sites") gains the inventory sites when the Inventory ports; `project_roster_access_equivalence` does NOT move. **Everything else** lands with the unported Inventory programme. Help (`help_tree_equivalence`): `project-characters.md`, `projects.md` re-vendored WITH the Inventory (they link `project-inventory.md`); `docs/v4/developer/` `PROMPT_ARCHITECTURE.md`, `character-progressions.md`, `bugs.md` + the two fixed-bug files. | **UNPROCESSED** — past the `25b8fab6a` round's pin. Bug 254 is a one-guard PORT that can ride an earlier round; the rest ORDER with the Inventory programme. |

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
