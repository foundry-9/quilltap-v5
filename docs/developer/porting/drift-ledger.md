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

- **Oracle baseline: `236c04e35`**: "dev: started 4.12.0 development" (v4
  main, 2026-10-28 19:30 −0500, `package.json` `4.12.0-dev.0`). Its code is
  v4's 4.11.0 release, measured by CONTENT: `1ca45c171` = `7789ebed6`'s
  tree, and `236c04e35` adds only the `4.12.0-dev.0` stamps. It was adopted
  when the `236c04e35` 4.11.0 catch-up round was unified (2026-11-04). All
  twenty lanes landed: P4.D341 → {P4.D350 → P4.D349} → {P4.D347 → P4.D348 →
  {P4.D343 ∥ P4.D345}} ∥ P4.D342 ∥ P4.D351 ∥ P4.D356 ∥ P4.D354 → P4.D355 ∥
  P4.D357 ∥ P4.D344 ∥ P4.D346 ∥ P4.D352 ∥ P4.D353 ∥ P4.D359 ∥ P4.D360 ∥
  P4.D358. The round was pinned at the same commit, so the baseline equals
  the pin. Sixty-eight of the sixty-nine §3 rows `6fd2d5ffb..236c04e35`
  retired to §6; **`b15fe9404` stays in §3, PARTIAL** (P4.D343 item 41, the
  `simultaneous` order, and item 29, the `.qtap` import refusals). The
  previous baseline was `322a5488e` (2026-10-27 → 2026-11-04).
