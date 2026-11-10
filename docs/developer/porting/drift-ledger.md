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

- **Oracle baseline: `624d0d25c`**: "ci: restore the 8 GB heap for the
  coverage run" (v4 main, 2026-11-04 11:00, `package.json`
  `4.13.0-dev.8`). Its code contains v4's 4.12.0 and 4.12.1 releases. It
  was adopted when the `624d0d25c` 4.12.x catch-up round was unified
  (2026-11-09). All twenty-eight lanes landed: P4.D361 → P4.D362 →
  {P4.D369 → {P4.D370 ∥ P4.D371}} ∥ … ∥ P4.D388 (picked D361 → D365 →
  D382 → D362 → D363 → D364 → D367 → D369 → D370 → D371 → D372 → D373 →
  D374 → D375 → D366 → D368 → D376 → D377 → D378 → D379 → D380 → D381 →
  D383 → D384 → D385 → D388 → D386 → D387). The round was pinned at the
  same commit, so the baseline equals the pin. All seventy-four §3 rows
  `b15fe9404` + `8832cfd20` … `624d0d25c` retired to §6 — `b15fe9404`
  whole at last (P4.D368 landed P4.D343 items 41 and 29). The previous
  baseline was `236c04e35` (2026-11-04 → 2026-11-09).
- **Checked:** 2026-11-10 (morning), by `/driftcheck` on v5 main
  (`07f32dded`, the `624d0d25c` round's unification commit; tree clean).
  NOT fetched; local refs read as they stand.
  - The §2 probe FAILED on **3 new `main` commits** past the 2026-11-09
    tip `2f0a033ca`: `bb875c6f2` (`dev.57`), `ac6b60c21` (`dev.58`),
    `0d73a4aee` (`dev.59`). `bugfix` (`d67c70a51`) and `release`
    (`c9b281ec9`) did NOT move. No new bug issues past #155 (`gh issue
    list`, 2026-11-10); none of the three cites `Fixes #N`.
  - The facts below for the 45 earlier commits are the 2026-11-09
    (afternoon) check's, on v5 main `a11fbe767` during the round, re-read
    at its unification (same tips, tree clean).
  - The §2 probe FAILED on **45 new `main` commits** past `624d0d25c`
    (`4089469a8` … `2f0a033ca`, `4.13.0-dev.12` → `dev.56`), a moved
    `bugfix` (`997657639` → `d67c70a51`, 11 commits) and a moved `release`
    (`6121f59b9` → `c9b281ec9`).
  - **v4 RELEASED 4.12.2** (`c9b281ec9`, tag `4.12.2`, 2026-11-06 13:11;
    sole parent `6121f59b9`, i.e. 4.12.1). Its content is nine `bugfix`
    cherry-picks of `main` commits — `8dfe307f5`, `99391975f`,
    `f5d37e718`, `b9b9621e8`, `c874ae9ba`, `4b97636b2`, `0ae0ba3a7`,
    `4d3b00621` — plus the Gemini-arrival half of `403d60081`
    (`35d0bd803`) and the Wolfi Dockerfiles (`f1037ba27`). `452e61ea9`
    ("merge: 4.12.2 back into main") brings it onto `main`. Measured by
    CONTENT: every line 4.12.2 added under `lib/` / `app/` / `packages/` /
    `plugins/` is on `main` HEAD, except three hunks later `main` commits
    superseded (the Anthropic breakpoint block and the OpenRouter header /
    `user` lines, both rewritten by `819fc9c82`; two
    `character-optimizer.service.ts` message lines reshaped by `4089469a8`
    / `c874ae9ba`). **No unabsorbed release content.**
  - `gh issue list -R foundry-9/quilltap-server --label bug --state all`:
    NEW issues **#145–#155** (all CLOSED). Twelve of the 45 cite them with
    `Fixes #N`: `f5d37e718` #145 (spec `5f8c45966`); `7624332c0` #146;
    `88c9cc81d` #147 (test `5f0bb6b0b`); `1dc11e947` #148 / #149 / #150
    (spec `3277aa572`); `348172b9c` #151–#154 (spec `d281c2332`);
    `921fe94bb` #155. **#147 and #155 were FILED BY THIS PORT** (P4.D381
    R-6, P4.D378 `CONSULTED_CROSS_TIER_INTERLEAVE`); the rest are v4
    code-reading or CI finds.