- **Checked:** 2026-11-04, by `/driftcheck` (`ddda33e2f`) run as this
  round's `/unify` started, on v5 main (`8976915ea`). **The baseline move
  did NOT re-probe v4; that state stands.** The §2 probe had FAILED on
  fifty-two new `main` commits past the recorded `544c7ab1e` (51
  first-parent; `29ba4d3f8` … `7b4e1b9fa`, `4.12.0-dev.10` →
  `4.13.0-dev.0`) and a moved `bugfix` / `release`. Fetched first (tags
  too); local equals `origin` on all three branches.
  - **v4 RELEASED 4.12.0** (`7bdb45873`, tag `4.12.0`, 2026-11-03) and
    started `4.13.0-dev`. By CONTENT, `git diff 70d2859d8 7bdb45873` is the
    4 stamp files (+5 / −5), so **4.12.0's code = `70d2859d8`'s tree**;
    `7bdb45873..main` adds only the `4.13.0-dev.0` stamps and the CHANGELOG
    split (`docs/CHANGELOG_V4.md`). The topology repeats 4.11.0's: the
    release squash's sole parent is `1ca45c171`, and `merge: 4.12.0 back
    into main` (`b37769882`) makes it an ancestor of `main`.
  - `gh issue list -R foundry-9/quilltap-server --label bug --state all`
    returns #120, #121 and #133–#137, all CLOSED. #121 and #133–#137 are
    "Original to v4" — **NO CONVERGENCE** in the 52 (#120 was this port's,
    absorbed by P4.D356).
  - Earlier checks (2026-10-29 twice; 2026-10-28 four times; 2026-10-27
    three times; 2026-10-26 and before) are recorded in the §6 bullets and
    this file's git history. Their §3 rows are retired, held (`b15fe9404`),
    or among the 62 below.
- **v4 `main` HEAD at check: `7b4e1b9fa`** ("docs: CHANGELOG update for new
  version", 2026-11-03, `4.13.0-dev.0`) — **SIXTY-TWO commits past the
  baseline, all UNPROCESSED in §3**: the 10 of 2026-10-29 plus the 52 of
  2026-11-04. (Against the old baseline `322a5488e` it was 131.)
  - **The 10 of 2026-10-29** (`8832cfd20` … `544c7ab1e`, `4.12.0-dev.1` →
    `dev.9`):
    - *Dependency move:* `8832cfd20`, the driver to
      `better-sqlite3-multiple-ciphers` 13.0.3, which moves v4 onto
      **SQLite3MC 2.4.0 / SQLite 3.53.4**. Build flags
      (`deps/defines.gypi`) are byte-identical to 12.11.1; there is NO
      migration and NO D23 move; one new server INFO line. **A ruling is
      owed on v5's vendored 2.3.5 amalgamation.**
    - *PORT:* `bf7a457f6`, the iCloud-placeholder scan fix on the ported
      scanner and `?action=scan` (a NEW `filesInCloud` wire key). The
      boot-scan split and the watcher are NO-PORT: v5 has never had a
      watcher. **`544c7ab1e`**: the Anthropic API version `2026-10-21` with
      NO sampling parameters, documents moved into a user turn,
      `ProviderCapabilities.samplingParameters` (plugin-types 2.10.0), the
      repository stripping sampling keys, and a NEW DATA-REWRITE MIGRATION
      `strip-anthropic-sampling-parameters-v1`.
    - *NO-PORT?:* `db4a76e39`, the chokidar drop (both watchers are
      NO-COUNTERPART; one About-credits line is a PORT small); `f56965ef9`
      (`openai` 7.45.1 / `@openrouter/sdk` 1.5.21 —
      `provider_sdk_version_guard` trips at any pin past it); `3936fdf65`;
      `eb90eb466`; `7dedfa067`; `3e4f9dea9`; `9f761e157`.
  - **The 52 of 2026-11-04**, by class (§3, from the hunks):
    - *PORT / PORT-NEW with a D23 move:* `db6190b44`, Deus Ex Machina —
      established facts (the 22nd move); `d439f6dc9`, the per-profile image
      cap (the 23rd move; `openai` ^7.47.0 / `@openrouter/sdk` ^1.5.33 ride
      it).
    - *PORT with a non-D23 migration:* `902c6efe9`, Staff rows out of global
      search (an FTS5 trigger replace + rebuild; #121).
    - *PORT — wire or behaviour moves on ported surfaces:* **`604a082c4`,
      the Salon SSE wire becomes `kind`-tagged frames** (Help chat / Brahma
      / Scenario Builder / one-shot keep the old bag); **`29c1e2690`, the
      image transport budget REWRITTEN** (4 MB base64 total, provenance
      ranking, an omission text); `a3d53eae1`, the document core (#133 /
      #134 / #137); `0c47f4539`, the tier walk, and reads no longer
      provision stores (#135 / #136); `600c2906f`, the item-route family (a
      NEW `?action=duplicate`, a NEW `wardrobe` realtime topic);
      `30a3b4cb7`, document moves; `a0ed5ecad` (`GET
      /api/v1/wardrobe/transfers` REMOVED — v5 still serves it);
      `aec626eff`, the mount-point document-store API (`listForBrowser`,
      `?action=links` / `render-html` via mammoth, copy / move /
      delete-folder, per-store realtime); `5efe53ea7` / `c17033f04`,
      Document Mode (non-text read-only, the General library, blob text
      editing); `fd165fce0`, Pascal presets on the tool walk; `9eab8d2da`,
      gesture-only yields + a `handAsked` frame; `ba6c8fe0f`, the
      "probation" prompt text (generators, extraction, consolidation, recall
      tags, 21 sample prompts); `4efea65e5`, Green Room multi-slot spread;
      `d4ddeb943`, avatar-job reuse PENDING-only; `4310d435f`, a stat error
      class; `c539c82e0`, the Brahma lookup tools + the `@Brahma` profile (a
      NEW project vault key); `70d2859d8`, the CLI's `db integrity` (no more
      `cipher_integrity_check`).
    - *SPA / CSS:* `29ba4d3f8`, `3c41cf3da`, `4f5047241`, `671c59a53`,
      `df8d4d70a`, `b2b783b47`, `c34986421` (the SVAR dependency removed),
      `27fb3ec77`, `87c3860df` (undefined CSS tokens), `94f041764` (a second
      WCAG 2.2 AA pass + five theme packs).
    - *NO-PORT? (docs / tests / tooling / stamps):* `d03eb2d63`,
      `a299de9f5`, `f12bda3d1`, `3d39c303c` (verify "output unchanged"),
      `134961572`, `f1c2b178d`, `06efd9ca2`, `d2795f72c`, `25a20221c`,
      `705b954c2`, `ec8d91667`, `05264ace3` (the watchers — NO-COUNTERPART;
      its scanner walk is a PORT? beside `bf7a457f6`), `547f1a739`,
      `856ed4f2b` (+ an About line), `8f83041aa`, `9b5bb27df`, `cbf4805c0`,
      and the four release commits `7bdb45873` / `b37769882` / `013b99b5d` /
      `7b4e1b9fa`.
  - **Surfaces the `236c04e35` round just LANDED that these commits move
    again** — v4 drift, not v5 bugs, when the next catch-up's pins trip:
    Storyteller (`604a082c4` DELETES the `tagSseChunk` seat-tagging P4.D343
    / P4.D344 ported, and re-shapes every Salon frame); Pascal intervention
    (`fd165fce0` on P4.D345 / P4.D346's run route and dialog); the composer
    and prose mode (`db6190b44`'s `/deus` + Cmd/Ctrl+Shift+Enter and
    `df8d4d70a`'s tooltips on P4.D352's composer); diagrams (`8f83041aa`
    moves `mermaid` to 12.3; P4.D353 pins 12.2); WCAG / themes (`87c3860df`
    + `94f041764` over P4.D357 / P4.D358's files and the same five packs);
    the mount index (`aec626eff`, `c34986421`, `c17033f04`, `05264ace3`,
    `0c47f4539` — P4.D354's family); the context builder (`db6190b44`,
    `ba6c8fe0f`, `d439f6dc9`, `29c1e2690`, `0c47f4539`'s knowledge injector
    — P4.D347); the memory rubric (`ba6c8fe0f` and `db6190b44` edit
    P4.D342's extraction prompts); vault keys (`c539c82e0`'s
    `brahmaConnectionProfileId` beside P4.D349's `humanoid` / pronoun /
    diagram keys). The Log carrier (P4.D355) is untouched.
- **v4 `bugfix` tip at check:** `17cb0e819` ("Merge tag 'rel/4.12.0' into
  bugfix", 2026-11-04 07:22 −0600; parents `5cde81c68` "bugfix: started
  4.12.0 bug branch" [`4.12.1-bugfix.0`, parent `73b334af3`] +
  `b37769882`). It landed DURING the check. Its tree is `release` + the
  bugfix stamps (3 files +4 / −4), and `git diff 70d2859d8 17cb0e819 -- lib
  app packages plugins migrations` is EMPTY: NO unabsorbed content. The
  merge makes `main`'s history an ancestor of `bugfix` for the first time;
  the content rule still governs. The §2 probe's bugfix base is
  `17cb0e819`. **`release` tip:** `7bdb45873` (`release: 4.12.0`, code =
  `70d2859d8`). Re-probe ALL THREE branches every time.
- **Checkout at check:** branch **`bugfix`** (⚠ WRONG BRANCH) at
  `17cb0e819`, tree **DIRTY — docs only**: STAGED `docs/CHANGELOG.md`
  (−612) + `docs/CHANGELOG_V4.md` (+614), the same 4.x history split
  `main`'s `7b4e1b9fa` made. This is infra-only dirt; it cannot poison a
  regen, and it is recorded so the next probe does not re-alarm. The human
  switched the checkout from `main` (CLEAN at `7b4e1b9fa`) mid-check. Its
  code is 4.12.0's, so a regen from it runs SIXTY-TWO commits past the
  baseline: poisoned by DRIFT and by BRANCH. The live root `node_modules`
  carries `openai` 7.47.0 / `@openrouter/sdk` 1.5.34 / `mermaid` 12.3.0 /
  TypeScript 6.0.3 / the v13 driver, with SVAR gone.
- **Verdict: DRIFT PENDING — 62 commits past the baseline, all UNPROCESSED
  (§3)**, plus the held PARTIAL row `b15fe9404`: THREE migrations past the
  baseline's own (TWO D23 moves — the 22nd `isEstablishedFact`, the 23rd
  `maxImagesPerRequest` — and the FTS rebuild), plus `544c7ab1e`'s
  data-rewrite migration; two ported-wire moves (the Salon's `kind`-tagged
  frames, the image transport budget); the SQLite3MC 2.4.0 driver move (a
  ruling owed); six v4-originated issue fixes (#121, #133–#137), NO
  convergence; one new vault key; ~28 NO-PORT? candidates (7 of the 10,
  21 of the 52).
- **Regen rule: PIN REQUIRED at `236c04e35`** (§5.1). Never regen from the
  live checkout: it is on `bugfix`, sixty-two commits past the baseline.
  - The pin's root takes its own `npm ci --offline --ignore-scripts`, then
    `npm rebuild better-sqlite3` (SQLite 3.53.2 / chacha20 — the pin's
    lockfile wants the 12.11.1 driver; the live tree has 13.0.3).
  - The pin's plugin + CLI dirs take the floor install
    (`.claude/pin-plugin-sdks.sh <PIN> <COMMIT>`, the 2026-11-02 rulings),
    never live symlinks. The script refuses a pin not at `<COMMIT>`. Its
    `--offline` installs answered `ENOTCACHED` for every lane after P4.D354
    (the npm cache lacked the floor packuments and
    `@quilltap/plugin-types@2.8.0`); since 2026-11-04 the script retries a
    cache miss once with `--prefer-offline`, which fetches only the missing
    public metadata and still installs the exact floors inside the pin
    (`openai` 7.23.0, anthropic 0.115.0, genai 1.52.0, `@openrouter/sdk`
    1.3.28, plugin-utils 2.7.1; the CLI driver 12.11.1). When the pin moves,
    re-check the printed floors against `provider_sdk_version_guard`.
  - `QT_V4_CHECKOUT` = `QT_V4_ROOT` = the pin.

### ⚠ LIVE-DATA — what stands at this baseline

- **A v5 at `236c04e35` reads and writes** `memories.keystone` (the 17th
  move: the migration's `INTEGER NOT NULL DEFAULT 0` + the partial index
  `idx_memories_keystone`, as ONE boot ensure), `chats.turnMode` /
  `storytellerInviteFilter`, `chat_messages.storytellerBid`, the NEW table
  `storyteller_rounds` (its FK + both indexes as ensures),
  `llm_logs.storytellerRoundId` (the first llm-logs column ensure v5 has
  needed), `chat_settings.customToolIntervention` (default ON) and
  `roleplay_templates.unmarkedStyle` / `markdownAsterisks`. It runs
  `778b97b2b`'s every-boot seed, which re-encodes the built-in templates'
  `narrationDelimiters` as JSON and inserts the Novel template with it
  `null`.
  - **The first v5 boot of a pre-`8cb584eff` Friday copy** ADDS the eight
    columns + one table (column-gated, v4's shapes, every ensure UNSTAMPED)
    and REWRITES the built-in templates' delimiter cells `*` → `"*"`. A
    copy v4 migrated sees NO-OP ensures and a no-op seed. The first-boot
    seed rewrite on a real vintage copy is still to measure (P4.D341 item
    24, the owed dogfood pass).
- **The three vault-key ERASURE hazards RETIRE on the union build**
  (P4.D349's statement): after P4.D349 a v5 physical write keeps `humanoid`
  (an explicit value as given; an omitted flag → the vault's current
  value); a v5 properties write (full, RMW, backfill, scaffold) carries
  `canDrawDiagrams` and `diagramStyle`; a v5 wardrobe projection carries
  `pronounGuidance`; the P4.82 head-and-shoulders backfill skips a
  non-humanoid. **A v5 build from BEFORE this round still erases them:** no
  pre-round v5 write to a character v4 edited past `6588972d5` /
  `b27e5089a` / `eb90d9ed8` (reads stay safe).
- **Keystones are protected:** maintenance, demotion, consolidation, fold
  and chat-wipe skip `keystone = 1` rows; recall lifts them into the
  circle; a v5 backup / `.qtap` carries the column. A pre-round v5 build
  must NOT run maintenance on a copy v4 keystoned.
- **The Log is carried** (`cc5f2347c`, P4.D355): a v5 backup writes
  `data/log-entries.json`; a `replace` restore / delete-all clears
  `log_entries`; a `new-account` restore remaps `id` / `chatId` /
  `characterId`; `.qtap` still omits it. A pre-round v5 backup has no Log
  file, so restoring it on the union leaves the Log empty (expected).
- **Terminals** (`85c002822`, P4.D355): Kill sends SIGHUP, then SIGKILL
  after 3 s; a deleted chat (including delete-all and a `replace` restore)
  ends its shells, and v5 deletes its `terminal_sessions` rows explicitly;
  the daily sweep unlinks orphaned transcripts older than 1 h.
- **Settings:** the chat-settings PUT refuses unknown keys with a 400
  (`3b8b6f42b`, P4.D356).
- **Files:** ingest fills `doc_mount_blobs.extractedText*` for native-text
  uploads, and a boot backfill fills old rows (`40fab1c54`, P4.D354) — a
  DATA WRITE on the first boot of any copy with uploads (measure the row
  count; PDF / DOCX rows stay `'none'` under the refusing seam). The
  backfill blocks boot where v4 runs it fire-and-forget (P4.D354 K10).
- **The general rule BITES:** a v5 build from before this round must not
  open an instance this round's ensures have altered. The owed dogfood pass
  runs with the UNION build.
- **Past `236c04e35` — build and scan:** `8832cfd20` moves v4 onto
  SQLite3MC **2.4.0** / SQLite **3.53.4** (v5 links 2.3.5 / 3.53.2; sqleet
  on both). v4 says "v12 and v13 databases interoperate" and ships NO
  migration; a v5 at 2.3.5 opening a file v4 wrote under 2.4.0 is the
  ordinary SQLite forward-compat case, NOT measured — the first v5 open of a
  post-`8832cfd20` Friday copy is the measurement. `bf7a457f6`: a v4-HEAD
  scan LEAVES an iCloud-evicted placeholder unread (`in-cloud`, its row
  kept); a v5 scan HASHES it, which makes iCloud download it, and may index
  it as modified or empty. Non-corrupting for the DB; the Friday copy's
  stores live outside iCloud's eviction set.
- **Past `bf7a457f6` — ONE data-rewrite ping-pong class:** `544c7ab1e`'s
  `strip-anthropic-sampling-parameters-v1` drops `temperature` / `top_p` /
  `topP` / `top_k` / `topK` from Anthropic profiles' `parameters`, and v4's
  repository strips them on every later write. v5 still SENDS them under
  the old pinned `anthropic-version` (harmless until v5 moves the header);
  a v5 write of such a profile puts the keys BACK on a shared instance.
- **Past `544c7ab1e` (v4 4.12.0) — two additive columns, ONE index-rebuild
  ping-pong, ONE new vault key:**
  - `chat_messages.isEstablishedFact INTEGER DEFAULT NULL` and
    `connection_profiles.maxImagesPerRequest INTEGER DEFAULT 4` (+ a
    NULL-only backfill) are additive with DEFAULTs; a v5 INSERT that does
    not name them is legal. v5 reads an established-fact USER row as a plain
    line and a Host row `systemKind='established-fact'` as an unknown kind;
    v5 ignores the image cap.
  - ⚠ **`reindex-chat-message-fts-character-only-v1` (`902c6efe9`):** v5's
    boot reconcile counts eligibility by the OLD filter
    (`chat_message_fts_reconcile.rs:144`), so a v5 boot of a v4-migrated
    copy sees eligible ≠ indexed and REBUILDS the whole index (re-indexing
    Staff rows; v4's newer trigger, created `IF NOT EXISTS`, survives); v4's
    next boot rebuilds again. Measured from source, NOT run. Non-corrupting,
    but expect the WARN `Message search index is out of step with the
    transcript; rebuilding` on the first v5 boot of a post-4.12 copy.
  - ⚠ **Vault-key ERASURE (`c539c82e0`):** a v5 project properties write
    drops `properties.json` `brahmaConnectionProfileId`.
    `brahmaConsole.inlineConnectionProfileId` sits in an existing settings
    value — measure whether v5's Brahma settings PUT preserves it.
  - The 4.12.0 docs now say ChaCha20 / SQLite3MC, not SQLCipher
    (`70d2859d8`) — a documentation convergence onto this port's Phase-0
    finding.
- **Standing:** no v5 build against live Friday — dogfood a COPY, and do
  not round-trip v4-written characters through v5 writes while comparing
  with v4. The extraction rate ledger is per-process; the `52d6e7ecd` /
  `07b8f0209` notes and the P4.D274 store-name plan probe on a copy stand.

### Schema / D23

- **LANDED this round: the SEVENTEENTH to TWENTY-FIRST moves in ONE
  `fresh_schema.json` re-dump** (P4.D341, the `KEYSTONE` commit
  `c2c28c031`): `memories."keystone"` (+ `idx_memories_keystone`),
  `chats."turnMode"` / `"storytellerInviteFilter"`,
  `chat_messages."storytellerBid"`, the NEW table `storyteller_rounds`,
  `llm_logs."storytellerRoundId"` + its index, `chat_settings.
  "customToolIntervention"`, `roleplay_templates."unmarkedStyle"` /
  `"markdownAsterisks"`, plus the boot-seed rewrite of
  `narrationDelimiters` and the Novel row. Five boot ensures, each in
  `restore_vintage_state`'s mirror-boot; `migration_indexes.json` +3.
  Vault / JSON keys landed with them: `humanoid`, `canDrawDiagrams` /
  `diagramStyle`, `pronounGuidance` (P4.D349),
  `smartTypographySettings.punctuationWarnings`, and the `systemKind` value
  `off-scene-left-behind`.
- **PENDING past the baseline — the TWENTY-SECOND and TWENTY-THIRD moves**,
  both additive, both with v4 migrations, both TWO-shape (the Zod field sits
  mid-schema; the migration APPENDS):
  - **22nd (`db6190b44`, `add-established-fact-field-v1`):**
    `chat_messages."isEstablishedFact" INTEGER DEFAULT NULL` — schema order
    between `isSilentMessage` and `systemSender`.
  - **23rd (`d439f6dc9`, `add-profile-max-images-per-request-v1`):**
    `connection_profiles."maxImagesPerRequest" INTEGER DEFAULT 4` — schema
    order between `supportsImageUpload` and `tags` (`DDL.md` lists it last;
    generateDDL follows the schema); the migration also backfills NULL → 4.
  - **Not D23:** `reindex-chat-message-fts-character-only-v1` (`902c6efe9`)
    — FTS5 trigger text + an index rebuild (a STAMPED boot ensure on v5's
    `chat_message_fts` module, not a dump);
    `strip-anthropic-sampling-parameters-v1` (`544c7ab1e`, a data rewrite);
    vault / JSON keys `properties.json` `brahmaConnectionProfileId`,
    `brahmaConsole.inlineConnectionProfileId`; the `systemKind` value
    `established-fact`; the job type `established-fact-memories`.
  - A further `fresh_schema.json` re-dump is owed at the next pin.

### Regen, dependencies, help

- **Dependencies at the baseline:** root `openai` 7.45.0 /
  `@openrouter/sdk` 1.5.9 (`provider_sdk_version_guard`, measured by
  P4.D360 — no recorder resolves the root), `zod` 4.6.7, `pdfjs-dist`
  6.4.299 with `pdf-parse` REMOVED (P4.D354 re-aimed the IMPORT-DEAD
  `file-content-extractor` oracle; R-C: no new PDF crate),
  `@quilltap/plugin-types` 2.9.0, `@xterm/headless` 6.0.0 (v5's twin is
  `vt100` 0.18.1); v5's `regress` crate backs the llm-logs `regexp()`. The
  diagram client deps landed under P4.D353's ruling R-B (A) (`mermaid`
  12.2.0, `@hpcc-js/wasm-graphviz` 1.30.0, `svg2roughjs` 3.2.3, `dompurify`
  3.4.17), and P4.D358's axe sweep added `@axe-core/playwright`.
- **Past the baseline (recorded 2026-11-04):** `openai` ^7.45.0 → ^7.45.1
  (`f56965ef9`) → **^7.47.0** (`d439f6dc9`); `@openrouter/sdk` ^1.5.9 →
  ^1.5.21 → ^1.5.33 → **^1.5.34** (`8f83041aa`) —
  `provider_sdk_version_guard` moves at any later pin; `mermaid` 12.2 → 12.3
  (`8f83041aa`); `@svar-ui/react-filemanager` REMOVED (`c34986421`);
  `@playwright/test` REMOVED (`856ed4f2b`); `mammoth` now also renders
  `.docx` → HTML for `?action=render-html` (`aec626eff` — no v5 twin; a
  ruling); the v13 driver (`8832cfd20`); `zod` UNCHANGED 4.6.7.
  - ⚠ **IMPORT-DEAD at any pin ≥ 4.12.0 (measured by grep over
    `harness/oracle`, 2026-11-04):** `lib/chat/message-attachment-adapter`
    (DELETED by `29c1e2690`) ← `cases/file-attachment-tier3.test.ts` (also
    its `adaptMessagesForProfile`); `lib/wardrobe/folder-mutations`
    (DELETED by `600c2906f`) ← `cases/item-mutations-tier2.ts`; the removed
    `createInventoryFolder` / `renameInventoryFolder` /
    `deleteInventoryFolder` / `normalizeInventoryFolderName` (`134961572`)
    ← `cases/inventory-keystone-tier2.ts`, `cases/inventory-types.ts`,
    `cases/item-folders.ts` — a stage-0 re-aim for the next catch-up. The
    Help-chat / Brahma / Scenario-Builder oracles that read the stream bag
    stay on `legacyFrameEncoder`; the Salon-turn stream oracles do not.
- **Help / docs:** `help/` is **137 files** (+ `diagrams.md`,
  `storyteller-mode.md`), md5-identical to the pin; `docs/v4/` **518**
  (P4.D360). Re-measure the past-the-baseline help / docs hunks at the next
  pin.
- **Themes / CSS:** v5 vendors `apps/web/public/themes/*` and
  `apps/web/src/styles/qt-components/*.css` at the pin's bytes (P4.D357;
  `known-violations.json` re-baselined on the union, 696 → 521 rows). Past
  the baseline, `87c3860df` + `94f041764` move the CSS and five packs; v5
  has no `theme-storybook` (its mirror hunks are NO-PORT).

### Rounds, guards, state

- **The `236c04e35` round — UNIFIED 2026-11-04** (the §6 bullet; the round
  record in `status-log.md`; `phase-4.md`'s UNIFIED block).
  - **Ratified at this move (P4.D360's §S.6 record, §6 lists them):**
    twenty-two whole rows `NO-PORT-RATIFIED` (`cb94e6e99` with P4.D348's
    rider) and thirteen NO-PORT halves (the v4-internal half of a PORT row
    — plugin rebundles, storybook, scripts, Node fork artefacts, tests,
    `.claude` / v4 `CLAUDE.md` / README; the PORT half stays with its
    owner). P4.D360 also JOINS every row whose `help/`, `docs/` or CLI
    README hunk it vendored (68 of the 69; ten joins §S.6 had omitted:
    `eb2d87788`, `8ae8a127c`, `99463fc56`, `b02d902f3`, `0276d10d9`,
    `9faed0c7e`, `40cc6ffda`, `4b0d1cb04`, `2b2b190b6`, `dff76ebef`).
  - **Unabsorbed drift caught at unify and fixed in production:** v4
    `0276d10d9`'s `InventoryScopeSchema` enum order in the item-images
    `SCOPE_ISSUE` (no lane had carried it).
  - **Ruled divergence `WARDROBE_VIEWER_ESCAPE_LAYERED`** (human,
    2026-11-04): with the wardrobe item editor's picture viewer open, Escape
    closes ONLY the viewer in v5; v4 at `236c04e35` closes both (v4
    `dff76ebef`'s `useEscapeKey` on `window` beside `useImageNavigation` on
    `document`, neither stopping propagation —
    `WardrobeItemImageSection.tsx:513`). Pinned red-first (`2655348b8`). A
    v4 GitHub issue is a filing candidate, pending the human's go-ahead.
  - **Ruling — Storyteller "All at once"** (human, 2026-11-04): hidden until
    ported. The server answers 422 `not_available: storyteller
    simultaneous` (P4.D343 item 41 DEFERRED); the SPA control renders
    DISABLED with a house-register title and never dispatches; v4's enable
    rule and title are recorded beside it; e2e beat (K) is skipped by name.
    It re-enables when item 41 lands, which also retires `b15fe9404` from
    §3.
- **The `322a5488e` round — UNIFIED 2026-10-27**; its follow-ups smalls
  round (P4.165–P4.174) UNIFIED 2026-10-28, absorbing no §3 row.
- **Next:**
  1. The owed dogfood pass on a Friday copy with the UNION build (the
     round's 💸 rows; P4.D341 item 24's first-boot seed rewrite first).
  2. A follow-ups smalls round: the round record's recorded notes, plus
     P4.D343 item 41 (re-enables "All at once") and item 29.
  3. The 4.12.0 catch-up (`/driftcheck` first): the 62 commits, the 22nd /
     23rd D23 moves in ONE re-dump, the SQLite3MC 2.4.0 amalgamation
     ruling, the FTS reconcile and vault-key hazards first, the stage-0
     IMPORT-DEAD re-aims, and the pin-setup cache fix above.

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
| `b15fe9404` | 2026-10-22 | Feature: Storyteller mode and declared tool effects | **PORT-NEW (Storyteller — a third Salon turn mode) + PORT (declared tool effects REPLACE `DESTRUCTIVE_TOOL_NAMES` with an execution-time ceiling) WITH THE EIGHTEENTH + NINETEENTH D23 MOVES — TWO migrations, both additive, no data rewrite, one a NEW TABLE, one in the LLM-LOGS partition** (classified 2026-10-26 from the hunks; 194 files +5,802 / −268; prod 172 files +5,433 / −244; `dev.58`; parent `036b32a78`). **D23:** `add-storyteller-mode-v1` (main; `dependsOn sqlite-initial-schema-v1`): `chats."turnMode" TEXT DEFAULT NULL` (NULL reads `'rotation'`; patch-only), `chats."storytellerInviteFilter" INTEGER DEFAULT 0`, `chat_messages."storytellerBid" TEXT DEFAULT NULL` (JSON), and a NEW TABLE `storyteller_rounds` (`id` PK, `chatId` NOT NULL, `triggerMessageId`, `status` NOT NULL, `bids DEFAULT '[]'`, `selection` / `closedReason` / `closedAt`, `createdAt` / `updatedAt` NOT NULL, FK `chatId → chats ON DELETE CASCADE`, `idx_storyteller_rounds_chat (chatId, status)`); `add-llm-logs-storyteller-round-id-column-v1` (llm-logs; `dependsOn add-llm-logs-autonomous-run-id-column-v1`; skips when the logs file is absent): `llm_logs."storytellerRoundId" TEXT DEFAULT NULL` + `idx_llm_logs_storytellerRoundId`. generateDDL renders every column (`turnMode` bare `TEXT`, `storytellerInviteFilter` between `isPaused` and `isManuallyRenamed`, `storytellerBid` before `pascalMeta`, `storytellerRoundId` after `autonomousRunId`) AND `storyteller_rounds` (its repository is in `RepositoryContainer`) but WITHOUT the FK and the index — those become boot ensures. **Effects (PORT):** NEW `lib/tools/effect-types.ts` (`none\|read\|write\|destroy`, `UNDECLARED_TOOL_EFFECT='write'`, `computedToolEffect`, `isWithinCeiling`, `lowerToolEffect`) + `lib/tools/effects.ts` (`BUILT_IN_TOOL_EFFECTS` from a per-tool `xxxToolEffect` export; four computed effects — `state`, `doc_open_document`, `edit_custom_tool`, `run_custom` — each with a per-ceiling note appended to the description by `applyEffectCeilingToTools`; the model-visible refusal ``Not run: `${name}` is a ${effect} call, and this turn allows only ${phrase}. Nothing was changed. Carry on without it.``). `destructive-tools.ts` DELETED: the autonomous-room gate lowers the ceiling to `write` — **the withheld set widens 3 → 5** (`doc_delete_blob`, `delete_annotation` newly withheld) — and the ceiling is enforced at EXECUTION in `executeToolCallWithContext`; `buildTools()` returns `customToolEffects` (`lib/pascal/tool-effect.ts`) + `pluginToolEffects`; triggers skip a tool above the ceiling; the Workbench library gains `effect`; `@quilltap/plugin-types` 2.9.0, MCP plugin 1.1.45 (per-server `effect`), curl 1.0.32. **Storyteller (PORT-NEW):** `lib/storyteller/*` (bid call `toolChoice:'none'`, logType `STORYTELLER_BID`; the BID/TIER/TOOLS/PLAN parser; tiers; invite probability `0.25+0.75×talkativeness`; the model-visible bid / plan sections under `[Turn note from the Salon — not spoken by any character]`; `STORYTELLER_BOARD_TIMEOUT_MS=45_000`, `STORYTELLER_CONCURRENCY_PER_PROFILE=4`; AsyncLocalStorage → `llm_logs.storytellerRoundId`; SSE `participantId` + `concurrent:true`); a dry repository scope (`lib/repositories/dry-scope.ts`, with the read/write rulebook moved VERBATIM into `method-classification.ts`); `buildContext` `purpose:'turn'\|'bid'` + `storytellerSection` LAST in the trailing list; orchestrator `holdForStoryteller` / `openRoundOnSend` (SSE `storytellerRound`, `storytellerBid`, `roundOpen`, `chainComplete` reason `'round_open'`) / `runStorytellerBid` / `handleStorytellerRun`; the finalizer writes `storytellerBid` and defers extraction / fold / danger to `runStorytellerRoundHousekeeping`; routes GET `?action=storyteller-round`, POST `?action=storyteller-run\|storyteller-ask\|storyteller-lie`, chat PATCH `turnMode` / `storytellerInviteFilter`, the chat GET wire gains both; realtime topic `storytellerRounds`; the replace restore TRUNCATES `storyteller_rounds` (never backed up); the `.qtap` schema gains `turnMode` / `storytellerInviteFilter` / `storytellerBid`; SPA `StorytellerBoard`, the sidebar's three-position mode radiogroup, the Inspector filter; help `storyteller-mode.md` NEW + six edits. **v5 intersection:** `services/tool_build.rs:690`'s `DESTRUCTIVE_TOOL_NAMES` filter + `tool_build_equivalence` (the 3 → 5 widening); the executor dispatch; `StreamMessage` `toolChoice` (P4.13/P4.14); `build_context.rs`'s trailing sections; the processMessage spine + turn chain; the trigger runner (P4.D305); the Workbench library (P4.D307); llm-logs D23 (P4.D49); realtime; the restore delete list (P4.9G); `public/schemas` (P4.D313). No v5 analogue for the dry scope or plugin tool effects. Rides ORDERED P4.D328 / P4.D331 (the new tools need `xxxToolEffect`), P4.D332 (`edit_custom_tool`'s computed effect), P4.D329 / P4.D333, P4.D334, P4.D335 / P4.D336 (`runWithTurnControl`, `chainComplete` reasons). **Comments claim what no hunk does:** `storytellerBid` "stripped from every transcript" (never copied, not stripped); `dry-scope.ts` names a nonexistent `bid.service.ts`; the llm-logs index comment's "per-run budget sum" is copy-paste. | **ABSORBED(236c04e35) — PARTIAL, HELD in §3** (P4.D341 [the three moves] + P4.D343 + P4.D344 + P4.D360; was ORDERED 2026-10-29). Not absorbed: **P4.D343 Tier 2 item 41** — the `simultaneous` order (the server answers 422 `not_available: storyteller simultaneous`; by the human's 2026-11-04 ruling the SPA's **All at once** renders DISABLED and e2e beat (K) is skipped by name) — and **item 29**, the `.qtap` import's refusals of a bad `turnMode` / `storytellerInviteFilter` / `storytellerBid` (v5 accepts them). Retires to §6 when item 41 lands. |
| `8832cfd20` | 2026-10-28 | deps: SQLCipher driver to better-sqlite3-multiple-ciphers 13 | **DEPENDENCY MOVE + PORT? (one server INFO line) + NO-PORT (CLI / Docker / CI / jest / docs)** (classified 2026-10-29 from the hunks; 24 files +1,492 / −1,776; `4.12.0-dev.1`; tag `4.12.0-dev.1`). **The driver:** root `better-sqlite3` alias `npm:better-sqlite3-multiple-ciphers@^12.11.1` → `^13.0.3` and `packages/quilltap`'s `^12.11.1` → `^13.0.3`; `allowScripts` DENIES the driver's install script (prebuilt Node-API binaries for 8 targets — darwin / linux / linuxmusl / win32 × x64 / arm64 — nothing compiles; Linux glibc ≥ 2.35). **MEASURED from the installed 13.0.3 tree: SQLite3MC `2.4.0` on SQLite `3.53.4`** (v5's `quilltap-sqlite3mc-sys` vendors **2.3.5 / 3.53.2**, the 12.11.1 pair). The `deps/defines.gypi` build flags are BYTE-IDENTICAL between 12.11.1 (npm-cache tarball) and 13.0.3 — `SQLITE_ENABLE_STAT4`, `SQLITE_DQS=0`, `SQLITE_USE_URI=0`, `THREADSAFE=2` etc. unchanged, so the two standing build-difference notes (STAT4 / DQS) stand as they are. NO migration, NO D23 move; v4's commit asserts v12 / v13 files interoperate, and the hunks make no on-disk change (the cipher default stays sqleet; no `cipher=` pragma appears). **Server hunk (the only `lib/` change):** `lib/database/backends/sqlite/client.ts` — after `SQLite database connection established`, a NEW `logDriverVersions(db)`: INFO `SQLCipher driver loaded` `{ driverVersion: <better-sqlite3/package.json version ?? 'unknown'>, sqliteVersion: <select sqlite_version()> ?? 'unknown' }`, DEBUG `Could not read SQLCipher driver versions { error }` on throw; "Parent process only" per its docblock. **CLI (`packages/quilltap/lib/native-modules.js` 238 lines, `bin/quilltap.js`):** `ensureDatabaseNativeModule()` LOADS the prebuild and classifies a failure (`unsupported-platform` / `glibc-too-old` / `load-failed`, with v4's sentences) instead of `readCompiledAbi` + `rebuildNativePackage`; `jest.global-setup.js` loses its ABI heal; `native-rebuild.test.js` DELETED, `native-driver-check.test.js` NEW. **Infra:** Dockerfile / Dockerfile.ci prune the driver to the image's arch; `release.yml` + `build-standalone-tarball.mjs` + `build-push-docker.ts` comments / steps; README glibc floor; v4 CLAUDE.md rule against v13-only driver API (`db.explain()`, `Statement.prototype.toString()`) until the Electron shell ships v13. **Docs:** `docs/CHANGELOG.md` SPLIT — 984 lines of 4.x history moved to a NEW `docs/CHANGELOG_V4.md`; NEW `docs/developer/features/complete/better-sqlite3-multiple-ciphers-v13.md` + `better-sqlite3-multiple-ciphers-v13-shell-notes.md`; ROADMAP line; `dependency-upgrade-safe-batch.md` touched. | **v5 intersection — a RULING owed (the cipher crate):** `crates/quilltap-sqlite3mc-sys/` vendors the **2.3.5** amalgamation "matching what v4 bundles" (CLAUDE.md's standing rule names 2.3.5 / 3.53.2 and `commit.md` §2 names 2.3.5) — v4 now bundles **2.4.0 / 3.53.4**. The crate's own rule says bump its pinned version ONLY when the vendored source changes — this is that case if the human rules to follow v4 (one ~4-min amalgamation recompile on every lane's cold target, then cached; `scripts/cargo-sweep.sh` keeps it). Alternative ruling: HOLD at 2.3.5 and record the pair as a ruled build difference beside STAT4 / DQS (`sqlite-stat4-build-difference`, `sqlite-dqs-build-difference`). Either way CLAUDE.md's cipher paragraph + `commit.md` §2 + `build.rs`'s "use a 2.3.x release to match … @12.11.1" docblock need the new numbers. **The INFO line:** v5's `db/runtime.rs` emits the three `… database connection established` lines (P4.159's boot-hardness family pins them byte-for-byte in `host_boot_hardness`); v4's new line names a Node DRIVER v5 does not have — PORT? as a v5-shaped line (`driverVersion` would be the sys crate / amalgamation version) or NO-PORT; the boot-line census + `host_boot_hardness` carry it either way. **CLI:** NO v5 analog (the 2026-10 `85813ddd2` finding stands: v5 links the amalgamation statically; `quilltap db` reaches core directly; Tier R's `npx quilltap` oracle bytes unchanged for every ported subcommand). **Docker (P4.10):** v5's image never installs the Node driver — NO-PORT. **Pins:** `provider_sdk_version_guard` reads SDK versions only (`openai` / `@openrouter/sdk`), so it does NOT trip on the driver; the real-DB jest oracles (`requireActual` of the cipher driver through the `packages/quilltap/node_modules` symlink, §5.1) will run on 13.0.3 / SQLite 3.53.4 at HEAD's tree and on 12.11.1 only under the pin's own `npm ci --offline` — a sweep from the pin MUST rebuild all three symlink classes. **Mirror:** `docs/v4/CHANGELOG.md` + NEW `docs/v4/CHANGELOG_V4.md` + the two specs + ROADMAP, for the catch-up's help/docs lane. | UNPROCESSED |
| `bf7a457f6` | 2026-10-29 | Fix: filesystem stores faulted in iCloud files, held the startup screen, and leaked a descriptor per file | **PORT (the scanner: `in-cloud` + `filesInCloud` + `emptyScanResult` + ONE new INFO line; the `?action=scan` handler's log key and wire key) + PORT-NEW (the pure `lib/files/cloud-placeholder.ts` pair) + NO-PORT (the boot-scan split, the startup-gate text, the `fs.watch` watcher — surfaces v5 has NEVER had) + help** (classified 2026-10-29 from the hunks; 18 files +1,012 / −117; `4.12.0-dev.2`; found LIVE on the human's V4test instance — a store on an iCloud-synced `~/Documents`, 23,615 of 24,153 files evicted). **Scanner (`lib/mount-index/scanner.ts`):** `ScanResult` gains `filesInCloud: number` (between `filesDeleted` and `chunksCreated`); NEW exported `emptyScanResult(mountPointId, errors = [])` replaces the three hand-built zero results (scanner + scan-runner ×2); `ProcessFileOutcome` gains `{ status: 'in-cloud' }`; `processMountFile` now `fs.stat`s BEFORE `computeSha256` (the stat MOVED up from after the sha/unchanged check — so an unsupported type still returns before any stat, and `in-cloud` returns before any read; the later `const stat` line is deleted) and returns `in-cloud` when `isEvictedCloudPlaceholder(stat)`; `scanMountPoint`'s switch counts `filesInCloud++` and `continue`s (the path IS added to `seenPaths` first, so the row survives the stale sweep); the completion INFO gains `filesInCloud`; a NEW INFO after it, only when `filesInCloud > 0`: `Left cloud-evicted files unread; they are indexed once downloaded` `{ mountPointId, name, filesInCloud }`; `walkDirectory` becomes EXPORTED (for the watcher). **`lib/files/cloud-placeholder.ts` (NEW, pure):** `STAT_BLOCK_BYTES = 512`, `AllocationStat = { size, blocks }`, `isNotFullyMaterialized(stat)` = `size > 0 && blocks * 512 < size` (MOVED out of `lib/startup/materialize-cloud-files.ts`, which now imports it), `isEvictedCloudPlaceholder(stat, platform = process.platform)` = `(darwin | win32) && size > 0 && blocks === 0`. **Scan runner (`scan-runner.ts`):** `scanAllMountPoints(options: { only?: 'database' | 'filesystem'; reportStartupProgress?: boolean = true })` filters `findEnabled()` by `mountType === 'database'`, makes every `startupProgress` call optional-chained, adds `only` to both INFO lines (`'all'` when omitted) + `totalInCloud` to the completion line, and a DEBUG `No enabled mount points to scan { only }` on the empty case. **Boot (`instrumentation.ts` PHASE 3.3):** `mountScanSettled = scanAllMountPoints({ only: 'database' })` inside the gate; then, NOT awaited by the gate, `.then(() => scanAllMountPoints({ only: 'filesystem', reportStartupProgress: false }))` with a WARN `Filesystem document store scan failed`; `startup-state.ts` docblocks only. **Route (`[id]/route.ts` `handleScan`):** the `Mount point scan completed`-class INFO gains `filesInCloud: scanResult.filesInCloud` (between `filesDeleted` and `chunksCreated`); the response is the whole `scanResult`, so the wire JSON gains the key. **Watcher (`lib/mount-index/watcher.ts`):** filesystem stores watch through Node's recursive `fs.watch` (one FSEvents stream per store) with `handleNativeEvent` resolving each debounced path against the disk (file → index; gone → unlink, + `removeIndexedBeneath` after a rename when the path has no file type; directory arrived by rename → `walkDirectory` + index each), `pendingRenames` per watcher, `in-cloud` → DEBUG `Mount watcher left a cloud-evicted file unread`; `QUILLTAP_WATCHER_POLLING=1` keeps chokidar polling (now ignoring non-indexable files) and is the fallback when `fs.watch` throws (WARN `Native recursive watch unavailable, falling back to polling`). **Help:** `help/mount-points.md` +10 / −2 — a NEW `#### Files Resting in the Cloud` section, the "startup sweep … runs *after* the house has opened its doors" paragraph, the polling paragraph reworded. Tests: `scan-runner.test.ts` +86 (the `only` trio + the two progress rows), NEW `cloud-placeholder.test.ts`, `scanner-cloud-placeholder.test.ts`, `watcher-native.test.ts`. | **v5 intersection — PORT on the ported scanner:** `crates/quilltap-core/src/services/mount_index/scanner.rs` (`ScanResult` → `to_json` key order; `ProcessFileOutcome`; `process_mount_file`'s stat-before-hash + `In-cloud`; `scan_mount_point`'s count + the INFO line; `empty_scan_result`); `api/mount_files.rs::mount_scan` (the `Request::MountScan` handler — the route INFO's `filesInCloud` key + the wire `scanResult` gaining the key — a SHARED CONTRACT with the SPA's `scriptorium.api.ts` scan-result type, P4.6-series Scriptorium); the mount-index tier-2 families that compare scan results / logs (`danger_scan_tier2_equivalence`, `doc_mount_*_tier2_equivalence`, `mount_link_groups_equivalence`, `doc_mount_fallback_sites_census` — the census counts `process_mount_file` sites); `apps/web/src/app/screens/scriptorium/scriptorium.api.ts:69` (`filesDeleted` … add `filesInCloud`) + whichever Scriptorium toast renders the scan summary. **PORT-NEW:** the two pure predicates as a tier-1 exact family (a `cloud_placeholder` module under `services/mount_index/` or `files/`; v5 has no `materialize-cloud-files` boot step — `grep` finds no analog in `quilltap-host`). ⚠ **`blocks` on Windows:** v4 relies on libuv reporting allocation size; Rust's `std::os::unix::fs::MetadataExt::blocks` is unix-only — the Windows arm needs its own reading (or a ruled `false` until Tauri-Windows), record it. **NO-PORT, by standing deferral:** v5 has NO boot-time `scanAllMountPoints` (the host's boot runs only the PHASE 3.3b orphan reaper; `grep scan_all_mount_points crates/` = 0), NO startup-progress gate, and NO mount watcher (`phase-4.md`'s "chokidar-equivalent fs watcher INCLUDING the db-store-event emitter chain (host cadence)" deferral, named since P4.6p/P4.6v) — so the `only` / `reportStartupProgress` options, the two-pass boot, the watcher's `handleNativeEvent` and polling fallback have no v5 home; record them in the deferral's ledger, do not stub them. **Help:** `help/mount-points.md` re-vendors (the `p4.9i2` bank; v5 serves `help/` from the binary). **Dogfood:** a Friday-copy store on a local (non-iCloud) folder scans identically before and after; the `in-cloud` arm is provable only with a planted dataless file (macOS `blocks === 0` needs a real evicted placeholder — `brctl evict`) or by the pure family. | UNPROCESSED |
| `db4a76e39` | 2026-10-29 | deps: drop chokidar; files watcher on fs.watch, mount polling walks the store | **NO-PORT? (both watchers are NO-COUNTERPART in v5) + PORT (one About-credits line)** (classified 2026-10-29 from the hunks; 13 files +505 / −176; `4.12.0-dev.3`; parent `bf7a457f6` — the morning's recorded dirt, committed). `lib/file-storage/watcher.ts` (+209 / −… : the instance `files/` watcher moved onto recursive `fs.watch`, `handleNativeEvent` resolving each debounced event against the disk, folder arrival / departure by rename, a pending-unlink cancel on a sha-matched move, symlinks skipped, `toStorageKey` normalising to `/`), `lib/mount-index/watcher.ts` (the polling backend rewritten as a 2 s walker over `walkDirectory` + `detectFileType`, diffing mtime / size, no chokidar), `package.json` / `packages/quilltap/package.json` (chokidar dropped), `app/about/AboutView.tsx` (`Filesystem & Archives: tar, yauzl, semver`), a comment in the mount-points route, two tests + a NEW `watcher-native.test.ts`. | **v5 intersection:** NONE in code — v5 ships NO file-storage watcher (`folders_chokepoint_wiring_guard::the_no_counterpart_rows_are_recorded`) and NO mount watcher (the standing "chokidar-equivalent watcher (host cadence)" deferral); only the About credits line (`about-page.ts:435-459`, the P4.D359 byte port of `a4945d8d9`'s list) gains this one-word drop — a one-line rider for whichever lane next touches it; `docs/v4/` mirror. | UNPROCESSED |
| `f56965ef9` | 2026-10-29 | chore(deps): minor npm update to root repo | **NO-PORT? (dependency stamps) — but it MOVES the two SDK versions the guard reads** (classified 2026-10-29 from the hunks; 10 files +71 / −64; `dev.4`). `package.json`: `openai` ^7.45.0 → **^7.45.1**, `@openrouter/sdk` ^1.5.9 → **^1.5.21** (+ its `allowScripts` key), `@tanstack/react-virtual` 3.14.24, `@types/yauzl` moved; seven `plugins/dist/*/index.js` bundles rebuilt (one line each). | **v5 intersection:** `provider_sdk_version_guard.rs:128/:132` (`INSTALLED_ROOT_OPENAI_SDK` / `INSTALLED_ROOT_OPENROUTER_SDK`) — at the `236c04e35` pin P4.D360 sets 7.45.0 / 1.5.9; ANY later pin moves them again (7.45.1 / 1.5.21). No code. | UNPROCESSED |
| `3936fdf65` | 2026-10-29 | deps: croner 11, jest-fetch-mock 4.2; test-only packages to devDependencies | **NO-PORT? (dependency stamps)** (classified 2026-10-29; 5 files +36 / −33; `dev.5`). `croner` ^10 → ^11 (v4's cron scheduler — v5's cadence is the host's own; no `croner` twin), `jest-fetch-mock` 3.2 → 4.2 moved to devDependencies with `@types/yauzl`. | **v5 intersection:** none (v4's jest mocks are oracle-side only; no oracle imports `jest-fetch-mock` directly — confirm at the next census). | UNPROCESSED |
| `eb90eb466` | 2026-10-29 | deps: Lexical 0.53 | **NO-PORT? (client dependency stamps)** (classified 2026-10-29; 5 files +219 / −211; `dev.6`). The nine `lexical` / `@lexical/*` packages 0.52 → 0.53; no source hunk. | **v5 intersection:** none — v5's composer is ProseMirror (D17). | UNPROCESSED |
| `7dedfa067` | 2026-10-29 | deps: ESLint 10 | **NO-PORT? (tooling)** (classified 2026-10-29; 5 files +201 / −340; `dev.7`). `eslint` 9.39 → 10.13 + three `overrides` entries for the react / jsx-a11y / import plugins; no source hunk. | **v5 intersection:** none. | UNPROCESSED |
| `3e4f9dea9` | 2026-10-29 | deps: TypeScript 6 | **NO-PORT? (tooling + the four `packages/*` manifests)** (classified 2026-10-29; 17 files +754 / −1,225; `dev.8`). `typescript` 5.9 → 6.0.3 at the root and in `create-quilltap-theme` / `plugin-types` / `plugin-utils` / `theme-storybook` (+ `css-modules.d.ts`, `version.generated.ts`); no `lib/` / `app/` hunk. | **v5 intersection:** none for v5's Rust / Angular; ⚠ the ORACLES run under v4's tsx — a TypeScript 6 `node_modules` at a post-`3e4f9dea9` pin is what `npm ci` installs there (tsx strips types; measure that every oracle still loads at that pin). | UNPROCESSED |
| `9f761e157` | 2026-10-29 | deps: bring packages and plugins up to root dependency levels | **NO-PORT? (every plugin manifest + bundle rebuilt; `packages/*` manifests)** (classified 2026-10-29 from the stat; 56 files +83,965 / −40,295 — nearly all `plugins/dist/*/index.js`; `dev.8`, no root stamp). Each of the fifteen plugins' `package.json` moves `@quilltap/plugin-types` ^2.8.0 / ^2.9.0 → **^2.9.1**, `@quilltap/plugin-utils` ^2.7.1 → **^2.7.2**, `openai` ^7.23.0 → **^7.45.1**; every `manifest.json` version +1 (anthropic 1.0.63, curl, deepseek, google, grok, mcp, nanogpt, ollama, openai, openrouter, search-serper, z-ai, …); `qtap-plugin-anthropic/provider.ts` +24 / −… (measure — a bundle-level change with a `.ts` hunk); `packages/quilltap` `sharp` 0.35.5. | **v5 intersection:** v5's providers are declarative manifests (no JS plugins); the plugin `node_modules` symlink class (§5.1) now points at trees built for these versions — at the `236c04e35` pin the plugin dirs' deps are inert type-only imports as before; the `anthropic/provider.ts` hunk needs a read at the next catch-up (it precedes `544c7ab1e`'s rewrite of the same file). | UNPROCESSED |
| `544c7ab1e` | 2026-10-29 | anthropic: move to API version 2026-10-21; drop temperature and top_p | **PORT (the Anthropic request builder + the connection-profiles repository + the profile editor) WITH A NEW DATA-REWRITE MIGRATION — `strip-anthropic-sampling-parameters-v1` (main partition; rewrites `connection_profiles.parameters` for Anthropic profiles; NO column move)** (classified 2026-10-29 from the hunks; 32 files +722 / −250; prod 22 files +251 / −115; `dev.9`; plugin-types **2.10.0**, plugin-utils 2.7.3, the Anthropic plugin 1.0.64). **Plugin (`plugins/dist/qtap-plugin-anthropic/provider.ts` +95 / −…, `index.ts`):** the `anthropic-version` pin `2023-06-01` DROPPED — the SDK default `2026-10-21` applies, which REJECTS `temperature` / `top_p` / `top_k` on every model and document blocks placed directly in assistant messages; the plugin sends NO sampling parameters on any model; assistant-turn documents move into a user turn placed just before it; the thinking shape unchanged (adaptive on new-gen, `budget_tokens` on older). **plugin-types:** NEW `ProviderCapabilities.samplingParameters?: boolean` (the Anthropic plugin declares `false`). **`lib/plugins/provider-validation.ts`:** NEW `acceptsSamplingParameters(provider)` (exported through `lib/plugins/index.ts`). **`lib/llm/sampling-params.ts`:** NEW `SAMPLING_PARAMETER_KEYS = ['temperature','top_p','topP','top_k','topK']` + `withoutSamplingParameters(params) → {parameters, removed}` (`max_tokens` and provider options kept). **`connection-profiles.repository.ts`:** `sanitizeParametersForProvider(provider, parameters, profileId?)` on `create` (when `data.parameters`) and `update` (when `parameters` OR `provider` is in the patch — a provider change re-checks the stored blob through `findById`), DEBUG `Stripped sampling parameters the provider does not accept {context:'ConnectionProfilesRepository', profileId, provider, removed}` — covers editor, auto-configure, import and restore. **Migration** `strip-anthropic-sampling-parameters-v1` (`migrations/scripts/index.ts` +4; a `prettify.ts` label) cleans existing Anthropic profiles. **SPA:** `components/settings/connection-profiles/types.ts` +2; the editor hides Temperature / Top P for such providers and omits them from save and test-message; the card shows only Max Tokens. `help/connection-profiles.md` +4 / −2. Tests: a 157-line integration test + a 41-line unit test. | **v5 intersection — PORT:** the Anthropic request builder (P4.11 / P4.13 — `providers/anthropic/*`: the `anthropic-version` header value, the sampling keys OMITTED, the assistant-document relocation), the provider manifest's capability (`samplingParameters: false` on the declarative Anthropic manifest + the `acceptsSamplingParameters` twin), `db/connection_profiles.rs` create / update (the strip + the DEBUG line; import / restore / auto-configure ride the repository), the migration as a STAMPED data-rewrite boot ensure (the P4.D265 class — it rewrites rows, so it stamps `migrations_state`; `restore_vintage_state`'s boot-align list), the Settings SPA profile editor / card (P4.6e) + the wire's capability field, `help/connection-profiles.md`. ⚠ LIVE-DATA (§1): until ported, a v5 write of an Anthropic profile puts the sampling keys BACK (ping-pong), and v5 keeps sending them under the old header. The oracle side: `provider_sdk_version_guard` unchanged (no SDK move here); the Anthropic request-builder families (`anthropic_*_request_equivalence`, the stream / non-stream builders) redden at a post-`544c7ab1e` pin; `plugin-types` 2.10.0 is a type-only import for the oracles. | UNPROCESSED |
| `29ba4d3f8` | 2026-10-29 | ui: opaque dropdowns; open menus rise above their rows | **PORT? (SPA / CSS only)** (classified 2026-11-04 from the hunks; 23 files +275 / −100; `dev.10`). NEW `qt-bg-popover` utility (`--qt-popover-bg`) in `qt-components/_utilities.css`; ten dropdowns / menus switch to it; `_surfaces.css`: a `qt-card` holding an open popup gets `position: relative; z-index: 20; translate: none`; NEW `hooks/useFixedMenu.ts` (viewport-fixed menu that flips upward, closes on outside scroll / resize) under the wardrobe, inventory, folder and scenario rows; roleplay-template and task-details dialogs on `qt-dialog`. No `lib/` / `app/api/` hunk. | **v5 intersection:** the SPA's shared `qt-components` CSS port (the P4.D357 WCAG/tokens lane's file family) and the four row menus (wardrobe / Inventory / folder / scenario rows — P4.D285 and the `322a5488e` round's Inventory SPA). Theme packs: Art Deco / Great Estate / Madman's Box clear `.bg-background` (why the utility exists). | UNPROCESSED |
| `3c41cf3da` | 2026-10-29 | salon: compact the header breadcrumb for a crowded cast | **PORT-NEW (SPA)** (classified 2026-11-04 from the hunks; 12 files +433 / −110; `dev.11`). NEW `components/chat/ChatHeaderBreadcrumb.tsx` (283) — project / character links step down full name → abbreviation → avatar only when the row doesn't fit, Quilltap tooltips; `SalonView.tsx` −153 net; `qt-page-toolbar` left section sizing in `_layout.css`; `help/chats.md` +4. No server hunk. | **v5 intersection:** the Salon header / page toolbar (P4.6a Salon vertical; the workspace tab toolbar `p4.9j`); `help/chats.md` re-vendor. | UNPROCESSED |
| `4f5047241` | 2026-10-29 | search: look in the open Salon chat first; ?msg= links scroll to the message | **PORT-NEW (SPA) + PORT (`?msg=` deep links)** (classified 2026-11-04 from the hunks; 27 files +943 / −146; `dev.12`). Client-side "This conversation" matches over the visible transcript (NEW `lib/chat/conversation-search.ts` 134 — markdown-stripped, case-insensitive, newest first, TOOL rows skipped; it is a CLIENT module under `lib/`), the CSS Custom Highlight API painting, `useSearchHighlight` / `useAutoScroll`; `lib/chat/message-navigation.ts` reworked; `/salon/<id>?msg=<messageId>` now READ by every entry (the workspace link interceptor, `useWorkspaceNavigate`, `WorkspaceIntent`, `page.tsx`); a `conversationSearch` slot on the page-toolbar context; `help/search.md` +29. No `app/api/` hunk. | **v5 intersection:** the SPA search bar / dialog (P4.6-era global search + the FTS5 message search, `f45a517a9`'s port), the Salon transcript scroller, the workspace link interception (`p4.9j`). `help/search.md` re-vendor. | UNPROCESSED |
| `902c6efe9` | 2026-10-29 | search: keep Staff messages and announcement bubbles out of global search | **PORT (FTS5 eligibility + the search filter) WITH A NEW INDEX-REBUILD MIGRATION `reindex-chat-message-fts-character-only-v1` — NOT a D23 move (the FTS5 objects are not generateDDL's; no column)** (classified 2026-11-04 from the hunks; 18 files +325 / −30; `dev.13`; **`Fixes #121`** — v4-ORIGINATED ("Original to v4", v5 status "Not assessed"), NOT a convergence). `lib/database/backends/sqlite/chat-message-fts.ts`: `chatMessageFtsEligibilitySql` gains `AND "systemSender" IS NULL AND "customAnnouncer" IS NULL`; the INSERT trigger's `WHEN` is now that function; NEW `staleChatMessageFtsTriggers(db)` compares each trigger's `sqlite_master.sql` against the expected text and `ensureChatMessageFtsSchema` DROPs + recreates stale ones (DEBUG `Replacing outdated chat message FTS triggers {stale}`). `chats-search.ops.ts`: the FTS query adds the eligibility SQL; the fallback filter is `isCharacterAuthoredMessage(msg)`. The migration (`shouldRun`: missing objects OR stale triggers OR eligible ≠ indexed) rebuilds with two INFO lines (`Rebuilding chat message search index without Staff messages`, `Chat message search index rebuilt {scanned, indexed, total, dropped}`) and progress; `create-chat-message-fts`'s `dependsOn` grows two ids; `prettify.ts` label; `docs/developer/DDL.md`, `help/search.md`. | **v5 intersection — PORT:** `db/chat_message_fts.rs` (the eligibility SQL + trigger text + a stale-trigger replace), `db/chat_message_fts_reconcile.rs` (the boot reconcile), `db/chats_search.rs` (both arms), the migration as a STAMPED boot ensure + `restore_vintage_state`'s boot-align entry; the FTS families (`chat_message_fts_*`, `chats_search_*`) redden at a post-pin regen. ⚠ **LIVE-DATA (§1): a PING-PONG REBUILD class until ported** — v5's reconcile counts eligibility by the OLD filter, so on an instance v4 trimmed it sees eligible ≠ indexed and REBUILDS (re-indexing every Staff row); v4's next boot rebuilds again. Measured from v5's source (`chat_message_fts_reconcile.rs:144`), not run. | UNPROCESSED |
| `d03eb2d63` | 2026-10-29 | Docs: Deus Ex Machina spec (established facts in the Salon) (#122) | **NO-PORT? — docs only, VERIFIED** (2 files +345: NEW `docs/developer/features/deus-ex-machina.md` + a ROADMAP line; no stamp). Superseded in substance by `db6190b44`, which edits the spec as-built. | **v5 intersection:** the `docs/v4/` mirror only. | UNPROCESSED |
| `db6190b44` | 2026-10-29 | salon: Deus Ex Machina — established facts | **PORT-NEW (Salon "established facts") + PORT (context builder, extraction prompts, cheap-LLM transcripts, Courier delta) WITH THE TWENTY-SECOND D23 MOVE — `chat_messages."isEstablishedFact" INTEGER DEFAULT NULL` (`add-established-fact-field-v1`, main partition, additive, NO data rewrite)** (classified 2026-11-04 from the hunks; 94 files +2,500 / −93; `dev.15`). **Schema:** `ChatMessageSchema.isEstablishedFact: z.boolean().nullable().optional()` sits BETWEEN `isSilentMessage` and `systemSender` — so TWO shapes (generateDDL's schema-order column for a fresh table vs the migration's APPENDED one). **Server:** NEW `lib/chat/established-fact.ts` (the one recogniser; the frames, the reminder header, the transcript rule — frames applied at READ time, never stored); the operator's USER row carries the flag (`recordOperatorPost`), the machine kind is a public Host row `systemSender='host'`, `systemKind='established-fact'`, `participantId` NULL; NEW background job `established-fact-memories` (`handlers/established-fact-memories.ts` 164, `queue-service.ts` +43, a job type + activity kind + job topic); NEW `lib/chat/context/established-fact-reminder.ts` (a trailing reminder in `turn-trailing.ts`); `context-manager.ts` / `context-summary.ts` / `message-attribution.ts` / `speaker-names.ts` / `transcript-projection.ts`; `context-builder.service.ts` +36, `orchestrator.service.ts` +30, `courier-transport.service.ts` +18, `operator-post.service.ts`; the four extraction prompts (`shared.ts` +11, `self.ts`, `other.ts`, `episode.ts`) + `window.ts` + `candidate.ts`; `cheap-llm-tasks/chat-tasks.ts`; NEW `lib/services/announcer/writer.ts` +108 and `in-scene-voiced.ts`; the announcement action + `schemas.ts` (a fact tab in Insert Announcement); a `/deus` slash command (`registry.ts`); `public/schemas/qtap-export.schema.json` +2. **SPA:** the composer (`/deus`, Cmd/Ctrl+Shift+Enter one-line fact, `KeyboardPlugin`), `MessageRow`, `system-message-labels`, `InsertAnnouncementDialog`, `_chat.css` +54. **Help:** NEW `help/deus-ex-machina.md` (67); `inform.md`, `insert-announcement.md`, `character-system-transparency.md`. | **v5 intersection — PORT:** the D23 re-dump (`fresh_schema.json`) + a column-gated UNSTAMPED boot ensure with both shapes + `restore_vintage_state`'s boot-align; the context builder / turn-trailing chain (the round's P4.D347 lane's file family — referent ledger, absent mentions — and P4.D254's trailing inform section), the extraction prompts (P4.D296; the round's P4.D342 rubric lane edits the same prompts), the Courier transport, the operator post, the announcer writer (the Insert Announcement port), the job runner (a new handler kind), the slash-command registry (P4.D308), the `.qtap` export schema, the Salon composer (the round's P4.D352 composer lane). | UNPROCESSED |
| `671c59a53` | 2026-10-29 | salon: one predicate for never-collapsed announcements | **PORT (SPA — a v4 display defect)** (classified 2026-11-04 from the hunks; 8 files +84 / −23; `dev.16`). `isAlwaysExpandedAnnouncement` in `announcement-render-items.ts` now the one rule (Carina, established facts, mail deliveries `suparna/mail-delivery`, Pascal results `pascal/custom-tool-result`); `isCollapsedAnnouncement` exported and passed to `MessageRow` (the message list's own exemption check removed). | **v5 intersection:** the Salon announcement chip grouping (the Staff / announcement render items, P4.6-era Salon). Measure whether v5 has the split check. | UNPROCESSED |
| `df8d4d70a` | 2026-10-30 | salon: Deus Ex Machina toggle on the composer portrait; Quilltap tooltips in the composer | **PORT-NEW (SPA, rides `db6190b44`) + PORT? (composer tooltips / aria-labels)** (classified 2026-11-04 from the hunks; 26 files +667 / −369; `dev.17`). `SpeakingAsAvatar` gains the established-fact toggle (`aria-pressed`, `qt-chat-established-fact-toggle`); NEW `deus-ex-machina` icon (`public/images/icons/`, the Madman's Box variant + `theme.json` 1.1.10, the icon registry); native `title` tooltips in `ChatComposer`, `ComposerGutterTools`, `FormattingToolbar`, `RngDropdown`, `CustomToolsButton`, `PendingInformChips` replaced with `Tooltip`, icon-only buttons gain `aria-label`; initial-only portraits get a min width. | **v5 intersection:** the Salon composer + formatting toolbar (the round's P4.D352 composer lane and P4.D358's accessible-names lane touch the same controls), the icon set and the Madman's Box pack (P4.D357). | UNPROCESSED |
| `fd165fce0` | 2026-10-30 | pascal: custom-tool presets follow the tool walk | **PORT (Pascal run presets) + PORT-NEW (`?action=presets` + the Run dialog's store labels / "Save into")** (classified 2026-11-04 from the hunks; 14 files +598 / −102; `dev.18`). NEW `lib/pascal/tool-preset-pool.ts` (`resolveToolPresetPool` — presets `Tools/{tool}.{preset}.settings.json` found on the same walk as definitions: character vault, group stores, project stores, Quilltap General, nearest first, a nearer name shadowing a farther one); `custom-tools.ts`: shared `listToolsFolderFiles` + exported `orderedMounts`; `GET /api/v1/chats/[id]/custom-tools?action=presets` (+76: winning presets with the stores they shadow, every walk store as a save destination, stores that would not list); `CustomToolRunDialog.tsx` +175; query key. | **v5 intersection — PORT:** the Pascal custom-tool walk + presets (P4.D305 / P4.D307 run dialog; the vault-only custom tools of the `322a5488e` round) and the round's P4.D345 / P4.D346 intervention lanes (same route, same dialog). | UNPROCESSED |
| `9eab8d2da` | 2026-10-30 | salon: raised-hand status chip; gesture-only answers count as a yield | **PORT (turn manager + the raise-hand SSE) + PORT-NEW (SPA chip)** (classified 2026-11-04 from the hunks; 18 files +382 / −33; `dev.19`). `turn-manager/yield-signal.ts`: NEW `isGestureOnlyReply(text)` (≤ 280 chars, no blank line, no spoken quote, peeled `*` / `_` / `( )` wrappers, a letter inside) → `{yielded: true, cleaned, inferred: true}`; the raise-hand note's model-visible text gains "A reply that is only a gesture, with no words spoken aloud, is also taken as yielding."; NEW `encodeHandAskedEvent` → `data: {"handAsked":true,participantId,characterName}`; `handAnswered` gains `inferred`; `turn-outcome.service.ts`. SPA: NEW `RaisedHandChip.tsx`, `useSSEStreaming` +90. `help/interrupting.md`. | **v5 intersection — PORT:** raise a hand / stop (the `322a5488e` round's interrupts lane — `turn_manager` yield signal + the streaming frames), the Salon SSE wire (a NEW frame), the SPA composer chip. Tier-1 candidate: `isGestureOnlyReply` is a pure function. | UNPROCESSED |
| `b2b783b47` | 2026-10-31 | item-browser: shared folder browser for the Wardrobe and the Inventory | **PORT? (SPA refactor of ported browsers) + PORT-NEW (shared folder browser)** (classified 2026-11-04 from the hunks; 32 files +2,214 / −1,618; `dev.20`). NEW `components/item-browser/*` (`FolderBrowserShell`, `ItemActionMenu`, `ItemFolderField`, `ContainerSelect`, `useFolderBrowserState`), `lib/folder-browser/view.ts` (142, client), `lib/hooks/use-container-catalogue.ts`; `WardrobeBrowser` / `InventoryBrowser` and their dialogs rebuilt on it; `lib/wardrobe/folder-view.ts` −132 (moved to the shared view); NEW spec `docs/developer/features/folder-browser.md`. | **v5 intersection:** the SPA wardrobe folder browser (P4.D285) and the Inventory browser (the `322a5488e` round's Inventory SPA) — a parity port, not a wire move. `help/inventory.md`, `help/wardrobe.md`. | UNPROCESSED |
| `a0ed5ecad` | 2026-10-31 | item-browser: shared server folder layer for the Wardrobe and the Inventory | **PORT (wire: a route REMOVED) + refactor ("No behaviour change" — verify)** (classified 2026-11-04 from the hunks; 21 files +891 / −689; `dev.21`). NEW `lib/mount-index/item-folder-location.ts` (`ItemFolderLocation`, `locationKey`, generic folder create / rename / delete / move-items keyed by `ItemFolderKind`), `item-folder-routes.ts` (`itemFolderActionHandlers` for both factories); `lib/wardrobe/transfers.ts` (413 — the transfer logic out of its route, typed refusals); **`GET /api/v1/wardrobe/transfers` REMOVED** ("no callers"); `wardrobe.types.ts` −9. | **v5 intersection — PORT:** v5 STILL serves `GET /api/v1/wardrobe/transfers` (`quilltap-web/src/wardrobe_routes.rs:457`, `api/types.rs:2788`) — a route v4 no longer has; the folder actions (P4.D281) + Inventory folder actions (`322a5488e` round). Measure the transfer refusal texts against the new typed refusals. | UNPROCESSED |
| `aec626eff` | 2026-10-31 | mount-points: document-store API for the folder browser | **PORT-NEW (mount-point API) + PORT (the files listing wire + realtime topics)** (classified 2026-11-04 from the hunks; 25 files +1,037 / −72; `dev.22`). `GET …/files` returns `DocMountBrowserEntry` rows (`listForBrowser` — no `extractedText`, plus `linkGroupId` / `linkCount`); per-file GET `?action=links` and `?action=render-html` (`.docx` through mammoth, unsafe link targets neutralised — NEW `converters/docx-html.ts`); NEW `unlink-file` (`clearLinkGroup`), `copy-folder`, cross-store `move-folder` (`destMountPointId`), recursive `delete-folder` (`folder-ops.ts` +258, undoing a failed copy / move); `doc-mount-file-links.repository.ts` +104; `publishMountChange` scopes the `mountPoints` topic per store (`realtime.ts`, `topic-map.ts`, `realtime.types.ts`). | **v5 intersection — PORT:** the mount-points routes + `db/doc_mount_*` (the Scriptorium vertical incl. the D18 bespoke file manager), the realtime invalidation subsystem's topic map, the round's P4.D354 mount-index lane (same files). `mammoth` is a v4 dependency v5 has no twin for — a dependency ruling for `render-html`. | UNPROCESSED |
| `c34986421` | 2026-10-31 | scriptorium: folder browser for document stores | **PORT-NEW (SPA — the Scriptorium store browser) + PORT (filesystem folder move / delete keep `doc_mount_folders` in step) + a dependency REMOVED** (classified 2026-11-04 from the hunks; 54 files +2,237 / −2,952; `dev.23`). The store detail page browses on the shared folder browser (NEW `components/document-browser/*`, viewers for images / PDF / Markdown / text / `.docx`, `FilePreviewDocx`); `FileTable` and the whole SVAR file-manager beta DELETED, **`@svar-ui/react-filemanager` REMOVED**; `ItemFolderPickerDialog` → `DestinationPickerDialog` (link mode, store chooser); `lib/mount-index/folder-ops.ts` +33 (folder rows follow a filesystem move / delete); NEW `lib/folder-browser/document-actions.ts`, `preview-kind.ts`; `AboutView.tsx` credits (SVAR dropped); `help/mount-points.md` +30. | **v5 intersection:** the Scriptorium SPA (v5 never ported SVAR — the D18 bespoke file manager; this converges v4 toward a browser of its own, so compare shapes rather than port the SVAR removal), `db/doc_mount_folders` (the folder-op server half — PORT), the About credits list (`about-page.ts`, P4.D359's byte port). | UNPROCESSED |
| `5efe53ea7` | 2026-10-31 | document-mode: folder browser in the picker, read-only viewers | **PORT (Document Mode open / read / write refuse non-text) + PORT-NEW (SPA picker)** (classified 2026-11-04 from the hunks; 24 files +565 / −385; `dev.24`). `lib/documents/operator-doc-actions.ts` +68: store files that are not text (images, PDFs, `.docx`, binaries) — `open-document` / `read-document` return a VIEWER TARGET instead of reading bytes as text, `write-document` REFUSES them; the chat documents action + `documents` route one line each; SPA: the Open Document picker on the folder browser (pick mode), `DocumentViewerPane`; `BaseModal` ignores an outside click / Escape belonging to a layered dialog; `help/scriptorium.md`. | **v5 intersection — PORT:** Document Mode's operator actions (P4.6-era Document Mode vertical, the chat `documents` action and `/api/v1/documents`), the SPA picker + modal stacking. | UNPROCESSED |
| `a299de9f5` | 2026-10-31 | docs: close out the folder browser; retire the SVAR specs | **NO-PORT? — docs only, VERIFIED** (11 files +65 / −23; `dev.24`): `folder-browser.md` + two SVAR specs moved to `features/complete/`, `API.md`, `FEATURES.md`, `DEAD-CODE-REPORT.md`. | **v5 intersection:** the `docs/v4/` mirror (three moved paths). | UNPROCESSED |
| `c17033f04` | 2026-10-31 | document-mode: store browser for picker libraries; uploaded text and outside edits | **PORT (Document Mode's General library + blob text editing + the accessible-stores wire) + PORT? (mount watcher realtime hint — v5 has NO mount watcher)** (classified 2026-11-04 from the hunks; 29 files +502 / −557; `dev.26`). Document Mode's General library opens Quilltap General (`generalLibrary` returned by `accessible-stores`; the old `files/_general/` resolution, which "no longer exists", failed every pick); text files stored as blobs in a database store open in the editor (`readEditableDocument` → `readDatabaseBlobAsDocument`, `database-store.ts` +30), the first save turns them into documents, unsavable text kinds open read-only; `projects/[id]/mount-points` route +11; `FileBrowser`'s mount mode retired (NEW `ProjectStoreBrowser`); `lib/mount-index/watcher.ts` +28 (a debounced realtime hint on outside changes); orphan `useMountPointBlobUpload` deleted. | **v5 intersection — PORT:** Document Mode (`operator-doc-actions` / the chat documents action / accessible-stores), `db/doc_mount_*` blob reads, the project Files card (#135's fix, P4.6l), the attachment picker. NO-COUNTERPART: the watcher hint. | UNPROCESSED |
| `ba6c8fe0f` | 2026-11-01 | prompt-trust: retire the slips ledger, record repairs ("the probation") | **PORT — model-visible prompt text across generators, memory extraction, consolidation, recall tags and the sample system prompts; NO D23 move, NO route** (classified 2026-11-04 from the hunks; 52 files +958 / −69; `dev.27`; NEW spec `prompt-trust-the-probation.md`). `lib/services/character-field-semantics.ts` (`COMPANION_TRUST_DISPOSITION`, `TRUST_SAFEGUARDS_DIRECTION` gains a sixth rule), `context-builder.service.ts` +12 (`GROUP_SCENE_DISCIPLINE` scoped to the cast), extraction `shared.ts` +23 (`REPAIRS_INSTRUCTION_BLOCK`) / `self.ts` / `other.ts` / `episode.ts`, `memory/recall/tags.ts` +33 (repair anchors), `consolidation-tasks.ts` +30, `consolidation-plan.ts` +29, `consolidation.ts`, `maintenance/consolidate.ts`; **`qtap-plugin-default-system-prompts` 1.1.30 — 21 sample prompts edited**; four help pages. | **v5 intersection — PORT:** the generators (P4.9k wizard / optimizer), the context builder (the round's P4.D347 lane), extraction prompts (P4.D296; the round's P4.D342 importance-rubric lane edits `shared.ts` too), recall tags (P4.D291), consolidation (P4.D271 / P4.D297), and v5's embedded sample prompts (`services/builtin_prompt_templates.json`). Prompt-byte families redden at a post-pin regen. | UNPROCESSED |
| `d439f6dc9` | 2026-11-01 | connection-profiles: cap images sent per request (default 4) | **PORT + PORT-NEW WITH THE TWENTY-THIRD D23 MOVE — `connection_profiles."maxImagesPerRequest" INTEGER DEFAULT 4` (`add-profile-max-images-per-request-v1`, main, additive, plus an `UPDATE … SET 4 WHERE NULL` backfill — a no-op on rows ADD COLUMN filled, so NOT a data rewrite of substance) + an SDK move** (classified 2026-11-04 from the hunks; 25 files +4,280 / −3,465 — 6,606 of them a re-vendored `public/pdf.worker.mjs`; `dev.28`). **Schema:** `maxImagesPerRequest: z.number().int().min(1).max(50).default(4)` between `supportsImageUpload` and `tags` — TWO shapes (schema order vs the migration's append; `DDL.md` lists it last). `resolveMaxImagesPerRequest()` (`connection-profile-utils.ts` +34; legacy fields); `adaptMessagesForProfile` enforces the cap, describing overflow (newest kept); the Lantern walk spends it; failover understudies re-adapted; both profile routes validate; restore / import seed; export schema +10; the profile editor field; `help/connection-profiles.md`. **`openai` ^7.45.1 → ^7.47.0, `@openrouter/sdk` ^1.5.21 → ^1.5.33** ride this commit. | **v5 intersection — PORT:** `fresh_schema.json` re-dump + an UNSTAMPED column-gated ensure (its backfill only fills NULLs) + boot-align; `db/connection_profiles.rs`, the profile routes + Zod twin (bounds 1..50), the backup / restore / `.qtap` carriers, the context builder's image path (rewritten again by `29c1e2690` — port the two together), the Settings profile editor (P4.6e). ⚠ `provider_sdk_version_guard` (`INSTALLED_ROOT_OPENAI_SDK` / `INSTALLED_ROOT_OPENROUTER_SDK`) moves at any pin ≥ this commit. | UNPROCESSED |
| `4efea65e5` | 2026-11-02 | wardrobe: Green Room multi-slot picks fill every slot they cover | **PORT (wardrobe outfit selection) — model-visible prompt text + a pure function** (classified 2026-11-04 from the hunks; 19 files +8,188 / −6,168 — almost all `package-lock.json` / tests; `dev.29`). NEW `spreadAcrossCoveredSlots` (`slot-ops.ts` +29) puts each item into every slot its types declare; `applyOutfitSelections` runs it on the LLM's pick after dissolving composites; `cheap-llm-tasks/outfit-selection.ts` states the rule (+2 prompt text); `help/wardrobe.md`. | **v5 intersection — PORT:** the Green Room outfit selection (P4.4 unit 2 + the wardrobe programme's `apply_outfit_selections` / slot ops, P4.D279 / P4.D282). Tier-1 candidate: `spreadAcrossCoveredSlots`. | UNPROCESSED |
| `27fb3ec77` | 2026-11-02 | salon: avatar opens full size; carried items shown under the title | **PORT-NEW (SPA)** (classified 2026-11-04 from the hunks; 15 files +253 / −19; `dev.30`). The message-row desktop avatar's picture opens the Salon image viewer (`Avatar.onImageClick`); equipped Inventory items listed under the title (SVG / emoji / two-letter abbreviation + Tooltip; `InventoryGlyph` `fallback: 'abbreviation'`); `useChatCarriedItems` shares `useChatInventory`'s cache; `getMessageAvatar` returns `characterId`. Help: `chats.md`, `inventory.md`. No server hunk. | **v5 intersection:** the Salon message row (P4.6a) + the Inventory SPA glyph (the `322a5488e` round). | UNPROCESSED |
| `4310d435f` | 2026-11-02 | doc-edit: operator document actions go through resolved-file helpers, not fs | **PORT? (a refactor with ONE behaviour change)** (classified 2026-11-04 from the hunks; 16 files +1,016 / −67, mostly tests; `dev.31`). `doc-edit/path-resolver.ts` NEW `statResolvedFile` / `moveResolvedFile` / `deleteResolvedFile`; `operator-doc-actions.ts` stops importing `fs/promises`; **`statResolvedFile` treats only `ENOENT` / `ENOTDIR` as absent — other errors now THROW** (before: any error read as absent). | **v5 intersection:** Document Mode's operator actions + v5's doc-edit path resolver (the P4.6-era Document Mode / doc_* tools port) — measure v5's stat arm for the error class. | UNPROCESSED |
| `f12bda3d1` | 2026-11-02 | docs: changelog for the release-checklist test coverage pass | **NO-PORT? — CHANGELOG only, VERIFIED** (1 file +11). | **v5 intersection:** none. | UNPROCESSED |
| `3d39c303c` | 2026-11-02 | Use API response helpers in announcement and project mount-point routes (#123) | **NO-PORT? — "Output is unchanged", VERIFY at port time** (2 files +6 / −6; `NextResponse.json` → `created()` / `successResponse()` in the announcement action and `projects/[id]/mount-points`). The helpers may differ in status / headers from the raw call — a byte compare on the two routes settles it. | **v5 intersection:** the two routes' families (P4.6-era announcement action; project mount-points) — measure, don't assume. | UNPROCESSED |
| `134961572` | 2026-11-02 | Remove dead Inventory folder wrappers and unused type (release checklist 5) (#124) | **NO-PORT? — dead code, VERIFIED** (4 files +29 / −36): `createInventoryFolder` / `renameInventoryFolder` / `deleteInventoryFolder` / `normalizeInventoryFolderName` + the `AnnouncementRouteSender` type removed; `DEAD-CODE-REPORT.md`. | **v5 intersection:** none (check v5 carries no twin of the dead wrappers — a census nicety, not a port). | UNPROCESSED |
| `f1c2b178d` | 2026-11-02 | Test maxImagesPerRequest through backup restore (#125) | **NO-PORT? — tests only, VERIFIED** (2 files +43). | **v5 intersection:** an oracle-side reference for `d439f6dc9`'s carrier row. | UNPROCESSED |
| `06efd9ca2` | 2026-11-02 | cli: correct the README's native-module note (release checklist 12) (#126) | **NO-PORT? — docs + stamps, VERIFIED** (5 files +10 / −4: `packages/quilltap/README.md`, stamps `dev.35`). | **v5 intersection:** none. | UNPROCESSED |
| `d2795f72c` | 2026-11-02 | Docs: draft 4.12.0 release notes (#127) | **NO-PORT? — docs only, VERIFIED** (2 files +262: NEW `docs/releases/4.12.0.md`). | **v5 intersection:** the `docs/v4/` mirror. | UNPROCESSED |
| `87c3860df` | 2026-11-02 | Fix undefined CSS custom properties in component styles (#128) | **PORT (SPA CSS — ~105 declarations referenced custom properties defined nowhere) + NO-PORT (storybook, the lint script)** (classified 2026-11-04 from the hunks; 13 files +2,688 / −1,023, nearly all the storybook CSS; `dev.35`). `qt-components/_chat.css` +86 / `_surfaces.css` +128 / `_content.css`: `hsl(var(--X))` → `var(--color-X)`, `hsl(var(--X) / N)` → `color-mix(…)`; restores focus indicators on the doc title input and help entity-picker filter; NEW `scripts/check-css-vars.mjs` in `npm run lint`; `help/help-chat.md`. | **v5 intersection — PORT?:** the SPA's ported `qt-components` CSS — measure whether v5 copied the undefined tokens verbatim (likely, P4.D338-era byte ports) — the round's P4.D357 WCAG/tokens lane's file family. | UNPROCESSED |
| `94f041764` | 2026-11-02 | UI: WCAG 2.2 AA pass across bundled themes (release checklist 13) (#129) | **PORT (SPA CSS + five theme packs + accessible names / keyboard tabs)** (classified 2026-11-04 from the hunks; 50 files +752 / −91; `dev.35`). Contrast fixes in `qt-components` (`_chat`, `_content`, `_layout`, `_workspace`, `_intervention`, `_storyteller`, `_surfaces`) and five bundled themes (`art-deco`, `earl-grey`, `great-estate`, `old-school`, `rains` — `styles.css` + `theme.json` bumps); keyboard-operable workspace tabs (`TabStrip.tsx` +56); inert closed slide-over panels; focus rings on Staff bars / chips; accessible names / states on glyph-only and toggle controls (~20 components); `help/tabbed-workspace.md`, `help/themes.md`. | **v5 intersection — PORT:** the round's TWO WCAG lanes (P4.D357 tokens / CSS / theme packs; P4.D358 dialog semantics / accessible names) land the PRE-pin pass — this is a SECOND, later pass over the same files; the workspace tab strip (`p4.9j`); the vendored theme packs. | UNPROCESSED |
| `25a20221c` | 2026-11-02 | docs: Refactor 4.12 specs for what the 4.12-dev cycle touched (#130) | **NO-PORT? — docs only, VERIFIED** (11 files +793: eight NEW `refactor-4-12-*.md` specs + ROADMAP). The specs describe `05264ace3` / `29c1e2690` / `604a082c4` / `a3d53eae1` / `0c47f4539` / `600c2906f` / `705b954c2` — read them as the design intent for those rows. | **v5 intersection:** the `docs/v4/` mirror. | UNPROCESSED |
| `705b954c2` | 2026-11-02 | Build: generate theme-storybook qt-components.css from the app source (#131) | **NO-PORT? — build tooling; the app CSS "compiles byte-identical"** (classified 2026-11-04 from the hunks; 30 files +14,847 / −7,287 — the generated storybook CSS; `dev.41`). The `@theme inline` block + the dark variant MOVED from `app/globals.css` into NEW `app/styles/theme-tokens.css`; generator + lint scripts; `release.yml` publishes theme-storybook. | **v5 intersection:** none in behaviour; the SPA's ported token map lives where v5 put it — a file-path note for any byte-port census over `globals.css`. | UNPROCESSED |
| `ec8d91667` | 2026-11-02 | Storybook: stories use only the app's real qt-* classes and markup (#132) | **NO-PORT? — storybook package only, VERIFIED** (9 files +190 / −64). | **v5 intersection:** none. | UNPROCESSED |
| `05264ace3` | 2026-11-02 | Refactor: one recursive watcher core under both filesystem watchers (#138) | **NO-PORT? for the watchers (NO-COUNTERPART in v5) + PORT? (the two scanners' walk + the files/ scanner's placeholder skip + shutdown ordering)** (classified 2026-11-04 from the hunks; 27 files +1,266 / −664; `dev.43`). NEW `lib/files/recursive-watcher.ts` (265) + `lib/files/walk.ts` (`walkTree` under `walkDirectory`, `listFilesystemFolders`, `walkDisk`, `scanDirectory`); `lib/mount-index/scanner.ts` +122 / `lib/file-storage/scanner.ts` +90 / `sync/walk-disk.ts` rewritten onto `walkTree` (behaviour claimed equal — measure); the files/ watcher no longer hashes evicted iCloud / OneDrive placeholders; one 750 ms debounce; NEW `lib/startup/watcher-shutdown.ts` (stop functions on SIGTERM / SIGINT ahead of the DB close; `instrumentation.ts`); `help/files.md`. | **v5 intersection:** v5 has NO watcher (the standing deferral); the mount scanner (ported, and the `bf7a457f6` scanner fix sits UNPROCESSED beside it — port the two together) and the files/ scanner; v5's SIGINT / SIGTERM shutdown (dogfood #153's fix) has nothing to stop. | UNPROCESSED |
| `29c1e2690` | 2026-11-02 | Refactor: one image transport budget on the way to a provider (#139) | **PORT — the Salon's image selection is REWRITTEN (behaviour moves: a 4 MB base64 total, provenance ranking, an omission text); NOT neutral** (classified 2026-11-04 from the hunks; 20 files +1,227 / −703; `dev.43`). NEW `lib/chat/image-transport-budget.ts` (508 — `applyImageTransportBudget` on the FINAL message array for the primary and each failover substitute: one count cap + one byte budget `LLM_TRANSPORT_TOTAL_BASE64` = 4 MB of base64 over uploads, re-hydrated uploads and Lantern images; ranked upload > re-hydrated > Lantern, newest first, from a transport TAG the loaders set; overflow described, or omitted with `[image omitted: over the request budget]`; vault images (mount-link ids) omitted, never described); `LANTERN_IMAGE_BASE64_BUDGET` retired; `message-attachment-adapter.ts` REMOVED (its `d439f6dc9` cap absorbed); `context-builder.service.ts` +258; `provider-failover.service.ts`; `streamMessage` strips the tag; `lib/files/llm-image-budget.ts`; `help/connection-profiles.md`. | **v5 intersection — PORT:** the context builder's image path + the Lantern walk + provider failover (the production chat spine, P4.2 / P4.6-era courier + chat images; the round's P4.D347 context-builder lane works in the same builder). Port with `d439f6dc9`. | UNPROCESSED |
| `604a082c4` | 2026-11-02 | Refactor 4.12 spec 5: Salon stream frames as a tagged union (#142) | **PORT — THE SALON SSE WIRE MOVES: every Salon turn-path frame is now `kind`-tagged (phase 2); NOT neutral** (classified 2026-11-04 from the hunks + the merged body; 42 files +2,050 / −771; `dev.50`). NEW `lib/chat/stream-frames.ts` (524 — a Zod discriminated union on `kind`: `content`, `reasoning`, `status`, `fileProcessing`, `toolsDetected`, `toolResult`, `turnStart`, `done`, `turnComplete`, `chainComplete`, `error`, `pendingExternalTurn`, `posted`, `storytellerRound`, `storytellerBid`, `roundOpen`, `handAsked`, `handAnswered`, `confirmationResult`, `swipeDone`; `encodeFrame` the only producer); `done` carries ONE `outcome` instead of `skipped` / `yielded` / `stopped` / `emptyResponse`; a swipe stream ends with `swipeDone`; concurrent Storyteller turns write through `seatedFrameEncoder` (the byte-level `tagSseChunk` / `taggingController` and `lib/storyteller/sse-tag.ts` DELETED); **Help chat, the Brahma console, the Scenario Builder and the one-shot tool loop KEEP the old bag** (`legacyFrameEncoder` → `toLegacyBag`); `streaming.service.ts` +161, `tool-execution.service.ts`, `regenerate-swipe-stream.ts`, `turn-outcome.service.ts`; SPA `useSSEStreaming` (748) / `useRegeneration` rewritten on `parseFrame`; a concurrent-Storyteller empty-bubble fix; `API.md` documents the kinds. | **v5 intersection — PORT (large):** `quilltap-web`'s Salon SSE + the core streaming encoders (P4.2 production chat spine; P4.13/P4.14 `StreamMessage`), the Storyteller concurrent-seat tagging (the round's P4.D343 / P4.D344 Storyteller lanes port the PRE-pin `tagSseChunk` shape this deletes), the raise-hand frames (`9eab8d2da`), the SPA's Salon stream reader + regenerate, and every Salon-streaming Playwright beat. The legacy surfaces (Help chat, Brahma, Scenario Builder, one-shot loop) keep v5's current bytes. | UNPROCESSED |
| `a3d53eae1` | 2026-11-02 | Refactor: Extract document core for Document Mode and doc_* tools (#140) | **PORT — two v4 bug fixes + a refactor (`Fixes #133`, `#134`, `#137` — all "Original to v4", v5 status "Not assessed"; NOT convergences)** (classified 2026-11-04 from the hunks + the merged body; 40 files +1,946 / −1,054; `dev.56`). #133: a Document Mode save / rename in `project` scope into the project's official store now re-indexes (gated on the resolved `mountPointId`, hard-link siblings re-indexed); #134: ONE blob reader `readDatabaseBlobForDocument` (raw bytes for native-text kinds, extracted text for converted kinds) under both Document Mode and `doc_read_file`; #137: `doc_open_document` records a new blank document under the scope it was CREATED in; NEW `lib/documents/core/*` (read / write / move / delete / view-target / errors); `operator-doc-actions.ts` −416 net, `operator-doc-http.ts`, the four doc-edit handler files, `database-store.ts` +31, the chat documents action + `documents` route. | **v5 intersection — PORT:** Document Mode + the `doc_*` tools (the Document Mode vertical; the tool subsystem's doc-edit family) — three v4 defects v5 likely shares (each to be measured before porting). | UNPROCESSED |
| `0c47f4539` | 2026-11-02 | Refactor: settle tier walk once in reach.ts (Refactor 4.12) (#141) | **PORT — #135 / #136 fixed (`Fixes #135`, `#136`, "Original to v4", NOT convergences) + the tier-walk refactor (behaviour claimed equal — measure)** (classified 2026-11-04 from the hunks + the body; 54 files +1,589 / −756; `dev.52`). #135 / #136: the wardrobe collection GET + `?action=instructions` GET, the groups / scenarios cast GET and the scenario collection list GET no longer resolve their location with `ensure: true` — a READ no longer mints a project / group store (an owner with no store answers EMPTY). NEW `lib/mount-index/reach.ts` (468: `resolveOwnerStore`, `groupMountsFor` / `groupOriginMap`, `resolveCastPool` / `castGroupMounts`); `tiered-mount-pool.ts`, `scenario-builder/mount-pool.ts`, `scenarios/location.ts`, `wardrobe/location.ts`, `inventory/location.ts` / `pool.ts` / `dependents.ts`; `core-whisper.ts` and `self_inventory` builders drop hand-written loops; `knowledge-injector.ts` +34 (context); `search-scriptorium-handler.ts`; `pascal/custom-tools.ts` / `tool-preset-pool.ts`. | **v5 intersection — PORT:** the wardrobe / scenario / group routes (P4.D281, P4.D295 scenarios-from-anywhere), v5's tier walks (wardrobe pool P4.D279, Inventory pool, Scenario Builder pool, core whisper, `self_inventory`, Pascal walk, `search_scriptorium`, the knowledge injector) — a read that provisions in v5 writes a store row v4 now refuses to mint. | UNPROCESSED |
| `600c2906f` | 2026-11-02 | Refactor 4.12 spec 6, Phases 1–3: the browser family, finished (#143) | **PORT (server: wardrobe GETs stop provisioning, a NEW `?action=duplicate`, a NEW `wardrobe` realtime topic, the shared image-route factory) + PORT? (SPA browser refactor)** (classified 2026-11-04 from the hunks + the body; 79 files +3,826 / −2,423; `dev.53`). Server: shared route-factory preamble + `withItemErrors` (`item-folder-routes.ts` +236); wardrobe GETs no longer provision stores; wardrobe `?action=duplicate`; `createItemImagesRoute(kind)` (`lib/mount-index/item-images-route.ts` 377) under both `wardrobe/[itemId]/images` and `inventory/[itemId]/images`; `lib/inventory/routes/responses.ts`, `request.ts`; three `normaliseRelativePath` copies onto `path-utils.ts`; a `wardrobe` realtime topic published from every wardrobe location write (`job-topics.ts`, `topic-map.ts`, `realtime.types.ts`); SPA: `ItemControlDialogShell`, one `ItemFolderPickerDialog`, `ItemImageViewer`, `ItemDialogProvider`, `useItemFolders`. | **v5 intersection — PORT:** the wardrobe / Inventory route factories + item-image routes (P4.D263, P4.D281, the Inventory round), the realtime topic map, the SPA wardrobe / Inventory dialogs (P4.D285 / P4.D286 / P4.D277's picture viewer). Measure every error body through `withItemErrors`. | UNPROCESSED |
| `30a3b4cb7` | 2026-11-03 | Fix: document moves follow every open copy; edits over uploads drop stale text | **PORT — two v4 defects (from v4's own live pass; no issue)** (classified 2026-11-04 from the hunks; 17 files +444 / −176; `dev.54`). `sweepChatDocumentsAfterMove` + NEW `sweepChatDocumentsAfterFolderMove` (`documents/core/move.ts`) take the resolved `mountPointId` and match `chat_documents` rows by the store they REACH (document_store by store name, project through the chat's project's official store, general literally) — a `project`-scope move no longer leaves a store-name-open copy behind or repoints a same-named file in another project; `doc_move_folder` records the resolved store name, not a raw `self`; `chat-documents.repository.ts` +83 (`renameFilePathInStore` / `renameFolderPathInStore`); an edit over an upload drops the blob's stale extracted text (`doc-mount-file-links.repository.ts`). | **v5 intersection — PORT:** `db/chat_documents`, the doc-edit move tools, Document Mode rename routes, `db/doc_mount_file_links` — port with `a3d53eae1`. | UNPROCESSED |
| `d4ddeb943` | 2026-11-03 | Fix: avatar regeneration no longer absorbs changes made mid-job | **PORT — a one-line job-queue fix** (classified 2026-11-04 from the hunks; 7 files +44 / −6; `dev.55`). `queue-service.ts`: avatar-job reuse adds `&& job.status === 'PENDING'` (a PROCESSING job is no longer collapsed onto, matching the wardrobe item-image path). | **v5 intersection — PORT:** the job runner's avatar-generation enqueue / collapse (Avatar Rolls + the avatar refresh, P4.D282; the in-process job runner). | UNPROCESSED |
| `547f1a739` | 2026-11-03 | Cleanup: remove obsolete top-level files, cicd, and devssl | **NO-PORT? — infra / docs, VERIFIED** (7 files +9 / −209: `GEMINI.md`, `cicd/deploy.sh`, `process_qt.sh` deleted; `DEVELOPMENT.md`, `migrations/README.md`). | **v5 intersection:** the `docs/v4/` mirror. | UNPROCESSED |
| `856ed4f2b` | 2026-11-03 | Cleanup: remove Playwright e2e tests and dependency | **NO-PORT? (tests / tooling) + PORT (one About-credits line)** (19 files +13 / −1,033; `dev.56`): `@playwright/test` dropped, `playwright.config.ts` + `__tests__/integration/*.spec.ts` deleted; `AboutView.tsx` −Playwright. | **v5 intersection:** the About credits list (P4.D359) only; v5's OWN Playwright suite is unaffected. | UNPROCESSED |
| `8f83041aa` | 2026-11-03 | Chore: update npm dependencies across root, packages, and plugins | **NO-PORT? (dependency stamps) — but `@openrouter/sdk` moves again (^1.5.33 → ^1.5.34) and `mermaid` 12.2 → 12.3** (51 files +18,043 / −14,767; `dev.57`): TanStack 5.105.3, `next` 16.4.4, `semver` 7.8.6; every plugin manifest +1; `plugin-utils` stamp; storybook CSS regenerated. | **v5 intersection:** `provider_sdk_version_guard`'s OpenRouter constant; the round's P4.D353 diagrams lane pins `mermaid` at the pre-pin version (a re-pin moves it). | UNPROCESSED |
| `9b5bb27df` | 2026-11-03 | Docs: bring 4.12.0 release notes up to date | **NO-PORT? — docs only, VERIFIED** (2 files +51 / −7). | **v5 intersection:** the `docs/v4/` mirror. | UNPROCESSED |
| `c539c82e0` | 2026-11-03 | Feature: Brahma lookup shortcuts and an engine setting for @Brahma | **PORT-NEW (four operator-only tools + the inline-Brahma profile resolution) + PORT (`run_sql` budget, the Brahma SQL prompt, the global message-search hit shape) — a NEW VAULT KEY, NOT a D23 move** (classified 2026-11-04 from the hunks; 47 files +2,108 / −69; `dev.58`; no migration). NEW tools `lookup_character`, `lookup_memories` (inspection, NO access stamp), `lookup_messages` (FTS5, with conversation title / date / speaker), `read_messages` (`brahma-lookup-handler.ts` 548 + four definition files, declared effects) — refused off the operator surface with `${name} is only available in the Brahma Console.`; `run_sql` results under a 24,000-character `fitToBudget`; `brahma-sql-prompt.ts` leads with the shortcuts, forbids LIKE scans of message text; `searchMessagesGlobal` hits carry `participantId`; inline `@Brahma` resolves project → global → default (`inline-profile.ts`, `resolveInlineBrahmaConnectionProfile`, a stale id falls through); **`brahmaConsole.inlineConnectionProfileId`** (a JSON key in the existing settings value; the PUT's `logSummary` gains it) and **`properties.json` `brahmaConnectionProfileId`** (a project VAULT key; `ProjectSchema`, the export schema, `uuid-remap.ts` new-account remap, `.qtap` import reconcile); SPA pickers (Settings → Chat → Brahma Console; the project Model Behavior card); help `brahma-console.md` / `carina.md` / `project-settings.md`. | **v5 intersection — PORT:** the Brahma console + one-shot (P4.165 / the Brahma vertical), the tool registry + effects (the round's P4.D343 declared-effects lane), `run_sql`, `chats_search` (with `902c6efe9`), the Brahma settings PUT, the project `properties.json` reader / writer (#136's three-state keys; the round's P4.D349 vault-keys lane), the restore remap + `.qtap` import, the SPA settings + project cards. ⚠ LIVE-DATA (§1): a v5 project properties write ERASES `brahmaConnectionProfileId`. | UNPROCESSED |
| `cbf4805c0` | 2026-11-03 | Docs: cover the Brahma lookup shortcuts in the 4.12.0 release notes | **NO-PORT? — docs only, VERIFIED** (2 files +15 / −3). | **v5 intersection:** the `docs/v4/` mirror. | UNPROCESSED |
| `70d2859d8` | 2026-11-03 | Docs: release checklist 14 pass; describe database encryption correctly; fix db integrity | **PORT (CLI `db integrity`) + docs** (classified 2026-11-04 from the hunks; 27 files +134 / −109; `dev.59` — **4.12.0's code**). `packages/quilltap/lib/db-commands.js` +45: `quilltap db integrity` NO LONGER runs `cipher_integrity_check` (a SQLCipher pragma the sqleet driver lacks, which always reported `ok`) — `integrity_check` alone; a page that fails decryption is reported as an issue (exit 1) instead of crashing; `bin/quilltap.js` +9; `scripts/audit-mount-blob-sha256.mjs`'s key check fixed the same way. **Docs: v4 now describes the databases as ChaCha20-Poly1305 (SQLite3MC default, raw key), NOT SQLCipher** — across `DATABASE_ENCRYPTION.md`, help (`database-protection.md`, `data-directory.md`, `mount-points.md`), READMEs, About, CLAUDE.md. | **v5 intersection — PORT:** the CLI's `db integrity` (Tier R — v5's `db_cmd.rs` / `db_help.txt:60,108` still document `cipher_integrity_check`); the About page text; `docs/v4/developer/DATABASE_ENCRYPTION.md` — the file this repo's CLAUDE.md calls WRONG is now right upstream (re-vendor it; the CLAUDE.md warning can then cite v4's own correction). | UNPROCESSED |
| `7bdb45873` | 2026-11-03 | release: 4.12.0 | **NO-PORT? — version stamps, VERIFIED BY CONTENT** (sole parent `1ca45c171`, the squash; `git diff 70d2859d8 7bdb45873` = 4 files +5 / −5: `package.json` / `package-lock.json` / README badge / the CHANGELOG heading). **4.12.0's code = `70d2859d8`'s tree.** Tag `4.12.0`. | **v5 intersection:** none. | UNPROCESSED |
| `b37769882` | 2026-11-03 | merge: 4.12.0 back into main | **NO-PORT? — merge, VERIFIED BY CONTENT** (parents `70d2859d8` + `7bdb45873`; first-parent diff = the release stamps). Same topology as 4.11.0's `9e01ebd25`. | **v5 intersection:** none. | UNPROCESSED |
| `013b99b5d` | 2026-11-03 | dev: started 4.13.0 development | **NO-PORT? — version stamps, VERIFIED** (`4.13.0-dev.0`). | **v5 intersection:** none. | UNPROCESSED |
| `7b4e1b9fa` | 2026-11-03 | docs: CHANGELOG update for new version | **NO-PORT? — docs only, VERIFIED** (`docs/CHANGELOG.md` −614 → NEW `docs/CHANGELOG_V4.md` +614; the 4.x history split out). | **v5 intersection:** the `docs/v4/` mirror if it carries the CHANGELOG. | UNPROCESSED |

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