- **v4 `main` HEAD at check: `0d73a4aee`** ("fix: Wardrobe History's Wear
  again stages until Done", 2026-11-10 01:04 −0600, `4.13.0-dev.59`) —
  **FORTY-EIGHT commits past the baseline `624d0d25c`, all UNPROCESSED in
  §3.** The 2026-11-09 tip was `2f0a033ca` ("feat: opaque characters may
  use their own vault's non-core folders", 13:13 −0600, `dev.56`) —
  forty-five past the baseline (118 past the old baseline `236c04e35`; the 73 in
  between are this round's, now absorbed). `4089469a8` (the Optimizer's
  "Refine from Memories & Instructions", committed during the round's
  planning and held out of it by ruling R-B) is now classified among the
  45.
- **v4 `bugfix` tip at check: `d67c70a51`** ("bugfix: started 4.12.2 bug
  branch", 2026-11-06 13:12). Since `997657639` it carried the nine
  cherry-picks above and its own stamp. By CONTENT, `git diff c9b281ec9
  bugfix` touches 3 files (+4 / −4: `README.md`, `package.json`,
  `package-lock.json` — the bug-branch stamps only). **NO unabsorbed
  bugfix content.** The §2 probe's bugfix base is `d67c70a51`.
  **`release` tip:** `c9b281ec9` (`release: 4.12.2`). Re-probe ALL THREE
  branches every time.
- **Checkout at check:** branch **`main`** at `0d73a4aee`, tree **CLEAN**.
  Its code is `4.13.0-dev.59`, forty-eight commits past the baseline, so a
  regen from it is poisoned by DRIFT.
- **Verdict: DRIFT PENDING — 48 commits past the baseline `624d0d25c`,
  all UNPROCESSED (§3).** By leading class: 2 CONVERGENCE, 21 PORT-led, 13
  PORT-NEW-led, 12 NO-PORT? candidates.
  - **The three 2026-11-10 commits** (no migration, no DDL, no `lib/`
    server hunk): `bb875c6f2` the `:` / `\` typeaheads in every Markdown
    editor (SPA PORT); `ac6b60c21` the dependency sweep — **new provider SDK
    floors** `openai` 7.54.0, `@anthropic-ai/sdk` 1.7.0, `@openrouter/sdk`
    1.5.73, plugin-utils 2.7.4 (plugins) / 2.7.5 (package) — a PORT for the
    recorded request corpora at the next pin, as P4.D364 was; `0d73a4aee`
    v4 FIXES the bug behind this port's **R-L** ruled divergence (P4.D318,
    2026-10-19) in a DIFFERENT shape — Wear again now STAGES the rollback
    until Done instead of v5's discard-then-post — so R-L retires and v5
    re-ports v4's shape (a CONVERGENCE-of-intent row; §5.4).
  - **THREE v4 migrations**, all additive (one NULL backfill; none
    destructive): `add-profile-tool-result-verbatim-turns-v1`
    (`9d68d8421`), `add-chat-avatar-timeline-table-v1` (`ebb0ca13a`, a NEW
    TABLE), `add-chat-opening-state-v1` (`348172b9c`, three
    migration-only `chats` columns). Suggested D23 numbering 24th / 25th /
    26th (Schema / D23 below; `/setupphase` confirms).
  - **Two probable FRESH-ONLY generateDDL moves on `characters`** —
    `19e45d664` (`birthday`) and `1dc11e947` (the repository re-keyed onto
    `CharacterRowSchema`: a fresh v4 `characters` table LOSES the 19
    vault-managed columns, `kind` among them) — to be MEASURED by re-dump.
  - **ONE convergence: `921fe94bb` (#155)** — v4 adopts this port's
    `CONSULTED_CROSS_TIER_INTERLEAVE` ruling (P4.D378, RULED 2026-11-09):
    `consulted` (and now `unread`) concatenated in tier order own → group
    → project → General. The round's unifier already NARROWED the carves to
    named two-way rows (`5c3930619`); they trip at the next baseline move
    by design — retire by measurement (§5.4), checking v5's
    `unread_mounts` order too (the ruling covered `consulted`).
    `88c9cc81d` (#147) is NOT a convergence: v5 ported v4's case-sensitive
    folder move FAITHFULLY, so it is a plain PORT of v4's fix.
  - **Programmes:** the Green Room / opening refactor (`0ae0ba3a7`,
    `4d3b00621`, `89f259c2f`, `3db965416`, `348172b9c`, `309741661`,
    `dca8ff120`); the character repository / vault refactor (`1dc11e947`,
    `c135a5561`); Aurora's one character page + autosave (`7624332c0`,
    `3a79f744c`); birthdays / public identity (`19e45d664`); the Salon
    avatar timeline (`ebb0ca13a`); Turns mode (`b941280d5`); edit any Salon
    line (`8d5eb8f0e`); the Foundry settings rebuild (`f1386cebd`); the
    Optimizer (`4089469a8`, `c874ae9ba`); provider prompt-cache fixes across
    five plugins (`8dfe307f5`, `b9b9621e8`, `4b97636b2` — which REMOVES
    `previousResponseId` chaining from the spine — `f5d37e718`,
    `78aca159d`, `819fc9c82`); the per-profile tool-result elision window
    (`9d68d8421`); Brahma turn caps (`3cc12d346`, default 50 → 10); the
    wardrobe image-prompt optimizer (`fee1bd63d`, `aefbdde05`); opaque
    characters' own-vault reach (`2f0a033ca`).
- **Regen rule: PIN REQUIRED at `624d0d25c`** (§5.1) — HEAD is past the
  baseline. Never regen from the live checkout: it is forty-eight commits
  past it. (Unchanged from 2026-11-09.)
  - The pin's root takes its own `npm ci` (the unifier's pin
    `/tmp/qt-v4-pin-unify624-624d0d25c` did); its lockfile carries the
    13.0.3 driver (SQLite3MC 2.4.0 / SQLite 3.53.4), which now matches v5's
    vendored amalgamation.
  - The pin's plugin + CLI dirs take the floor install
    (`.claude/pin-plugin-sdks.sh <PIN> <COMMIT>`, the 2026-11-02 rulings),
    never live symlinks; the script refuses a pin not at `<COMMIT>` and
    retries a cache miss once with `--prefer-offline`. The floors at this
    baseline are P4.D364's (`openai` 7.47.0, `@anthropic-ai/sdk` 1.4.0,
    `@google/genai` 2.32.0, `@openrouter/sdk` 1.5.34, plugin-utils 2.7.3).
  - ⚠ **`provider_sdk_version_guard` is an env artifact at every gate**
    until the next pin: it reads the LIVE v4 install, now (read
    2026-11-10, after `ac6b60c21`) root `openai` 7.54.0 / `@openrouter/sdk`
    1.5.73 / plugin-utils 2.7.4, the plugins' `@anthropic-ai/sdk` 1.7.0,
    `openai` 7.54.0, `@openrouter/sdk` 1.5.73, `@google/genai` 2.32.0
    (unmoved) — past the pin's 7.47.0 / 1.4.0 / 1.5.34 / 2.7.3. (Past the
    baseline, `99391975f` / `f5d37e718` had pinned the OpenRouter
    **plugin's** `@openrouter/sdk` to 1.5.56; it is `^1.5.73` since
    `ac6b60c21`.) Re-read the live versions and re-check the printed floors
    against the guard before the next pin set-up.
  - `QT_V4_CHECKOUT` = `QT_V4_ROOT` = the pin.

### ⚠ LIVE-DATA — what stands at this baseline

- **A v5 at `624d0d25c` reads and writes** (on top of the `236c04e35`
  state — keystones, Storyteller, intervention, the Novel template, the
  Log carrier, terminals; see §6 and the round records):
  - `chat_messages.isEstablishedFact` (the 22nd move) and
    `connection_profiles.maxImagesPerRequest` (the 23rd, + its NULL → 4
    backfill) — both UNSTAMPED, column-gated boot ensures;
  - `strip-anthropic-sampling-parameters-v1` as a STAMPED data heal, with
    v4's repository-side sampling STRIP on every profile write (P4.D362) —
    **the sampling-key ping-pong RETIRES**; the Anthropic builder sends no
    `temperature` / `top_p` under `anthropic-version` `2026-10-21`
    (P4.D364);
  - `reindex-chat-message-fts-character-only-v1` as a STAMPED boot ensure
    with v4's byte-identical trigger text (P4.D363) — **the FTS reconcile
    ping-pong RETIRES** (a v5 boot no longer re-indexes Staff rows);
  - the vault / settings keys `properties.json` `kind` (P4.D369),
    `brahmaConnectionProfileId` and `brahmaConsole.inlineConnectionProfileId`
    (P4.D374 ports their reader / writer). That a v5 write no longer erases
    a v4-written value is to MEASURE at the dogfood pass.
  - The first v5 boot of a pre-4.12 copy ADDS the two columns, runs the
    sampling heal and the FTS reindex (both stamped); a copy v4 already
    migrated sees NO-OP ensures. Measure both at the dogfood pass.
- **The cipher build moved:** v5 now links SQLite3MC **2.4.0** / SQLite
  **3.53.4** (`quilltap-sqlite3mc-sys` 0.2.0, P4.D365, ruling R-A), the same
  as v4's 13.0.3 driver; `defines.gypi` converged (STAT4 and DQS included)
  except `SQLITE_DEFAULT_FOREIGN_KEYS`, HELD at the sys crate
  (`SQLITE_DEFAULT_FK_HELD`; the CLI's writable open now enforces foreign
  keys itself, `94a60996b`). ⚠ **The real-instance open on 2.4.0 is NOT
  verified** (`REAL_INSTANCE_OPEN_AWAITS_DOGFOOD`) — owed to the next
  dogfood pass on a Friday copy.
- **The general rule BITES:** a v5 build from before this round must not
  open an instance this round's ensures have altered, and a pre-round v5
  build (2.3.5) should not write a file the union build (2.4.0) touched.
  The owed dogfood pass runs with the UNION build.
- **Past `624d0d25c` — NO destructive migration** (all three are additive),
  but measure these before any v5 build opens a copy v4 ran past the
  baseline:
  - **(a) `1dc11e947` stamps frontmatter `id:` into `Prompts/*.md` /
    `Scenarios/*.md` on the next write.** Until a file is renamed the
    stamped id equals v5's path-derived id; after a v4 rename, v4 keeps the
    stamped id while v5 (`stable_uuid` in `scenarios/location.rs` /
    `vault_overlay.rs`) mints a new one, so `defaultScenarioId` /
    `defaultSystemPromptId` resolve differently. Also measure whether a v5
    frontmatter rewrite (`db/scenarios.rs`) preserves the `id:` key.
  - **(b) `348172b9c`: a chat v4 created but has not opened sits at
    `openingState 'pending'`.** v5 has no opening runner, so opening it in
    v5 shows no greeting; v4 would later resume it, and a v5 turn may land
    first.
  - **(c) `ebb0ca13a`'s `chat_avatar_timeline`:** a v5 backup / `.qtap`
    export of a v4-migrated instance omits `data/avatar-timeline.json` /
    `avatar_timeline`, so a v5 restore drops its rows; a v5 character
    delete leaves orphan timeline rows (v4 calls `deleteByCharacter`; chat
    deletes cascade by FK).
  - **(d) `19e45d664`:** `properties.json` `birthday` should survive v5's
    read-modify-write (`vault_character_update.rs` preserves untouched
    keys) and progressions' `public` key in `metadata.json` should survive
    v5's progression write — measure both.
  - **(e) `1dc11e947`'s boot heal no longer repopulates a vault from the
    raw row; v5 at the baseline still does** — a v5-side divergence, not a
    hazard to Friday.
  - **(f) `9d68d8421`:** a v5 profile write that omits
    `toolResultVerbatimTurns` is legal (the column has `DEFAULT 3`); v5
    keeps eliding at its literal 3.
  - **(g) `3cc12d346`:** v5 still defaults Brahma `maxAgentTurns` to 50
    where v4 now defaults to 10 (a stored 5–200 value parses on both).
  - None of the 45 rewrites existing rows the way `544c7ab1e` or the
    memory refactor did.
- **Standing:** no v5 build against live Friday — dogfood a COPY, and do
  not round-trip v4-written characters through v5 writes while comparing
  with v4. Live v4 has migrated Friday past the baseline (4.12.x and
  beyond); no earlier backup exists, so the dogfood pass waits until v5
  catches up with the v4 state Friday is at (the 2026-11-04 ruling). The
  extraction rate ledger is per-process; the P4.D274 store-name plan probe
  on a copy stands.

### Schema / D23

- **LANDED this round: the TWENTY-SECOND and TWENTY-THIRD moves in ONE
  `fresh_schema.json` re-dump** (P4.D362, the `SCHEMA` commit
  `4e3a1584e`): `chat_messages."isEstablishedFact"` (generateDDL's shape
  between `isSilentMessage` and `systemSender`; the migration's `INTEGER
  DEFAULT NULL` appended on an existing table) and
  `connection_profiles."maxImagesPerRequest"` (`INTEGER DEFAULT 4` between
  `supportsImageUpload` and `tags`; the migration appends + backfills NULL
  → 4). The same dump carries the two FRESH-ONLY moves the ledger had
  missed — `characters."kind" TEXT DEFAULT 'roleplay'` (`b09ca46bb`) and
  `projects."brahmaConnectionProfileId" TEXT` (`c539c82e0`), no ensure, no
  SQL writer. `migration_indexes.json` re-dumped (`source.v4Commit` only);
  three boot ensures in v4's registration order through ONE chain (host,
  vintage mirror-boot, test twin); `schema-key-order.json` dumped,
  `qtap-export.schema.json` re-vendored whole. P4.D363 re-homed the FTS
  reindex as a stamped ensure (not a dump).
- **PENDING past the baseline — suggested 24th to 26th moves** (`/setupphase`
  confirms the numbers):
  - **24th (`9d68d8421`, `add-profile-tool-result-verbatim-turns-v1`):**
    `connection_profiles."toolResultVerbatimTurns" INTEGER DEFAULT 3` — a
    Zod field, so generateDDL renders it (position to MEASURE at re-dump);
    the migration is additive with a no-op NULL → 3 backfill. A boot
    ensure.
  - **25th (`ebb0ca13a`, `add-chat-avatar-timeline-table-v1`):** the NEW
    TABLE `chat_avatar_timeline` + `idx_chat_avatar_timeline_chat` —
    MIGRATION-ONLY (its own DDL file, not a Zod collection; the
    `chat_wardrobe_history` class), so per the 2026-10-19 ruling a boot
    ensure, not a dumped artifact; add it to `restore_vintage_state`'s
    boot-align list.
  - **26th (`348172b9c`, `add-chat-opening-state-v1`):** MIGRATION-ONLY
    `chats."openingState" TEXT NOT NULL DEFAULT 'done'`, `"openingPhase"`,
    `"openingPlan"` (both `TEXT DEFAULT NULL`; not in `ChatSchema`, not
    exported). A boot ensure.
  - **Probable FRESH-ONLY moves on `characters` — MEASURE with
    `harness/oracle/provision/dump-fresh-schema.ts` at the next pin:**
    `19e45d664` adds `birthday`; `1dc11e947` re-keys onto
    `CharacterRowSchema`, so a fresh table loses all 19 managed columns
    (including the `kind` this round just dumped). No migration; v4 never
    calls `generateAlterStatements`. ⚠ Census v5's raw `characters` reads
    at the re-dump: a SELECT of a managed column by name fails on a
    v4-fresh instance — and under DQS a double-quoted missing column could
    read as a string literal (memory note `sqlite-dqs-build-difference`;
    P4.D365 reports DQS converged — re-measure).
  - **Not D23:** `89f259c2f`'s `outfitSelectionProfileId` (a JSON key in
    `chat_settings."cheapLLMSettings"`); `3cc12d346`'s `maxAgentTurns`
    (JSON in `instance_settings`, default only); `b941280d5`'s `'turns'`
    (a new value in the bare-`TEXT` `turnMode`); `3a79f744c`'s retired
    physical-description keys (never stored in the vault).

### Regen, dependencies, help

- **Dependencies at the baseline:** the pin's SDK floors above (P4.D364's
  full re-record, ruling R-F; ruled ports X1 genai 2.32.0 `inlineData`
  order, X2 Anthropic `Authorization: Bearer`, X3 OpenRouter 1.5.34
  image-model errors); `zod` 4.6.7; the v13 driver; croner 11 (P4.D365,
  reclassified PORT); `mermaid` 12.3.0, `@hpcc-js/wasm-graphviz` 1.30.0,
  `dompurify` 3.4.17 in the SPA; `mammoth`'s `.docx` → HTML is a NAMED
  refusal in v5 (`render-html` 422, ruling R-E); v5's `vt100` /
  `regress` twins unchanged.
- **Past the baseline:** the OpenRouter plugin pins `@openrouter/sdk` 1.5.56
  (`99391975f`); five provider plugins re-shape their cache placement and
  usage readers (`8dfe307f5`, `b9b9621e8`, `4b97636b2`, `f5d37e718`,
  `78aca159d`, `819fc9c82`) — the round's full re-record moves again at the
  next pin. No root dependency move was recorded among the 45; re-read the
  live root versions at the next pin set-up.
- **IMPORT-DEAD:** the round's stage-0 re-aim (P4.D361) cleared the
  4.12.0 set. For the next catch-up, grep `harness/oracle` for every export
  the 45 delete — at least `lib/chat/initial-greeting.ts`,
  `first-message-context.ts`, `initialize.ts`, `creation-progress.ts`
  (DELETED by `348172b9c`), the `vault-overlay/` tree (→
  `lib/characters/vault/*`, `1dc11e947`), `findPreviousResponseId`
  (`4b97636b2`), and `PhysicalDescriptionSchema`'s `name` /
  `usageContext` (`3a79f744c`).
- **Help / docs:** `help/` is **138 files** (+ `deus-ex-machina.md`),
  md5-identical to the pin; `docs/v4/` **537** (P4.D382, 51 operations,
  incl. the vendored CLI README). Past the baseline, `7624332c0` (~30
  pages), `f1386cebd` (~75 pages, settings links rewritten) and a dozen
  smaller commits move `help/`; the spec docs `5f8c45966`, `3277aa572`,
  `d281c2332`, `523503e8a` add `docs/v4/` paths. Re-measure at the next
  pin.
- **Themes / CSS:** v5 vendors `apps/web/public/themes/*` and
  `qt-components/*.css` at the pin's bytes (P4.D383, the WCAG 2.2 pass and
  the undefined-token fixes). Past the baseline `7ea21c151` adds four
  Madman's Box backdrops and `dca8ff120` a `_green-room.css`.

### Rounds, guards, state

- **The `624d0d25c` round — UNIFIED 2026-11-09** (the §6 bullet; the round
  record in `status-log.md`, "The `624d0d25c` 4.12.x catch-up round —
  UNIFICATION record (2026-11-09)"; `phase-4.md`'s UNIFIED block).
  - **Ratified at this move (P4.D382's NO-PORT record, §6 lists them):** 24
    whole rows `NO-PORT-RATIFIED`, and 8 rows NO-PORT in half with the PORT
    half ABSORBED by its owner (32 `NO-PORT?` rows in all; `3936fdf65`
    reclassified PORT for croner, `afec0af10` carrying 4.12.1's content).
  - **Unabsorbed v4 chokepoint caught at unify and fixed:** the overlay
    writes through `doc_mount_file_links` never published the store's
    `mountPoints` hint (a second, emit-less copy of v4 `aec626eff`'s
    `writeDatabaseDocument`) — fixed in `6f9633ac2`, the publish order
    `[mountPoints(store), topic]` in `72600dbab`.
  - **Rulings now standing:** D18 RETIRED (R-D, P4.D387 ports v4's
    `document-browser/`); `.docx` `render-html` a named refusal (R-E);
    Salon frames rendered at the sink (R-G); Storyteller "All at once" LIVE
    (P4.D368 — the 2026-11-04 disabled-until-ported ruling retires);
    `WARDROBE_VIEWER_ESCAPE_LAYERED` re-measured by P4.D387 and STANDS.
  - **⚖ RULED 2026-11-10:** `NON_BOOLEAN_CHARACTER_AGENT_DEFAULT` — KEEP the
    divergence; `put_zero_one_flag` `"0.0"` → `true` — the v4 issue is
    drafted with every other candidate by P4.190, NONE filed yet.
- **The `236c04e35` round — UNIFIED 2026-11-04.**
- **Next:**
  1. ✅ ORDERED 2026-11-10 (P4.175–P4.194, pinned at `624d0d25c`; no §3 row
     is ordered — a follow-ups round absorbs no drift; the §2 probe passed
     at planning and before the orders' commit). Was: a follow-ups smalls
     round: the round record's "Recorded, not fixed" list and the order
     headers' OPEN items (`phase-4.md`), after the two rulings above.
  2. The 45-commit catch-up (`/driftcheck` first): D23 moves 24–26 and the
     `characters` fresh-only measurement in ONE re-dump, the live-data
     hazards (a)–(g) measured first, the IMPORT-DEAD re-aims, the
     `921fe94bb` convergence retired by measurement, a fresh pin's floors
     against `provider_sdk_version_guard`.
  3. The owed dogfood pass once v5 reaches the v4 state the Friday copy is
     at — including the real-instance open on SQLite3MC 2.4.0.

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
| `4089469a8` | 2026-11-04 | feat: Refine from Memories & Instructions | **PORT (the Character Optimizer's request + prompts) — supersedes the UNCLASSIFIED row** (19 files +618 / −140; `dev.12`). `app/api/v1/characters/[id]/handlers/post.ts`: `optimizeStreamSchema` gains `useMemories` (default `true`) + `instructions` (`max(OPTIMIZER_INSTRUCTIONS_MAX_LENGTH)`, default `''`) + a `.refine` "A refinement needs memories to consult, instructions to follow, or both." (path `instructions`); new INFO keys `useMemories` / `instructionsLength`. NEW `lib/characters/optimizer-limits.ts`. `lib/services/character-optimizer.service.ts` +219 / −86 (instructions in the shared context, outranking memories; continue on instructions alone when too few reinforced memories; analysis prompt / progress / evidence labels / vault suggestions file by source). SPA: the optimizer modal's textarea + "Consult the Commonplace Book" checkbox. `help/character-optimizer.md`. | **v5:** the `p4.9k` generators — the Optimizer (`quilltap-host/src/generators_detail_driver.rs`, core's optimizer service and its tier-1/3 families, `kind_directions_order` / `trust_directions_order` tests), the round's P4.D371 (the Optimizer's flattening drift signal) moves the same file; the SPA optimizer dialog; `help/` vendor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `8dfe307f5` | 2026-11-04 | fix: OpenAI model fetch lists GPT-6 and later generations | **PORT (OpenAI provider: model-list filter, reasoning / 24h-retention family rules, one 400 retry)** (8 files +350 / −51; `dev.12`; in 4.12.2 as `27f607a2a`). NEW `plugins/dist/qtap-plugin-openai/model-families.ts` (chat = gpt-4+ and every o-series, gpt-3.5 out; reasoning = gpt-5+ and o-series; 24h retention = gpt-5.1/5.2/5.5 and gpt-6+ except `-chat-latest`); `provider.ts`: a `createResponse` wrapper retries ONCE without `prompt_cache_retention` on a 400 naming it (all four request paths); plugin 1.0.69. | **v5:** the provider layer — OpenAI's declarative manifest / model listing and the Responses request builder (`model/request_builder/responses_api.rs`, `provider_manifest/`); the reasoning-model and retention predicates wherever v5 hard-codes v4's old prefix lists; `help/connection-profiles.md`. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `b9b9621e8` | 2026-11-05 | fix: restore Anthropic prompt-cache hits in Brahma loops and the Salon | **PORT (one-shot / Brahma / help-chat pass the profile's params; Anthropic breakpoint placement)** (15 files +394 / −94; `dev.14`; in 4.12.2 as `a7c8ec0fe`). `lib/services/agent-loop/one-shot-loop.ts`, `brahma-console/orchestrator.service.ts`, `help-chat/orchestrator.service.ts`: `modelParams: {}` → `profileParams(connectionProfile) ?? {}`. Anthropic plugin 1.0.66: the conversation breakpoint moves to the message BEFORE the final one; a mid-history breakpoint stepped at K=15 when ≥ 20 messages; the four-breakpoint budget respected. (Re-shaped again by `819fc9c82`.) | **v5:** `services/brahma_console/orchestrator.rs`, the one-shot loop (`services/primary_stream.rs` callers), the help-chat orchestrator; the Anthropic request builder's `cache_control` placement + its tier-1 builder families (round P4.D364 re-records providers at `624d0d25c`). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `9d68d8421` | 2026-11-05 | feat: per-profile tool-result elision window | **PORT-NEW WITH A D23 MOVE (suggested 24th) — `connection_profiles."toolResultVerbatimTurns" INTEGER DEFAULT 3`, migration `add-profile-tool-result-verbatim-turns-v1` (additive; `dependsOn add-profile-max-images-per-request-v1`; `ADD COLUMN … INTEGER DEFAULT 3` then `UPDATE … SET = 3 WHERE IS NULL` — a no-op backfill after a defaulted ALTER)** (25 files +496 / −35; `dev.15`). `lib/schemas/profile.types.ts`: `toolResultVerbatimTurns: z.number().int().min(0).max(1000).default(3)` (+ `DEFAULT_…` / `…_CEILING` / `…_NEVER_ELIDE = 0`) — a schema field, so generateDDL renders it (position to MEASURE at re-dump). `lib/llm/connection-profile-utils.ts`: NEW `resolveToolResultVerbatimTurns` / `parseToolResultVerbatimTurns`. `context-builder.service.ts`: the constant 3 → the profile's window; `0` never elides; the stub text `result elided (>${n} turns old)`. Brahma's `buildConversationMessages` takes it too. Profile POST / PUT routes parse it; `qtap-export.schema.json` +6; `lib/startup/prettify.ts` +1. | **v5:** `fresh_schema.json` (re-dump) + a boot ensure for the migration; `services/message_context.rs` / the conversation builder's tool-result elision (v5's literal 3 + stub text); the Brahma orchestrator; connection-profile repo / routes / `.qtap` + backup carriers; the SPA profile editor (P4.D388 already edits that dialog); `help/connection-profiles.md`. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `c874ae9ba` | 2026-11-05 | fix: Character Optimizer prompt caching and usage logging | **PORT (Optimizer message shape + LLM-log usage)** (7 files +115 / −22; `dev.16`; in 4.12.2 as `0f47488f4`). `character-optimizer.service.ts`: the shared context and the per-pass instruction become SEPARATE user messages; optimizer log rows now carry usage, cache usage and raw provider usage. | **v5:** the Optimizer service (with `4089469a8`) and its `logLLMCall` sites (the `p4.9k` families, the llm-logs comparands). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `403d60081` | 2026-11-05 | feat: choose the vision profile in Import from image; fix Gemini arrival false positive | **PORT (bug-116 arrival ceiling) + PORT-NEW (the analyze-image profile picker)** (14 files +326 / −30; `dev.17`; its fix half in 4.12.2 as `35d0bd803`). `lib/llm/vision-request.ts`: `textOnlyTokenCeiling` = min(text/2.5, text/4 + 85). `app/api/v1/wardrobe/analyze-image/route.ts`: a NEW GET (eligible profiles + default); POST accepts `profileId` (400 when unusable). `lib/wardrobe/image-analysis.ts`; the import modal's dropdown. | **v5:** `llm/vision_request.rs` (bug 116 / 197's module — the ceiling is a pure-function tier-1 change); `analyze-image` is REFUSAL-ARMED in v5 (`quilltap-web/src/wardrobe_routes.rs`, `api/types.rs`; round ruling R-L), so the picker rides that deferral. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `4b97636b2` | 2026-11-05 | fix: OpenAI prompt-cache hits on GPT-5.6 and later | **PORT (OpenAI Responses: `previousResponseId` chaining REMOVED from the spine; cache breakpoints)** (20 files +691 / −266; `dev.18`; in 4.12.2 as `647f2a0b5`). `primary-stream.service.ts`: `findPreviousResponseId` DELETED; `orchestrator.service.ts` / `streaming.service.ts` / `provider-failover.service.ts` drop `previousResponseId`. OpenAI plugin: NEW `cache-breakpoints.ts` (206 lines), `model-families.ts` +27, `provider.ts` −93 / +43. | **v5:** `services/primary_stream.rs` / the orchestrator spine, `model/stream.rs`, `streaming_provider.rs`, `completion_provider.rs`, `request_builder/responses_api.rs` (all carry `previous_response_id`); the OpenAI builder families. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `0ae0ba3a7` | 2026-11-05 | fix: Green Room opening outfits vary from chat to chat | **PORT (the Green Room `llm_choose` outfit pass)** (15 files +427 / −17; `dev.19`; in 4.12.2 as `d4b393d0d`). NEW `lib/wardrobe/recent-openings.ts` + `chatWardrobeHistory.findRecentOpenings` (entry 0 of real arrivals, never seeded rows, excluding the chat being dressed — read-only); the outfit prompt shows the last five openings; the wardrobe list SHUFFLED per call; `CheapLLMTaskOptions.temperature` (default 0.3), outfit selection 0.8. | **v5:** `services/outfit_selections.rs` / `chat_create.rs` / `api/chat_cast.rs` and the cheap-LLM task (`cheap_llm.rs`); the `chat_wardrobe_history` repo (P4.D315/316). ⚠ The shuffle is nondeterministic — tier-3 needs a seeded / masked comparand. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `4d3b00621` | 2026-11-05 | fix: Green Room never dresses a character in two outfit sets | **PORT** (10 files +147 / −8; `dev.20`; in 4.12.2 as `9a72101e6`). NEW `lib/wardrobe/slot-ops.ts` (drop the weaker of two ensembles sharing top / bottom when either is a composite, before dissolving); one prompt line in `outfit-selection.ts`. | **v5:** `services/outfit_selections.rs` (P4.D282 / P4.D306's composite dissolve). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `89f259c2f` | 2026-11-05 | feat: Green Room wardrobe model setting; wear single pieces of outfit sets | **PORT-NEW (`cheapLLMSettings.outfitSelectionProfileId` — a JSON key inside `chat_settings."cheapLLMSettings"`, NO column, NO D23 move)** (21 files +268 / −15; `dev.21`). `settings.types.ts` `CheapLLMSettingsSchema.outfitSelectionProfileId: UUIDSchema.nullable().optional()`; `app/api/v1/settings/chat/route.ts` refuses a non-UUID ("outfitSelectionProfileId must be a profile id or null"); NEW `lib/wardrobe/green-room-model.ts` (`selectGreenRoomLLM`, stale id → cheap LLM); `lib/backup/restore/uuid-remap.ts` +1; the outfit prompt names each composite's pieces and allows one piece of a set. | **v5:** the chat-settings PUT arm + `cheapLLMSettings` validation (the F-1 CONVERGE ruling's bag), the restore uuid-remap key list, `cheap_llm.rs` selection; SPA Cheap LLM Settings card. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `99391975f` | 2026-11-05 | fix: pin OpenRouter plugin to @openrouter/sdk 1.5.56 | **NO-PORT? (plugin-dir dependency pin + rebuilt bundle)** (8 files; `dev.22`; in 4.12.2 as `a528f7946`). `plugins/dist/qtap-plugin-openrouter/package.json` pins `@openrouter/sdk` 1.5.56 (1.5.57 removed `callModel()` / chat-compat — #145); `index.js` rebuilt. | **v5:** none in code; ⚠ `provider_sdk_version_guard` / `.claude/pin-plugin-sdks.sh` floors read plugin SDK versions — re-check at the next pin. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `5f8c45966` | 2026-11-05 | docs: spec for moving the OpenRouter plugin off callModel() (#145) | **NO-PORT? — docs only, VERIFIED** (3 files +172: NEW `docs/developer/features/openrouter-plugin-chat-completions-streaming.md`). | **v5:** `docs/v4/` mirror. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `f5d37e718` | 2026-11-05 | fix: OpenRouter plugin streams every turn over Chat Completions | **PORT? (OpenRouter transport — measure against v5's own builder)** (19 files; `dev.24`; #145; in 4.12.2 as `bd05d7f26`). `plugins/dist/qtap-plugin-openrouter/provider.ts` +238 / −443: streaming (every turn shape) and image sends speak Chat Completions over a direct fetch; the SDK only for `chat.send` (text-only non-streaming) + `models.list`; `user` = the cache key. `scripts/build-plugins.ts` + NEW `scripts/lib/plugin-resolution-guard.ts` and the `__mocks__` are build / test only (NO-PORT). | **v5:** the OpenRouter manifest / request builder + stream decoder (P4.13/P4.14 `StreamMessage`; P4.D364's full re-record). If v5 already speaks Chat Completions for OpenRouter this is a byte-diff of the body, not a transport change — MEASURE. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `19e45d664` | 2026-11-05 | feat: character birthdays, public progressions, composed public identity | **PORT-NEW (birthday, `public` progressions, `public-identity.ts`) + a PROBABLE FRESH-ONLY generateDDL move (`characters."birthday"`, transient — see `1dc11e947`)** (50 files +830 / −79; `dev.25`). NEW `lib/characters/birthday.ts` (YYYY-MM-DD; age on the instance calendar, never stored) — a `properties.json` key through the vault overlay (`managed-fields.ts` / `read-overlay.ts` / `schema.ts`); `CharacterSchema.birthday` added (at this commit `CharacterSchema` still drives `ensureCollection('characters')`, so a FRESH v4 table gains `birthday`; NO migration, existing instances unchanged — v4's `generateAlterStatements` is exported, never called). `lib/progressions/schema.ts` `public: z.boolean().default(true)` (+ the Pascal writable field, `qtap-progression.schema.json`); NEW `lib/characters/public-identity.ts` (age, public progressions in fixed third-person wording, pronouns, then identity.md) used by the Host join announcement (`host-notifications/writer.ts` +55 / −30), off-scene cards (`system-prompt-builder.ts`), Carina's asker card, `self_inventory`, Brahma's `lookup_character`; `absent-mentions.ts`, `apply-chat-continuation.ts`, `turn-trailing.ts`; `qtap-export` / `qtap-custom-tool` schemas. | **v5:** the vault read / write overlay (`db/vault_character_update.rs` — its RMW preserves untouched keys, so a v4 `birthday` survives a v5 write; MEASURE), progressions (`lib.rs` progressions module + Pascal sheet), `system_prompt.rs` / `carina_query.rs` (the `public_identity` sites), `tools/brahma_lookup.rs`, `self_inventory`, the Host announcement writer; `.qtap` carriers; SPA Aurora + progression editor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `7624332c0` | 2026-11-06 | feat: one character page in Aurora; autosave everywhere | **PORT-NEW (SPA: one character page, autosave) + PORT (two small server moves)** (149 files +6,535 / −6,283; `dev.26`; Fixes #146). Server: `characters/[id]/handlers/get.ts` `?action=stats` gains `subprompts` / `mail` / `scenarioFiles` / `missingCharacterFiles` and a top-level `vault {mountPointId,name}` + a NEW DEBUG `[Characters v1] Character stats computed`; `characters/[id]/scenarios/route.ts` DELETE clears `defaultScenarioId` when it names the deleted scenario (+ DEBUG); `lib/realtime/topic-map.ts` adds `characters.stats`. Client: `app/aurora/[id]/**` rewritten (view / edit merged), `lib/autosave/autosave-engine.ts`, `lib/characters/page-sections.ts`, workspace intent / persistence / legacy redirect; #146 (the "System Prompt" field discarded on save) is a client fix. ~30 `help/` files. | **v5:** the characters `?action=stats` route + scenarios DELETE (the characters vertical), the realtime topic map; the SPA Aurora character view / edit (P4.6 characters vertical, P4.D388's kind card); workspace tab kinds; `help/` vendor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `ef5b180e7` | 2026-11-06 | docs: correct transparency storage and replace fictional Partnerships | **NO-PORT? — help text + one header string** (9 files +38 / −128: `help/character-management.md` / `-system-transparency.md` / `characters.md`; one line in `app/aurora/[id]/CharacterHeader.tsx`). | **v5:** `help/` vendor; the SPA header line (rides `7624332c0`). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `ebb0ca13a` | 2026-11-06 | feat: Salon avatars fixed to the moment they were written | **PORT-NEW WITH A D23 MOVE (suggested 25th) — a NEW TABLE `chat_avatar_timeline`, migration `add-chat-avatar-timeline-table-v1` (`dependsOn sqlite-initial-schema-v1`; `shouldRun` = table absent; additive, NO backfill)** (48 files +1,639 / −19; `dev.28`). DDL single source `lib/database/backends/sqlite/chat-avatar-timeline-ddl.ts`: `id` TEXT PK, `chatId` NOT NULL `REFERENCES chats(id) ON DELETE CASCADE`, `characterId` NOT NULL (no FK), `imageId`, `effectiveAt` NOT NULL, `source` NOT NULL (`baseline` / `generated` / `cache` / `operator` / `operator-removed`), `createdAt` NOT NULL + `idx_chat_avatar_timeline_chat (chatId, characterId, effectiveAt)`. NOT a Zod collection — a MIGRATION-ONLY table (the `chat_wardrobe_history` class; v5 ruling 2026-10-19: boot ensure). NEW repo (`record` writes a `baseline` first per pair; buffered from the job child), `lib/chat/salon-avatar.ts` (`avatarAt`) + loader; `GET /api/v1/chats/[id]?action=avatar-timeline`; `set-avatar` / `remove-avatar` record; `character-avatar` job binds record; cascade delete `deleteByCharacter`; backup `data/avatar-timeline.json` + restore remap / preview / delete; `.qtap` `avatar_timeline` records (`qtap-export.schema.json` +19); realtime on the chats topic; `method-classification.ts`. | **v5:** host boot ensures (beside `chat_wardrobe_history`'s three) + `restore_vintage_state`'s boot-align list; the avatar job (P4.1 host drivers / avatar refresh — P4.D282), `api/salon.rs` chat actions, cascade delete, the backup / restore / `.qtap` carriers (P4.D317-class); SPA Salon bubble avatar. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `3a79f744c` | 2026-11-06 | fix: character page follow-ups; retire physical description name and usage context | **PORT (physical description `name` / `usageContext` RETIRED; appearance resolution one-description; identity-stack v3) + SPA** (56 files +830 / −426; `dev.29`). `character.types.ts`: `PhysicalDescriptionSchema` drops `name` / `usageContext` (not strict — old keys stripped on read; neither was stored in the vault; NO migration). `lib/image-gen/appearance-resolution.ts` / `prompt-expansion.ts`, `cheap-llm-tasks/image-scene-tasks.ts` / `types.ts`: `selectedDescriptionId` / `physicalDescriptionName` removed; `system-prompt-builder.ts` drops the label and "(best used: …)" — `IDENTITY_STACK_BUILDER_VERSION` 3; `ai-import` / wizard / optimizer / rename / NPC drop the keys; `?action=stats` returns `stats.inventory`; `put.ts` / `post.ts`. SPA: Metadata card, ListPager, `useFixedMenu` / `useAnchorReveal`. | **v5:** the physical-description model + vault overlay (`db/vault_read_overlay.rs`), `system_prompt.rs` identity stack (the P4.D343-class golden families), the image appearance resolution + scene tasks, the generators (`p4.9k`), AI import; the SPA Aurora page. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `cc6b2dadb` | 2026-11-06 | chore: dev-tag cleanup deletes Docker Hub images by digest | **NO-PORT? — tooling, VERIFIED** (1 file: `.claude/commands/remove-old-dev-tags.md`). | **v5:** none. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `3cc12d346` | 2026-11-06 | feat: Brahma Console SQL cards collapse and interleave with thinking; turn caps | **PORT (Brahma turn budget + per-turn reasoning save; Salon agent-mode cap) + SPA** (32 files +540 / −164; `dev.31`). `settings.types.ts`: `BrahmaConsoleSettingsSchema.maxAgentTurns` `min(5).max(200).default(50)` → `min(1).default(DEFAULT_BRAHMA_MAX_AGENT_TURNS = 10)` (JSON in `instance_settings`; a stored 5–200 value still parses, the DEFAULT moves 50 → 10); `storyteller.types.ts` agent-mode `maxTurns` max 25 → 50; `brahma-console/turn-budget.ts`; `brahma-console/orchestrator.service.ts`: each tool-call turn's assistant row stores the reasoning since the last save (`unsavedReasoning`) + a NEW DEBUG `Brahma Console saved tool-call turn`; `instance-settings/index.ts`. SPA: collapsed `run_sql` cards, thinking interleave, `ThinkingBlock align`. | **v5:** `db/instance_settings.rs` / `api/settings.rs` (`max_agent_turns`), the Almanack's `turn_budget`, `services/brahma_console/orchestrator.rs` (+ its tier-3 family), agent-mode cap; the SPA Brahma console (floating console, P4.D358-era) + settings dial. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `f1037ba27` | 2026-11-06 | security: move Docker production image from Debian bookworm to Wolfi | **NO-PORT? — Docker / docs** (5 files: `Dockerfile`, `Dockerfile.ci`, `docs/DEPLOYMENT.md`, one `help/ariel.md` line; in 4.12.2 as `bc484765f`). | **v5:** none in code (v5's own P4.10 Docker packaging is independent); `help/ariel.md` vendor line. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `c9b281ec9` | 2026-11-06 | release: 4.12.2 | **NO-PORT? — release squash, VERIFIED BY CONTENT** (62 files; tag `4.12.2`; sole parent `6121f59b9`). Its code = the nine `bugfix` cherry-picks listed in §1 (each has its own `main` row) + `docs/releases/4.12.2.md`. | **v5:** none of its own — the content rows are `8dfe307f5`, `99391975f`, `f5d37e718`, `b9b9621e8`, `c874ae9ba`, `4b97636b2`, `0ae0ba3a7`, `4d3b00621`, `403d60081`, `f1037ba27`. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `452e61ea9` | 2026-11-06 | merge: 4.12.2 back into main | **NO-PORT? — merge, VERIFIED BY CONTENT** (parents `f1037ba27` + `c9b281ec9`; first-parent diff = `docs/releases/4.12.2.md` only, +48). Same topology as `afec0af10`. | **v5:** `docs/v4/` (release notes). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `78aca159d` | 2026-11-06 | fix: Z.AI sends prompt-cache key as user_id; DeepSeek logs raw cache usage | **PORT (two providers + `cache-key.ts`)** (16 files +180 / −27; `dev.35`). `lib/llm/cache-key.ts`; Z.AI plugin 1.1.35 sends `user_id` (6–128 chars; omitted outside) on send + stream instead of `user`; DeepSeek 1.0.31 keeps `rawProviderUsage` on the final stream chunk. | **v5:** the Z.AI / DeepSeek request builders + stream decoders, `cache_key` (`cheap_llm.rs`, `spine.rs`), the llm-logs usage fields. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `819fc9c82` | 2026-11-07 | fix: prompt caching for OpenRouter, NanoGPT and Anthropic plugins | **PORT (three providers' cache placement + usage readers)** (26 files +1,012 / −396; `dev.36`). OpenRouter 1.0.72: every send uses the one Chat Completions body, one usage reader incl. `cache_write_tokens`, `rawProviderUsage`, cache key also as `session_id` + `prompt_cache_key`, opt-in explicit Claude breakpoints (NEW `cache-breakpoints.ts`), all content lifted to text parts. NanoGPT 1.2.14: `cutAfterMessageIndex` before the newest user line. Anthropic 1.0.67: the history breakpoint placement revised again (supersedes `b9b9621e8`'s). `lib/llm/cache-key.ts`; `help/connection-profiles.md`. | **v5:** OpenRouter / NanoGPT (P4.D-NanoGPT, bug 269) / Anthropic builders + decoders and their recorded corpora (P4.D364's re-record is at `624d0d25c` — these move again). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `8d5eb8f0e` | 2026-11-07 | feat: edit any Salon line, with LLM-seen warning and memory regeneration | **PORT-NEW (edit impact + memory regeneration) + PORT (message PUT / GET)** (27 files +1,056 / −128; `dev.37`). `app/api/v1/messages/[id]/route.ts`: GET dispatches `?action=edit-impact` (NEW `lib/services/chat-message/message-edit-impact.ts` `assessMessageEditImpact` — later chat `llm_logs` less its own generation calls, later character lines as fallback, memories formed); PUT accepts `regenerateMemories` (retires sourced memories with reason `message-edit` via `lib/memory/retire.ts`, enqueues the source's job — the extraction turn backfill / `CARINA_MEMORY_EXTRACTION` / `ESTABLISHED_FACT_MEMORIES`; nothing retired without a profile); editing clears a stale `opaqueContent`. NEW `LLMLogsRepository.countForChatSince` (read). `chats/[id]/actions/memories.ts` → shared `resolveChatExtractionProfileId` (`lib/memory/extraction/context.ts`). `message-edit-impact.types.ts`. SPA: `MessageEditor`, the pencil on every row. | **v5:** `api/salon.rs` / `api/engine.rs` message edit (the `salon_mutations_equivalence` family), the memory retire path (P4.D297-class maintenance) + extraction enqueue (P4.D296), the llm-logs repo; SPA Salon message row. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `b941280d5` | 2026-11-07 | feat: Turns mode for the Salon; Paused renamed Manual | **PORT-NEW (a third `turnMode` value `'turns'`) — NO migration, NO DDL move** (`chats."turnMode"` stays bare `TEXT`; the Zod enum widens to `['rotation','turns','storyteller']`) (34 files +571 / −122; `dev.38`). `lib/chat/turn-manager/state.ts` +64 / −38 (`turnClosesCycle`); `turn-orchestrator.service.ts` +66 (`executeTurnChain` stops after each turn when the cycle closes, re-reads `turnMode`, chain-complete reason `turn_complete`, queues memory extraction for the round); `message-finalizer.service.ts` +34 (`ProcessMessageResult.closedCycle`); `turn-outcome.service.ts`; `lib/chat/stream-frames.ts` +1; `chats/[id]/schemas.ts` (the PATCH enum); `qtap-export.schema.json`; `chats.repository.ts` +1. SPA: four-position control, Next Round, toast; Continue's "the floor is theirs" text. | **v5:** the turn manager (Phase-1 tier-1 family), the turn chain / orchestrator (Storyteller P4.D343 / P4.D344 and the round's P4.D368 item 41 sit beside it), the finalizer, stream frames (P4.D366 / P4.D384), the turn-mode PATCH arm + `.qtap` enum; SPA sidebar control. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `3db965416` | 2026-11-08 | feat: the Green Room chooses what a character carries | **PORT-NEW (the Green Room inventory pass)** (21 files +896 / −94; `dev.39`). NEW `lib/inventory/green-room.ts` (`chooseArrivalInventory` — reachable carriable objects + kits, no archetypes / archived / closed project shelf) + NEW cheap-LLM task `inventory-selection.ts`; picks equipped leniently and silently under the chat lock; failure / timeout → the source-chat copy and the defaults; `lib/inventory/apply-defaults.ts` +76; chat create (`chats/route.ts`), `participants.ts`, `apply-chat-merge.ts` pass `choose`; `apply-outfit-selections.ts` −75 (helpers to `green-room-model.ts`). (Re-homed under `lib/chat/opening/` by `348172b9c`.) | **v5:** the Inventory programme's arrival defaults (P4.D325-class, `inventory/`), `services/outfit_selections.rs` / `chat_create.rs` / `api/chat_cast.rs`, merge + add-participant; a new cheap-LLM task family. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `fee1bd63d` | 2026-11-08 | feat: wardrobe image-prompt optimizer; import-from-image writes image prompts | **PORT-NEW (two wardrobe route actions + a cheap-LLM review) + PORT (one 200-char clamp)** (25 files +1,236 / −19; `dev.40`). `POST /api/v1/characters/[id]/wardrobe?action=optimize-image-prompts` (review; writes nothing) + `?action=apply-image-prompts` (NEW `updateItemImagePrompts` in `item-mutations.ts`) via `wardrobe-route-factory.ts` + NEW `routes/prompt-optimizer-handlers.ts` / `prompt-optimizer.ts`; `wardrobe.types.ts` request schemas (`itemIds ≤ 1000`; `changes` 1–1000, `imagePrompt ≤ 2000`); NEW `lib/wardrobe/image-prompt-limit.ts` (ONE 200-char limit / clamp — the item editor, image analysis, the optimizer); `image-analysis.ts` asks for `imagePrompt` per item. | **v5:** the wardrobe route factory (P4.D281 — one factory, four tiers), item mutations, image analysis (rides the refused `analyze-image`); SPA wardrobe dialog. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `aefbdde05` | 2026-11-08 | feat: portrait-cue wizard on every wardrobe tier; wardrobe pop-ups above the rail | **PORT-NEW (the two actions on all four tiers) + SPA** (19 files +161 / −94; `dev.41`). `wardrobe-route-factory.ts` registers `optimize-image-prompts` / `apply-image-prompts` on every tier (only a character review is filed against the character in LLM logs); `prompt-optimizer.ts` / handlers per container. SPA portals to `document.body`. | **v5:** with `fee1bd63d`. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `88c9cc81d` | 2026-11-08 | fix: folder moves match folder paths case-insensitively | **PORT (a bug THIS PORT FILED — #147 — that v5 ported FAITHFULLY; no carve to retire)** (11 files +120 / −13; `dev.42`; Fixes #147). `lib/mount-index/folder-ops.ts` `moveFolderWithinMount`: the source folder row + descendant prefixes + file links matched CASE-INSENSITIVELY (`isUnderSource` via `under()`; slice by prefix length); BOTH the database-store and filesystem branches now call NEW `sweepOpenDocuments` → `sweepChatDocumentsAfterFolderMove` (scope `document_store`) + a NEW DEBUG `Swept open documents after a store-browser folder move`. `chat-documents.repository.ts` `findUnderFolder` lower-cases prefix and path. `help/mount-points.md`. | **v5:** P4.D381 (R-6, "v5 ports all three faithfully"; draft `work-orders/surveys/2026-11-04-p4.d381-v4-folder-sweep-case-draft.md`) — v5's folder-ops move + `chat_documents` repo; the doc tools' folder sweep. ⚠ The fix is DISK-dependent (case-insensitive APFS vs case-sensitive; see `5f0bb6b0b`). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `43623cca3` | 2026-11-09 | fix: custom tooltips no longer stick open | **PORT (SPA only)** (9 files +196 / −25; `dev.43`). `components/ui/Tooltip.tsx`: document `pointermove` close-grace, outside `pointerdown` / window blur / hidden document close it, focus opens only on `:focus-visible`, touch `pointerenter` no longer opens. `help/appearance-settings.md` +4. | **v5:** the SPA tooltip directive (the tooltips vertical; P4.D385's tooltips); `help/` vendor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `5f0bb6b0b` | 2026-11-09 | test: make the #147 folder-move casing test disk-aware | **NO-PORT? — test only, VERIFIED** (2 files: `folder-ops-fs-move.test.ts` +14, CHANGELOG). On a case-sensitive disk the other-casing move is refused `SOURCE_NOT_FOUND` with no rows written. | **v5:** none in code — but it states v4's intended behaviour on Linux, which the `88c9cc81d` port's tests should pin both ways. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `3277aa572` | 2026-11-09 | docs: spec for refactoring the character repository, the vault and Aurora's writes | **NO-PORT? — docs only, VERIFIED** (NEW `docs/developer/features/refactor-character-repository-and-vault.md`, ROADMAP). Spec of record for `1dc11e947`; files #148–#150. | **v5:** `docs/v4/` (take `1dc11e947`'s `complete/` copy). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `d281c2332` | 2026-11-09 | docs: spec for refactoring the Green Room, casting and the opening | **NO-PORT? — docs only, VERIFIED** (NEW `docs/developer/features/refactor-green-room-and-opening.md`, ROADMAP). Spec of record for `348172b9c`; files #151–#154. | **v5:** `docs/v4/`. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `1dc11e947` | 2026-11-09 | refactor: character repository, vault layout table, and Aurora's writes | **PORT (the character repository / vault — behaviour on ported surfaces, three v4 bug fixes #148 / #149 / #150) + PORT-NEW (vault-health / repair-vault / patch-metadata actions; frontmatter `id:` on Prompts / Scenarios) + a PROBABLE FRESH-ONLY generateDDL move (MEASURE)** (197 files +6,244 / −4,850; `dev.47`). **Schema:** `CharacterRowSchema` (the table) / `CharacterManagedFieldsSchema` (vault) / `CharacterSchema` = row.extend(managed); `CharactersRepository extends AbstractStoreBackedRepository<CharacterRow, Character>` with `super('characters', CharacterRowSchema)` — so `ensureCollection` on a FRESH instance renders only the row columns: the 19 managed columns (`title`, `identity`, `description`, `manifesto`, `personality`, `scenarios`, `firstMessage`, `exampleDialogues`, `systemPrompts`, `metadata`, `talkativeness`, `canChooseOutfit`, `canDrawDiagrams`, `diagramStyle`, `kind`, `aliases`, `pronouns`, `birthday`, `physicalDescription`) DISAPPEAR from a fresh v4 `characters` table; `name` stays. NO migration — existing tables keep every column (not destructive). **Behaviour:** the startup `backfillCharacterVaults` NEVER writes over content (#148): seeds an ABSENT `properties.json` (defaults) / `metadata.json` (`{}`), reports a present-but-invalid one (`keystonesInvalid`, Almanack), no more repopulate-from-raw-row; clearing a physical description empties both files (#149); a whole-array scenarios PUT refused (#150); prompts / scenarios written per item (`lib/characters/vault/items.ts`), never swept when unparseable; `Prompts/*.md` / `Scenarios/*.md` frontmatter `id:` (a file without one keeps the path-derived `stableUuidFromString`, the next write STAMPS it — `lib/utils/stable-uuid.ts` `resolveFileItemId`); a keystone present-but-unparseable makes `findById` throw while list reads default it; Character API schemas derived + strict, one envelope (`lib/characters/api-schemas.ts`), new `?action=vault-health` / `repair-vault` / `patch-metadata`; `vault-overlay/` deleted → `lib/characters/vault/*` (layout table drives hydrate / write / scaffold / keystones / archive keep-set); `ensureCharacterVault` scaffolds only (`cutover-characters-to-vault.ts` adjusted). Also: `lib/api/middleware/context.ts` +11, `store-backed.repository.ts`, Almanack phase-4 lines, `self-inventory/builders.ts` +53, `character-rename.service.ts`, `brahma-sql-prompt.ts`, `memory/maintenance/run.ts`. | **v5:** the character vault read / write overlay + boot backfill (`vault_overlay.rs`, `db/vault_read_overlay.rs`, `db/vault_character_update.rs`, `db/character_vault.rs`, the host boot backfill — v5 at the pin still repopulates from the raw row), `db/scenarios.rs` + `scenarios/location.rs` (`stable_uuid` path-derived ids), the characters routes (P4.D369 kind keystone moves the same PUT / GET), Almanack, `self_inventory`, rename; `fresh_schema.json` (re-dump); SPA Aurora vault-repair card. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `c135a5561` | 2026-11-09 | fix: keep node:crypto out of the client bundle | **NO-PORT? — client-bundle module split, no behaviour** (8 files +76 / −30; `dev.48`). NEW client-safe `lib/characters/vault/paths.ts` (vault path constants) re-exported by `layout.ts`; `character-field-semantics.ts` imports from it. | **v5:** none (a Next.js bundling constraint; any constant moved is byte-identical). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `523503e8a` | 2026-11-09 | docs: spec for rebuilding the Settings page | **NO-PORT? — docs only, VERIFIED** (NEW `docs/developer/features/settings-redesign.md`, ROADMAP). Spec for `f1386cebd`. | **v5:** `docs/v4/`. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `348172b9c` | 2026-11-09 | refactor: casting, the opening, and the Green Room in the Salon | **PORT (chat creation and the greeting REWRITTEN on the Salon spine — #151–#154) + PORT-NEW (the opening state machine, `?action=open`, `?action=casting-defaults`, the `greenRoom` stream frames) WITH A D23 MOVE (suggested 26th) — three MIGRATION-ONLY `chats` columns via `add-chat-opening-state-v1` (`dependsOn sqlite-initial-schema-v1`; additive: `openingState TEXT NOT NULL DEFAULT 'done'`, `openingPhase TEXT DEFAULT NULL`, `openingPlan TEXT DEFAULT NULL`; not in `ChatSchema`, patch-only, NOT exported)** (134 files +5,927 / −9,680; `dev.50`; Fixes #151 / #152 / #153 / #154). `POST /api/v1/chats` (`chats/route.ts` −1,485 / +64) is casting only (`castChat`; `resolveCastingDefaults`, also `GET ?action=casting-defaults`), row created `openingState 'pending'`, no model call before the response. `lib/chat/opening/` NEW (`run.ts` `runOpening` over `OPENING_PHASES` resumable by `openingPhase`, `arrival.ts` one `seatArrival` for start / add-participant / merge, `cast.ts`, `concierge.ts`, `defaults.ts`, `narrator*.ts`, `staff.ts`, `schema.ts`); the greeting is a turn (purpose `'opening'`) — `lib/chat/initial-greeting.ts`, `first-message-context.ts`, `initialize.ts`, `creation-progress.ts` DELETED with the SYSTEM fossil row, the private recall copy (#153) and the greeting ladder; a scripted `firstMessage` closes via `finalizeScriptedOpener`; `chats.repository.ts` +99 (`getOpeningState` / `setOpeningState`, a stale `running` → `pending` reset); `app/api/v1/chats/creation-progress/route.ts` REMOVED; `actions/open.ts` NEW (drains a `continueMode + opening` stream; `{openingState}`); add-participant can narrate the join (`narrate`); `instrumentation.ts` +13; `autonomous-run-start.ts` +10; `brahma-console/route.ts` / `help-chats/route.ts` −11 each; `message-finalizer` +144, `orchestrator` +109, `context-builder`, `turn-trailing.ts`, `stream-frames.ts` +34; `lib/wardrobe/outfit-summary.ts`; `settings.types.ts` one line. SPA: the Green Room panel, New Chat on TanStack Query. | **v5:** the New-Chat vertical + `services/chat_create.rs` / `api/chat_cast.rs` (P4.4 unit 2 "chat creation + the Green Room"), `services/first_message_context.rs` / `initial_greeting` (both DELETED in v4), `creation_progress` in `quilltap-host/src/host.rs` / `spine.rs` + the Almanack, the Salon spine / finalizer / stream frames (P4.D366 / P4.D384 `kind`-tagged frames), add-participant + merge, the autonomous run start (enclave); host boot ensures + `restore_vintage_state` boot-align. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `dca8ff120` | 2026-11-09 | style: the Green Room panel on qt-* classes | **PORT (SPA / CSS only)** (10 files: NEW `app/styles/qt-components/_green-room.css` +170, `GreenRoomPanel.tsx`, theme-storybook CSS). | **v5:** SPA Green Room panel (with `348172b9c`). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `f1386cebd` | 2026-11-09 | feat: the Foundry rebuilt — settings by what things do, with search | **PORT-NEW (SPA: the settings page re-organised) + help text; NO server hunk** (174 files +6,232 / −1,925; `dev.52`). `lib/settings/registry.ts` (862, client — groups / cards / `resolveSettingsLink`, legacy `?tab=` links), `lib/settings/hrefs.ts`, `lib/foundry/subsystem-defaults.ts` (Brahma a subsystem), `help-guide/categories.ts`, `navigation/route-to-intent.ts`, workspace tab meta; the eight `*TabContent` files gone, ten groups in a rail, persona badges, search, attention strip, Connections → per-provider cards; `ImageProfileForm` `initialProvider`; NEW art `public/images/brahma.webp`. ~75 `help/*.md` files (settings links rewritten). `lib/tools/help-navigate-tool.ts` / `legacy/text-block-prompt.ts` one line each (help-link text — MEASURE: model-visible). | **v5:** the SPA settings vertical (P4.6c-e + the Foundry lens pages), workspace settings tab; `help/` vendor (+ `help_search` corpus) and the help-navigate tool string. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `7ea21c151` | 2026-11-09 | theme: Madman's Box backdrops for the Concierge, Pascal, Saquel and Brahma | **PORT (bundled theme assets)** (10 files: four `.webp` textures + `themes/bundled/madmans-box/theme.json` +6 / −2). | **v5:** bundled themes (the five packs P4.D357 / P4.D358 / P4.D383 carry). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `921fe94bb` | 2026-11-09 | fix: list the Inventory pool's consulted stores in tier order | **CONVERGENCE — v4 adopting THIS PORT's ruling `CONSULTED_CROSS_TIER_INTERLEAVE` (filed as #155, P4.D378, RULED 2026-11-09)** (7 files +70 / −29; prod = `lib/inventory/pool.ts` +44 / −24; `dev.54`; Fixes #155). `readTier` now returns its own `{records, consulted, unread}`; `loadInventoryPool` keeps the reads under one `Promise.all` but concatenates the bookkeeping in tier order own → group → project → General (`EMPTY_TIER`); the own tier with `opts.ownRecords` keeps `[ownMountPointId]`; `loadPoolForLocation` the same. NOTE: **`unread` is also tier-ordered now** (it was settle-ordered too); records / verdict unchanged. NEW test `pool-order.test.ts`. | **v5:** `crates/quilltap-core/src/inventory/pool.rs` + `title_lint.rs`; P4.D378's carve on `inventory_title_lint_equivalence` (Census `poolsJson` v4 `[Vault, General, Covenant]` vs v5 `[Vault, Covenant, General]`; consulted v4 `[vault, dangling, general, covenant]` vs v5 `[vault, dangling, covenant, general]`) and the printed row in `inventory_pool_order_tier2` — **both-directions pins trip at the baseline move BY DESIGN; retire by measurement (§5.4)**, and check v5's `unread_mounts` order against v4's new tier order (the ruling covered `consulted`). | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `309741661` | 2026-11-09 | feat: the Green Room panel closes and docks into the transcript | **PORT (SPA only — "Client-only")** (15 files +453 / −120; `dev.55`). `useGreenRoom` room list with docked / expanded state; RenderItem kind `'green-room'` (`insertGreenRoomItems`, `renderItemMessageIds`); CSS. `help/chats.md`. | **v5:** SPA Salon transcript (with `348172b9c`); `help/` vendor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `2f0a033ca` | 2026-11-09 | feat: opaque characters may use their own vault's non-core folders | **PORT (doc-edit tool reach for a character without `systemTransparency`)** (18 files +692 / −111; `dev.56`). NEW `lib/characters/vault/core-guard.ts` (root-level files + `Prompts` / `Wardrobe` / `Outfits` / `Inventory` / `Mail` / images folders neither readable nor writable; those folders and the root not deletable / movable); `lib/doc-edit/path-resolver.ts` `hideCharacterVaults` → `guardOwnVault` (peer vaults hidden; own vault reachable; results tagged `ownVaultGuarded`); `tools/handlers/doc-edit/{shared,text-handlers,blob-handlers}.ts` filter / refuse core paths; `read-mail-tool.ts` / `read-mail-handler.ts` one line each (model-visible). `help/character-system-transparency.md` / `document-editing-tools.md`. | **v5:** `tools/doc_edit/` path resolver (`project_roster_access.rs` / `document_ui.rs` sites), the doc-tool handlers' reach (the system-transparency gate), the read-mail tool description; `help/` vendor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-09; past the round pin `624d0d25c` (§R.1 R-B) |
| `bb875c6f2` | 2026-11-09 | feat: emoji and symbol typeaheads in every Markdown editor | **PORT (SPA only)** (11 files +33 / −10; `dev.57`). `components/markdown-editor/MarkdownLexicalEditor.tsx` +8: mounts `CharTypeaheadPlugin` for `EMOJI_PROFILE` and `UNICODE_PROFILE` (each still gated by its composer setting; `/` and `@` stay composer-only). `_surfaces.css`: `.qt-typeahead-menu` `z-50` → `z-index: 200` (above modal overlays 60–101, below toasts 1000). `help/chat-settings-composer.md` +2 (a "Not the Salon alone" paragraph). No server hunk. | **v5:** `apps/web/src/app/editor/rich-editor.ts` already carries `charTypeaheadPlugin` gated by `composerEmoji` / `composerUnicode` inputs that only the Salon composer and Document Mode pass (the composer typeahead verticals); the port is `markdown-field.ts` reading the chat settings and passing both, plus `apps/web/src/styles/qt-components/_surfaces.css:1045` (`z-50` → 200); `help/` vendor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-10 |
| `ac6b60c21` | 2026-11-09 | chore: dependency updates across root, packages and plugins | **PORT (provider SDK floors → recorded corpora) / NO-PORT for the rest** (54 files, mostly rebuilt plugin bundles; `dev.58`). Root: `openai` ^7.54.0, `@openrouter/sdk` ^1.5.73, `@quilltap/plugin-utils` ^2.7.4, next 16.4.6, TanStack Query 5.107.1, `ws` 8.22.2, patch bumps. `packages/plugin-utils` 2.7.5 (its `openai` ^7.54.0). Plugins (14 bumped + rebuilt; NO `provider.ts` source hunk — only `index.js` / `manifest.json` / `package.json`): `@anthropic-ai/sdk` ^1.4.0 → ^1.7.0 (anthropic 1.0.68), `openai` ^7.54.0 (deepseek, grok, nanogpt, openai, openai-compatible, z-ai), `@openrouter/sdk` ^1.5.60 → ^1.5.73 (openrouter 1.0.73), plugin-utils ^2.7.4 everywhere; `@google/genai` unmoved. | **v5:** `crates/quilltap-harness/tests/provider_sdk_version_guard.rs` (`RECORDED_*` 7.47.0 / 1.4.0 / 1.5.34 / 2.7.3, P4.D364) and the recorded corpora it stamps (`request-envelopes.recorded.ndjson`, `image-dialects.recorded.ndjson` — UA / `x-stainless-package-version` bytes) — re-record at the new pin and MEASURE any wire change between SDK versions (anthropic 1.4 → 1.7 is three minors). The guard is already an env artifact (§1). Next / TanStack React / storybook / `ws`: NO-PORT (v5's SPA is Angular). | UNPROCESSED — recorded by `/driftcheck` 2026-11-10 |
| `0d73a4aee` | 2026-11-10 | fix: Wardrobe History's Wear again stages until Done | **CONVERGENCE OF INTENT, DIFFERENT SHAPE — v4 fixes the bug behind this port's R-L RULED DIVERGENCE (P4.D318, ruled 2026-10-19: clean *Wear again* left v4's Live staging in place, so a later Done `set_all` undid the rollback) — + PORT (SPA only)** (14 files +408 / −139; `dev.59`; no `Fixes #N`, no issue filed). `lib/wardrobe/staged-live-outfits.ts` (client-only) +78: `rollbackTargetSlots` (the client mirror of the server's `filterStaleItems` — missing items off, archived off unless already worn in that slot), `rollbackGesture` (replaces slots outright), `standingRollbacks` (at Done, an unedited staged rollback with a baseline and no worn bundles commits via `?action=wardrobe-rollback`; anything edited since goes the ordinary `set_all`). `useStagedLiveOutfits.ts` +145 / `outfit-history-tab.tsx` / `wardrobe-control-dialog.tsx`: *Wear again* STAGES (nothing sent until Done), `stagedIndex` marks the entry with an **Unstage** button, `discard` loses its `options`. `help/wardrobe.md` one paragraph; `docs/…/complete/wardrobe-outfit-history.md`. No server hunk. | **v5:** P4.D318's History tab (`5233dc3c1` — `wearAgain` confirms only when dirty then ALWAYS discards and POSTs immediately, R-9's `rollbackPending`); the Live-tab staging service. R-L RETIRES: re-port v4's staged shape and its Done-time split (rollback vs `set_all`); retire R-L's red-first pins by measurement (§5.4). `help/` vendor. | UNPROCESSED — recorded by `/driftcheck` 2026-11-10 |

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

- **The `624d0d25c` 4.12.x catch-up round (2026-11-09, baseline `236c04e35`
  → `624d0d25c`; twenty-eight lanes P4.D361–P4.D388, nine stacking commits
  `REAIM` / `SCHEMA` / `PROVIDERS` / `KIND` / `CATALOG` / `FRAMES` /
  `REACH` / `CSS` / `BROWSER`):** all seventy-four rows `b15fe9404` +
  `8832cfd20` … `624d0d25c` retired, `b15fe9404` among them (its PARTIAL
  hold ends: P4.D368 landed P4.D343 items 41 and 29). The round was pinned
  at the new baseline. The 45 commits past it stay UNPROCESSED in §3
  (`/driftcheck`, 2026-11-09). Round record: `status-log.md` → "The
  `624d0d25c` 4.12.x catch-up round … — UNIFICATION record (2026-11-09)".
  - **`NO-PORT-RATIFIED(624d0d25c)`** — P4.D382's NO-PORT record (32
    blocks, each with its evidence), 24 whole rows:
    - `f56965ef9` (interim root SDK stamps; the guard re-pin is
      P4.D364's), `eb90eb466` (Lexical 0.53 — on the measured zero-diff
      re-record), `7dedfa067` (ESLint 10), `3e4f9dea9` (TypeScript 6).
    - `d03eb2d63`, `a299de9f5`, `f12bda3d1`, `d2795f72c`, `25a20221c`,
      `9b5bb27df`, `cbf4805c0`, `9d6f155c5`, `763dfaceb`, `7b4e1b9fa` —
      docs / specs / release notes / CHANGELOG (vendored by P4.D382 where
      mirrored).
    - `3d39c303c` (API response helpers — output unchanged, verified),
      `f1c2b178d` (a v4 test), `06efd9ca2` (the CLI README note, mirrored),
      `705b954c2` / `ec8d91667` (theme-storybook), `547f1a739` (top-level
      file cleanup).
    - `7bdb45873`, `b37769882`, `013b99b5d`, the 4.12.0 release, its
      merge-back and the 4.13.0-dev stamps; `624d0d25c`, the CI heap flag.
  - **NO-PORT in half (P4.D382), the PORT half ABSORBED by its owner:**
    `db4a76e39` (+ P4.D388, the About line; watchers NO-COUNTERPART),
    `3936fdf65` (croner → PORT, P4.D365; jest-fetch-mock / yauzl NO-PORT),
    `9f761e157` (+ P4.D364, the SDK floors), `134961572` (+ P4.D361 /
    P4.D377, the IMPORT-DEAD re-aim), `05264ace3` (+ P4.D381 verify +
    P4.D382 help; the watcher core NO-COUNTERPART), `856ed4f2b` (+
    P4.D388), `8f83041aa` (+ P4.D364 + P4.D388), `afec0af10` (4.12.1's
    content → P4.D375 + P4.D370; the heap flag NO-PORT).
  - **`ABSORBED(624d0d25c)`** (owners as ordered; P4.D382 also vendored
    every `help/` / `docs/` hunk in the span):
    - `b15fe9404` (P4.D368: items 41 + 29), Storyteller "All at once" and
      the `.qtap` import refusals.
    - `8832cfd20` (P4.D365), the v13 driver — v5's amalgamation to
      SQLite3MC 2.4.0 / SQLite 3.53.4 (R-A); the real-instance open is
      owed to dogfood.
    - `bf7a457f6` (P4.D381), the iCloud-placeholder scan.
    - `544c7ab1e` (P4.D362 + P4.D364 + P4.D388), Anthropic `2026-10-21`
      without sampling + the stamped strip heal.
    - `902c6efe9` (P4.D363), Staff rows out of the FTS index.
    - `db6190b44` (P4.D362 + P4.D372 + P4.D385 + P4.D383 + P4.D382), Deus
      Ex Machina — the TWENTY-SECOND D23 move.
    - `d439f6dc9` (P4.D362 + P4.D367 + P4.D364 + P4.D388), the per-profile
      image cap — the TWENTY-THIRD move.
    - `29c1e2690` (P4.D367 + P4.D361), the image transport budget.
    - `604a082c4` (P4.D366 + P4.D384 + P4.D361), the `kind`-tagged Salon
      frames, rendered at the sink (R-G).
    - `9eab8d2da` (P4.D366 + P4.D384 + P4.D385), gesture-only yields + the
      raised-hand chip.
    - `a3d53eae1`, `30a3b4cb7`, `4310d435f` (P4.D380), the document core,
      moves, operator actions; `5efe53ea7`, `c17033f04` (P4.D380 +
      P4.D387), Document Mode.
    - `aec626eff` (P4.D381 + P4.D387), `c34986421` (P4.D381 + P4.D387 +
      P4.D388), the document-store API and browser (D18 RETIRED, R-D;
      `.docx` `render-html` a named refusal, R-E).
    - `0c47f4539` (P4.D376 + P4.D378), the tier walk; `a0ed5ecad`
      (P4.D377 + P4.D378), the shared folder layer and the transfers GET
      removal; `600c2906f` (P4.D377 + P4.D386), the item-route family;
      `b2b783b47` (P4.D386), the shared item browser.
    - `fd165fce0` (P4.D379 + P4.D388), Pascal presets on the tool walk.
    - `c539c82e0` (P4.D374 + P4.D363 + P4.D388), the Brahma lookups and
      the `@Brahma` profile.
    - `d4ddeb943` (P4.D368), avatar-job reuse PENDING-only.
    - `ba6c8fe0f` (P4.D373 + P4.D371), the probation prompts;
      `4efea65e5`, `d0201575a`, `c69a08882` (P4.D373), `7ff5fda89`
      (P4.D373 + P4.D371), Green Room spread, sample prompts 1.1.33, the
      inform header, group scenes.
    - `b09ca46bb` (P4.D369 + P4.D388), `96156566b` (P4.D370), `3ae4cf582`
      (P4.D371), the character-kind programme.
    - `6121f59b9` (P4.D375 + P4.D370), 4.12.1's window-first dated search.
    - `70d2859d8` (P4.D365 + P4.D382), `db integrity` without
      `cipher_integrity_check` + the ChaCha20 docs.
    - `29ba4d3f8` (P4.D383 + P4.D386), `3c41cf3da`, `4f5047241` (P4.D384
      + P4.D383), `671c59a53`, `df8d4d70a`, `27fb3ec77` (P4.D385),
      `87c3860df` (P4.D383), `94f041764` (P4.D383 + each file owner) —
      the SPA / CSS / WCAG 2.2 rows.
  - **Caught at unify:** the `doc_mount_file_links` overlay writes'
    missing store hint (v4 `aec626eff`'s one chokepoint, `6f9633ac2`); the
    production tool runners' `ErasedEmbeddingProvider::none()`
    (`d6202b2d3`). `921fe94bb` (#155, past the pin) converges on
    P4.D378's ruling; the carves were narrowed (`5c3930619`) and wait for
    the next baseline move.
- **The `236c04e35` 4.11.0 catch-up round (2026-11-04, baseline `322a5488e`
  → `236c04e35`; twenty lanes P4.D341–P4.D360, five stacking commits
  `KEYSTONE` / `CATALOG` / `CONTEXT` / `SPINE` / `FILES`):** sixty-eight of
  the sixty-nine rows `6fd2d5ffb..236c04e35` retired; `b15fe9404` stays in
  §3, PARTIAL (P4.D343 items 41 and 29). The round was pinned at the new
  baseline. The 62 commits past it stay UNPROCESSED in §3 (`/driftcheck`
  `ddda33e2f`). Round record: `status-log.md` → "The `236c04e35` 4.11.0
  catch-up round … — UNIFICATION record (2026-11-04)".
  - **`NO-PORT-RATIFIED(236c04e35)`** — P4.D360's §S.6 record, 22 whole
    rows, each `--stat` and code census re-run at lane time:
    - `332c650b9`, the intervention spec (vendored at its final
      `complete/` path only).
    - `4a25f7eab`, `505a524fa`, `3c9ae1fc3`, `b892227eb`, the 4.11.0
      release notes and their three corrections (`docs/releases/4.11.0.md`
      vendored at the pin's bytes).
    - `cb94e6e99`, the unread `debugLLMRequest` frame — v5 never emitted
      it; ratified with P4.D348's rider, which retired the harness drop.
    - `5e18ede6a` and `744596158`, the two merges (their own resolutions
      are CHANGELOG / stamps; each side is absorbed by its own row).
    - `a0905454b`, the restored diagram deps (the deps are P4.D353's R-B).
    - `8bf856050`, the pruned diagram trace lines (P4.D353 ports the
      post-prune shape).
    - `8becfc6ce`, the dead-code sweep (P4.D343 records `isComputedTool`'s
      absence).
    - `7d7e38d2f`, v4 tests (spec input to P4.D343's keepOrder rows and
      P4.D358's toast selector).
    - `66f8c4d41`, plugin bundling (v5 has no JS plugins).
    - `539bb4012`, the CLI README line (mirrored; the verb is NO-PORT under
      the `83334b9a5` round's R-P).
    - `c50cdddc6`, the CODE half (a test mock); its help halves were
      vendored by P4.D360, and P4.D355 confirms v5's backup leaves
      `terminal_sessions` out.
    - `dd200c695`, bug filing to GitHub issues (already this port's rule;
      `docs/v4/developer/bugs.md` re-vendored).
    - `b09ff5c86`, the feature-spec cleanup (docs ops only).
    - `3a98a6cca`, `3c818c856`, the release checklists 13 / 14.
    - `1ca45c171`, `9e01ebd25`, `236c04e35`, the 4.11.0 release, its
      merge-back and the 4.12.0-dev stamps (code = `7789ebed6`'s, by
      content).
  - **NO-PORT halves (P4.D360, 13):** the v4-internal halves of PORT rows —
    `82a2f6536`'s forked-child startup order, `026222176`'s SPA canvas race
    + `pdfjs-dist` worker + plugin rebundles, `faeae2b71`'s Docker / build
    scripts, `a4945d8d9`'s v4 docs, `58120a7fe`'s theme-tool / storybook /
    test selector, the `plugins/dist` rebuilds in `dff76ebef` / `68340008b`
    / `eed72d60c`, `10e09b7c3`'s icon script / storybook / footer padding,
    the storybook mirrors in `cce44e098` / `86cc25482`, the README /
    CHANGELOG / stamp / manifest hunks of `3b8b6f42b` / `7789ebed6` /
    `40cc6ffda` / `4b0d1cb04`, and the span-wide v4-internal set (`.claude`,
    v4 `CLAUDE.md` / README, scripts, `.github`, every `__tests__` /
    `__snapshots__`). Each PORT half stayed with its owner, below.
  - **`ABSORBED(236c04e35)`** (owners per §S.6; **+ P4.D360** wherever it
    vendored a `help/` / `docs/` / CLI-README hunk — 68 of the 69 rows,
    including the ten §S.6 omitted, marked *join*):
    - `6fd2d5ffb` (P4.D342), the importance rubric.
    - `a5efdbb97` (P4.D342), the inner circle in recall.
    - `8cb584eff` (P4.D341, P4.D342, P4.D359; the CLI verb NO-PORT R-D),
      keystone memories — the SEVENTEENTH D23 move.
    - `036b32a78` (P4.D348), bug 258 — a whisper starts no chain.
    - `eb2d87788` (P4.D348, P4.D359; *join*), every Salon LLM call logged.
    - `839575d89` (P4.D341 [the column], P4.D345, P4.D346, P4.D356),
      Pascal operator intervention — the TWENTIETH move. P4.D345 item 15's
      scripted-resolver tier-3 rows stay OPEN.
    - `946a50a04` (P4.D343, P4.D344), storyteller keepOrder.
    - `778b97b2b` (P4.D341 [the columns + the seed], P4.D351, P4.D352), the
      Novel template — the TWENTY-FIRST move. Residue OPEN: P4.D352 item 2
      (the preview modal's narration row), P4.D351 item 16 (the
      whole-entity schema twin, partial).
    - `eb90d9ed8` (P4.D349, P4.D359), wardrobe pronoun guidance.
    - `8ae8a127c` (P4.D345, P4.D346; *join*), intervention min/max ranges.
    - `99463fc56` (P4.D359; *join*), the floating Brahma Console.
    - `265b711dd` (P4.D349 [wardrobe], P4.D350 [inventory]), the picture
      cue.
    - `fa126bde8` (P4.D347), bug 259.
    - `40fab1c54` (P4.D354), bugs 260 / 261 (the upload text backfill).
    - `074e1e01a` (P4.D350), `self_inventory`'s checklist + holdings.
    - `b02d902f3` (P4.D345, P4.D346; *join*), the hidden-consult re-ask.
    - `f7a1789df` (P4.D347), the referent ledger + closing echo.
    - `d68c1af88` (P4.D347), absent-character cards into the ledger.
    - `9a88f862f` (P4.D347, P4.D359), absent-character name chips.
      Residue OPEN: P4.D347 item 24, the REST arm (v5 serves the read over
      `/api/dispatch`, the recorded R-3 shape).
    - `b27e5089a` (P4.D349, P4.D359), the `humanoid` flag.
    - `1dd0a0c01` (P4.D351, P4.D352, P4.D356), punctuation warnings.
    - `0276d10d9` (P4.D350, P4.D359; *join*), the Inventory chokepoints;
      its item-images `InventoryScopeSchema` enum order, carried by no lane,
      was fixed by the unifier.
    - `a8214b253` (P4.D348), the Salon turn pipeline (bug 262).
    - `bdd09b75b` (P4.D347), the context manager (bugs 263–265).
    - `6588972d5` (P4.D349 [server], P4.D353 [client]), diagrams.
    - `9faed0c7e` (P4.D353; *join*), the diagram lightbox + Copy PNG.
    - `2041bb875` (P4.D359), the Automation checkbox class.
    - `40cc6ffda` (P4.D356 verify-only; *join*), `db --count`.
    - `cc5f2347c` (P4.D355), the Log in backups (flipped P4.D327's pins by
      design).
    - `026222176` (P4.D354), bug 266 / pdfjs 6.4 / `pdf-parse` removed (the
      IMPORT-DEAD oracle re-aimed).
    - `85c002822` (P4.D355), terminal bugs 267 / 268.
    - `4b0d1cb04` (P4.D356; *join*), bug 269 — NanoGPT image sizes.
    - `2b2b190b6` (P4.D352; *join*), composer prose mode.
    - `cce44e098` (P4.D357, with the intervention / storyteller CSS halves
      in P4.D346 / P4.D344), the legibility pass.
    - `86cc25482` (P4.D357), legible user dialogue.
    - `d64b25e96` (P4.D360), the help pronoun fix.
    - `58120a7fe` (P4.D357, P4.D358), the WCAG contrast pass + the toast
      role.
    - `dff76ebef` (P4.D358; *join*), dialog semantics. One ruled divergence:
      `WARDROBE_VIEWER_ESCAPE_LAYERED` (§1).
    - `eed72d60c` (P4.D358), accessible names.
    - `68340008b` (P4.D358), the composer and field-editor names.
    - `10e09b7c3` (P4.D358, P4.D357 [CSS]), the remaining WCAG items.
    - `a4945d8d9` (P4.D359 [About]), the checklist-14 docs pass.
    - `82a2f6536` (P4.D354), the chunk-embedding guard.
    - `faeae2b71` (P4.D354), the Docker persistent-target guard + the CLI's
      in-container base dir.
    - `3b8b6f42b` (P4.D356, P4.D359), the Core-whisper PUT + the unknown-key
      400 (the SPA Core-whisper card stays the standing deferral).
    - `7789ebed6` (P4.D356 — CONVERGENCE), #120: v4's fix for this port's
      own F-1 filing; v5 converged onto it.
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
