# Setupphase survey — the vendor lane (`help/` 137 → 138, `docs/v4/` 518 → 537), the public schema, the NO-PORT ratification record, and the guards that move for `236c04e35..624d0d25c`

**Date:** 2026-11-04. **Read-only survey.** Nothing was built, run or written
except this file. No worktree was created. The v4 working tree has unrelated
uncommitted edits, so every v4 fact below comes from the object store only:
`git -C ~/source/quilltap-server show <rev>:<path>`, `git diff 236c04e35
624d0d25c -- <path>`, `git ls-tree`, and `git log --full-history --no-merges
236c04e35..624d0d25c -- <path>`. Plain `git log` follows one parent through
the two merges and under-attributes. Two read-only peeks went into the
live `node_modules`: Lexical's version and its `src/` (§C.3,
`eb90eb466`). Those files are untracked and match the pin's lock (0.53.0).

**Pin:** v4 `624d0d25c` ("ci: restore the 8 GB heap for the coverage run",
2026-11-04 11:00 −0600, `package.json` `4.13.0-dev.8`). **Baseline:**
`236c04e35` (`4.12.0-dev.0`). **Span:** 73 commits, two of them merges
(`b37769882`, `afec0af10`). **v5 main:** `ff1651177`.

**Squash artefacts.** Two commits appear in almost every `--full-history`
attribution, and both are dropped from the per-file tables below:

- **`7bdb45873`** ("release: 4.12.0"). Its parent is `1ca45c171` (4.11.0),
  so its stat against that parent is the whole 4.11 → 4.12 squash.
  `git diff --stat 70d2859d8 7bdb45873` is four stamp files (+5 / −5).
- **`6121f59b9`** ("release: 4.12.1"). Its sole parent is `7bdb45873`. Its
  content is bugfix `182282af4` (which is NOT reachable from `main`), plus
  `97f5c849f` and the docs. It reaches `main` only through the merge
  `afec0af10`.

Where `6121f59b9` is the ONLY attribution of a path, it is kept: it is how
4.12.1's bytes arrived.

---

## Headline findings (the ten a planner must not miss)

1. **`help/`: 137 → 138.** One page is added: `help/deus-ex-machina.md`
   (68 lines, `url: /salon`; from `db6190b44` and `df8d4d70a`). Thirty-four
   pages are modified. In all, 35 files change, +369 / −64. There are no
   deletes or renames, no `url:` line moves on an existing page, and no
   spaced names.
   - v5's `help/` holds 137 files today, and each one is md5/byte-equal to
     `236c04e35`. This was measured, with 0 diffs and an empty `comm`.
   - Nothing in v5 indexes a help page by name in production: no SPA slug
     list, and no `include_str!` of a named page. The only name hits are SPA
     `*.spec.ts` fixtures (`help-guide-tab.spec.ts`: `chats`, `help-chat`).
   - The context resolver is unaffected. `/salon` first-matches `ariel.md`,
     which sorts before `deus-ex-machina.md`.
   - **The help reconcile count:** `help_docs` goes 137 → 138 rows, with 34
     `updatedAt` moves. H2/H3 headings across the tree go 2,774 → 2,794
     (+20; the new page adds 7 H2).
2. **`docs/v4/`: 51 operations.** They are 29 overwrites, 19 adds, 2 moves
   (`git mv` then overwrite), **0 deletes**, and the CLI README overwrite.
   v4 `docs/` grows 517 → **536**, and v5 `docs/v4/` grows 518 → **537**
   (536 plus `packages-quilltap-README.md`).
   - v5's 518 are byte-equal to `236c04e35` today (measured; the README too).
   - The only spaced filename, `features/complete/Wardrobe UX Overhaul.md`,
     is unchanged, but the copy loop must still use `while IFS= read -r`.
   - **`CHANGELOG.md` → `CHANGELOG_V4.md` is not a file split.** Both files
     already exist at the baseline. `8832cfd20` moved the `### 4.11.0` section
     (984 lines) and `7b4e1b9fa` moved the `### 4.12.0` section (614 lines)
     into `CHANGELOG_V4.md`. Net effect: `CHANGELOG.md` 990 → 87 lines and
     `CHANGELOG_V4.md` 13,728 → 15,326. Two overwrites, no new path.
   - **The search spec is mirrored TWICE at the pin.**
     `features/search-window-first-and-honest-empty.md` (315 lines, from
     `763dfaceb` on `main`) and `features/complete/search-window-first-and-honest-empty.md`
     (324 lines, arriving via 4.12.1) are both present. The merge kept both;
     vendor both.
3. **Public schemas: ONE moves.** `qtap-export.schema.json` goes 115,274 →
   **117,110** bytes (md5 `43817a725eba85bd74aa8ce94da00804`, +18 / −1). It
   picks up five things from four commits:
   - character `kind` (`b09ca46bb`);
   - `isEstablishedFact`, plus a `systemKind` description sentence
     (`db6190b44`);
   - `supportsImageUpload` and `maxImagesPerRequest` (`d439f6dc9`);
   - project `brahmaConnectionProfileId` (`c539c82e0`).

   `qtap-custom-tool.schema.json`, `qtap-progression.schema.json` and both
   `CUSTOM_TOOL_SPEC*.json` are unchanged. So `qtap_schema.rs:207` and
   `qtap_schema_embed_guard.rs:104` move 115_274 → 117_110. By precedent the
   owner is the carrier lane, not the vendor lane (§D).
4. **⚠ `provider_sdk_version_guard` goes RED on FIVE `RECORDED_*` constants,
   not just the two root ones.** At `624d0d25c` every plugin dir *declares*
   newer SDK floors, and the committed `pin-plugin-sdks.sh` installs exactly
   those floors:

   | SDK | recorded | pin floor |
   |---|---|---|
   | `openai` | 7.23.0 | **7.47.0** |
   | `@anthropic-ai/sdk` | 0.115.0 | **1.4.0** |
   | `@google/genai` | 1.52.0 | **2.32.0** |
   | `@openrouter/sdk` | 1.3.28 | **1.5.34** |
   | `@quilltap/plugin-utils` | 2.7.1 | **2.7.3** |

   The moves come from `9f761e157` and `8f83041aa`, and the plugin-utils
   floor also from `544c7ab1e`. The root constants move as well: openai
   7.45.0 → **7.47.0** and openrouter 1.5.9 → **1.5.34**.

   Per the guard's own header, a plugin-dir move "is a re-record of every
   provider corpus". **A RULING is owed (§H.1).** Either:
   - a provider re-record lane, which must sit beside `544c7ab1e`'s
     Anthropic API-version port, since SDK 1.x defaults to `2026-10-21`; or
   - pin the old floors, which runs v4's 4.13 code on SDKs it was not
     written for.

   This is not the vendor lane's to absorb silently.
5. **`3936fdf65` is NOT NO-PORT.** It bumps `croner` 10.0.1 → **11.0.0**, and
   v4's own CHANGELOG records a DST behaviour change: a spring-forward gap now
   runs at the first valid time; interval schedules run through both
   fall-back copies; `?` no longer acts as `*`.
   - v5's `crates/quilltap-core/src/enclave/cron.rs` transcribes croner-JS
     10.0.1 by hand.
   - `enclave_cron_equivalence.rs:31` `EXPECTED_CRONER = "10.0.1"` is a
     deliberate tripwire. It goes RED at the pin ("re-audit the port against
     the new croner").
   - The SPA's `apps/web/package.json:43` `croner ^10.0.1` (the autonomous
     room card's preview) moves too.

   This is a PORT for the enclave owner, not a ratification.
6. **`afec0af10` is NOT NO-PORT either.** It is the merge that carries 4.12.1
   onto `main`. Its first-parent diff is the whole window-first search (32
   files +1,717 / −78). Its `--cc` diff also holds an evil-merge hunk in
   `lib/tools/native-tool-prompt.ts`: 4.12.1's extended rule-6 sentence,
   folded into `NATIVE_TOOL_RULES` under `96156566b`'s `PREAMBLES` /
   `ToolRulesVariant` shape.
   - That one constant is a **meeting point** between the character-kind
     lane and the 4.12.1 search lane. Name ONE owner (§E.2).
   - The merge also dropped the CI heap flag, which `624d0d25c` restores.
7. **`134961572` is NO-PORT for v5 code, but it has an IMPORT-DEAD rider.**
   It deletes `normalizeInventoryFolderName`, `createInventoryFolder`,
   `renameInventoryFolder`, `deleteInventoryFolder` and
   `AnnouncementRouteSender` (the last is type-only).
   - `harness/oracle/cases/item-folders.ts:68` imports
     `normalizeInventoryFolderName` statically (`:309`), and
     `inventory-keystone-tier2.ts:266/269/272` calls
     `mutations.createInventoryFolder` / `rename…` / `delete…`.
   - Both families (`inventory_folders_equivalence`,
     `inventory_keystone_tier2_equivalence`) die or garble at the pin.
   - The re-aim targets at the pin are
     `normalizeItemFolderName(INVENTORY_FOLDER_RULES, input, m => new InventoryFolderPathError(m))`
     (`lib/mount-index/item-folders.ts:141`), and
     `createItemFolder` / `renameItemFolder` / `deleteItemFolder(INVENTORY_FOLDER_KIND, …)`
     (`lib/mount-index/item-folder-location.ts` + `item-mutations.ts:617`).
8. **`eb90eb466` (Lexical 0.52 → 0.53) needs a MEASUREMENT before
   ratification.** v4's CHANGELOG says "Markdown export escapes literal
   brackets, `<` and `&`". v4's `stripMarkdownEscapes`
   (`MarkdownBridgePlugin.tsx:135`) undoes only the `preserve*` set
   (`* _ \` ~`), so v4's composer bytes may have moved for any text with
   `[`, `<` or `&`.
   - `harness/oracle/cases/table-transformer.test.ts` drives the real
     Lexical, and it is the recorder of `apps/web/src/app/editor/markdown-round-trip.spec.ts`'s
     TABLE corpus. Re-record it at the pin and diff.
   - The composer owner rules on the result (§C.3).
9. **The About page moves in four commits:** `70d2859d8`, `c34986421`,
   `db4a76e39`, `856ed4f2b`.
   - v5's `apps/web/src/app/screens/about/about-page.ts` is a verbatim port
     of v4's `AboutView.tsx`, pinned "at `236c04e35`" (`:434`), and
     `about-page.spec.ts:74,91-102` pins its values.
   - That is 11 lines in all: five feature blurbs, Language "TypeScript 6+",
     Database / Core "SQLite3 Multiple Ciphers", the deleted "File Manager:
     SVAR" row, and the deleted credits `@svar-ui/react-filemanager`,
     `chokidar` and `Playwright`.
   - ONE owner should take all of it, and the vendor lane is proposed
     (§H.4). Otherwise four lanes patch one array.
10. **The guards and constants that move.**

    | Guard / constant | From | To | Note |
    |---|---|---|---|
    | `help_tree_embed_guard.rs:121` `VENDORED_FILE_COUNT` | 137 | **138** | |
    | `host_help_docs_boot.rs:135` (+ the `//!` lines 7 / 14 / 17 and the comment at `:132-134`) | 137 | **138** | |
    | `help_tree_equivalence` | | | regenerated |
    | `help_section_size_equivalence` | | | regenerated (`js-tiktoken` is still 1.0.21 at the pin) |
    | `ai_import_tier3_equivalence.rs:126` `V4_APP_VERSION` | `"4.12.0-dev.0"` | **`"4.13.0-dev.8"`** | |
    | the two `INSTALLED_ROOT_*` | | | finding 4 |
    | `builtin_prompt_templates_guard` | | | RED at the pin: 22 prompt files moved, incl. `MODERN_GENERAL.md` → `MODERN_ROLEPLAY.md` (R058) and a NEW `MODERN_ASSISTANT.md` — `d0201575a` / `7ff5fda89` / `ba6c8fe0f`'s lane |
    | `EXPECTED_CRONER` | | | finding 5 |
    | `zod_version_guard` | 4.6.7 | 4.6.7 | **unmoved** (only `qtap-plugin-mcp` moved zod 3.25.76 → 4.6.7, and the guard reads the root) |

    The CLI's own manifest now carries the dev stamps too
    (`packages/quilltap/package.json` `4.11.0-dev.112` → `4.13.0-dev.8`).
    No v5 literal pins it (`ggrep` found none).

---

## §0 — The span for this slice, in one paragraph

Across `236c04e35..624d0d25c`, v4 shipped 4.12.0 and 4.12.1 and started
4.13. The release added:

- Deus Ex Machina, with the 22nd D23 move;
- the per-profile image cap and the image transport budget, with the 23rd;
- the document core, the tier walk, the browser family and the
  mount-point API;
- the Salon's `kind`-tagged frames;
- Brahma lookups;
- the "probation" prompt pass;
- character kind (phases 1–6) and the window-first search;
- two dependency waves: the driver v13, croner 11, Lexical 0.53, ESLint 10,
  TypeScript 6, and the plugin SDK majors;
- a Playwright removal;
- about twenty docs / CI / storybook commits.

For THIS slice that comes to:

- `help/` grew by one page and changed 34;
- `docs/` grew by 19 net files through 50 operations, plus the README;
- one public schema moved;
- six theme manifests and one icon moved (theme lane — §F);
- the CLI package moved three code files (`70d2859d8` / `8832cfd20` —
  PORT, not vendor) and its README;
- five deletions in `lib/` (`134961572`) killed three oracle call sites.

---

## §A — `help/`

### A.1 Counts and the diff

- `git ls-tree -r --name-only 236c04e35 -- help | wc -l` = **137**; at
  `624d0d25c` = **138**.
- v5: `git ls-files help | wc -l` = 137, every file `cmp`-equal to
  `git show 236c04e35:help/<p>`, and `comm -3` of the two lists is empty.
- `git diff --name-status -M 236c04e35 624d0d25c -- help` gives 1 `A` and
  34 `M`; `--stat` gives `35 files changed, 369 insertions(+), 64 deletions(-)`.
- No `url:` / `title:` line moves on an existing page. The only front-matter
  `+` lines are the new page's `---` / `url: /salon` / `---`.

| Page | +/− | Commits (full-history, squashes dropped) | Content class (for the HANDOFF table) |
|---|---|---|---|
| `help/deus-ex-machina.md` (**A**) | +68 | `df8d4d70a`, `db6190b44` | Deus Ex Machina (server `db6190b44`, SPA `df8d4d70a`). Headings: `# Deus Ex Machina — declaring a thing true`; `## In your own voice: /deus and the shortcut`; `## From the machine: the Deus Ex Machina tab`; `## What the cast actually hears`; `## Who knows it`; `## The reminder on their next turn`; `## Memories`; `## In-Chat Navigation` |
| `help/agent-mode.md` | +20 / −6 | `96156566b` | character kind (Agent Mode flavour) |
| `help/ai-character-import.md` | +1 / −1 | `3ae4cf582` | character kind (generators) |
| `help/brahma-console.md` | +24 / −3 | `c539c82e0` | Brahma lookups |
| `help/carina.md` | +1 / −0 | `c539c82e0` | Brahma lookups |
| `help/character-creation.md` | +1 / −1 | `3ae4cf582` | character kind |
| `help/character-editing.md` | +12 / −2 | `96156566b`, `b09ca46bb` | character kind |
| `help/character-external-prompt.md` | +1 / −1 | `3ae4cf582` | character kind |
| `help/character-optimizer.md` | +8 / −0 | `3ae4cf582`, `ba6c8fe0f` | character kind + probation |
| `help/character-system-transparency.md` | +6 / −5 | `db6190b44` | Deus Ex Machina |
| `help/chat-multi-character.md` | +2 / −0 | `c69a08882` | group-scene discipline |
| `help/chats.md` | +7 / −1 | `27fb3ec77`, `3c41cf3da` | SPA |
| `help/connection-profiles.md` | +14 / −1 | `29c1e2690`, `d439f6dc9`, `544c7ab1e` | image budget / image cap / Anthropic API version |
| `help/custom-tools.md` | +5 / −3 | `0c47f4539`, `fd165fce0` | tier walk / Pascal presets |
| `help/data-directory.md` | +1 / −1 | `70d2859d8` | CLI `db integrity` / ChaCha20 docs |
| `help/database-protection.md` | +2 / −2 | `70d2859d8` | same |
| `help/document-editing-tools.md` | +2 / −2 | `30a3b4cb7`, `a3d53eae1` | document core / moves |
| `help/episodic-memory.md` | +2 / −2 | `6121f59b9` (= bugfix `182282af4`) | 4.12.1 window-first search |
| `help/files.md` | +6 / −0 | `05264ace3` | watcher core (NO-COUNTERPART code; help vendored regardless) |
| `help/help-chat.md` | +2 / −0 | `87c3860df` | CSS tokens (SPA) |
| `help/inform.md` | +4 / −1 | `c69a08882`, `db6190b44` | inform header / Deus Ex Machina |
| `help/insert-announcement.md` | +4 / −0 | `db6190b44` | Deus Ex Machina |
| `help/interrupting.md` | +15 / −2 | `9eab8d2da` | gesture-only yields |
| `help/inventory.md` | +7 / −2 | `600c2906f`, `0c47f4539`, `27fb3ec77`, `b2b783b47` | browser family / tier walk / SPA |
| `help/memory-maintenance.md` | +2 / −2 | `ba6c8fe0f` | probation |
| `help/memory-playing-a-character.md` | +11 / −0 | `ba6c8fe0f` | probation |
| `help/memory-recall-relevance.md` | +1 / −1 | `6121f59b9` | 4.12.1 |
| `help/mount-points.md` | +40 / −9 | `70d2859d8`, `c17033f04`, `c34986421`, `bf7a457f6` | mount API / folder browser / iCloud scan / ChaCha20 |
| `help/project-settings.md` | +16 / −0 | `c539c82e0` | `@Brahma` profile |
| `help/prompts.md` | +13 / −4 | `7ff5fda89`, `d0201575a`, `ba6c8fe0f` | sample prompts |
| `help/scriptorium.md` | +12 / −4 | `c17033f04`, `5efe53ea7` | Document Mode |
| `help/search.md` | +33 / −5 | `902c6efe9`, `4f5047241` | Staff out of search / SPA |
| `help/tabbed-workspace.md` | +6 / −0 | `94f041764` | WCAG pass 2 |
| `help/themes.md` | +12 / −0 | `94f041764` | WCAG pass 2 |
| `help/wardrobe.md` | +8 / −3 | `600c2906f`, `0c47f4539`, `4efea65e5`, `b2b783b47` | item routes / tier walk / Green Room / SPA |

Multi-writer pages refuse per-lane patches by construction: `mount-points.md`
has four commits, `inventory.md` and `wardrobe.md` four each, and
`prompts.md` three. Re-vendor the tree WHOLE, as the P4.D323 R-1 / P4.D360 R-1
precedent does.

### A.2 Where v5's copy lives and how it is guarded (re-measured at `ff1651177`)

- **The embed:** `crates/quilltap-host/build.rs` walks `help/**/*.md` in
  `readdirSync` byte order into `help_embedded.rs`, which `help_content.rs`
  `include!`s. `embedded_help_count()` derives the count.
- **Literals that move 137 → 138:**
  - `crates/quilltap-harness/tests/help_tree_embed_guard.rs:121`
    `const VENDORED_FILE_COUNT: usize = 137;` — append a history line to the
    doc comment at `:115-120`, naming `deus-ex-machina.md`.
  - `crates/quilltap-host/tests/host_help_docs_boot.rs:135`
    `assert_eq!(expected, 137, "the vendored tree at v4 236c04e35");` — move
    the literal and the message, plus the comment at `:132-134` and the
    `//!` lines 7 / 14 / 17 ("137 rows").
- **Regenerated from the pin:**
  - `help_tree_equivalence.rs` (`help-tree-sync.test.ts`, jest). Its history
    comment at `:151-153` ("137 = 137 after (chunks 818 → 857)") gets the new
    line.
  - `help_section_size_equivalence.rs` (`help-section-size.ts`, tsx; it needs
    `js-tiktoken` 1.0.21, which the pin's lock still carries).
- **Derive the count (no edit):** `crates/quilltap-web/tests/help_web_routes.rs`
  and `help_doc_ensure_equivalence.rs`. The latter's committed two-page tree
  is NOT the vendored tree; re-run its recipe from the pin, as P4.D360 R-3
  did.
- **Spelling:** `harness/tools/check_spelling.py` scans `help/` (only
  `docs/v4/` is exempt, `ALLOWED_PREFIXES` `:63`).
  `git grep -il 'quil''ttap' 624d0d25c -- help` is EMPTY. The 15 hits in `docs/`
  are the same set and count as at the baseline, all under the exempt mirror.

### A.3 The rule, unchanged

```bash
git -C ~/source/quilltap-server ls-tree -r --name-only 624d0d25c -- help |
  while IFS= read -r p; do git -C ~/source/quilltap-server show "624d0d25c:$p" > "$p"; done
```

Do it in ONE commit with the two literals, then regenerate the two
equivalence families from the pin. Never read the v4 working tree, because
it is dirty.

---

## §B — `docs/` → v5 `docs/v4/`

### B.1 Counts

| | baseline `236c04e35` | pin `624d0d25c` |
|---|---|---|
| v4 `docs/` (`git ls-tree -r`) | 517 | **536** (+19 adds, 2 renames, 0 deletes) |
| v5 `docs/v4/` (`git ls-files`) | 518 = 517 + `packages-quilltap-README.md` | **537** |

- v5 is byte-equal to the baseline today. That was measured for all 517
  paths plus the README, with no MISSING and no extra.
- **Spaced filename:** `docs/developer/features/complete/Wardrobe UX Overhaul.md`
  is the only one at either end, and it is unchanged. Use
  `while IFS= read -r p` and quote every path (memory: v4 path quoting traps).
- **Reference-only, confirmed:** `crates/quilltap-host/src/spine.rs:3811`
  `docs_dir: Option<PathBuf>` defaults to `None` (`:3826`), and nothing sets
  it (no `docs_dir` in `quilltap-web` / `quilltap-tauri` `src`).
  - `self_inventory`'s release-notes / changelog reads stay "not found", so
    `releases/4.12.0.md` / `4.12.1.md` and the `CHANGELOG.md` → `_V4` moves
    are pure mirror operations.
  - The ONLY test that reads the mirror is `custom_tool_schema_embed_guard.rs:103`
    (`docs/v4/developer/CUSTOM_TOOL_SPEC*.json`). Those files are unchanged
    in the span.

### B.2 The operations (`git diff --name-status -M 236c04e35 624d0d25c -- docs`, full-history attribution, squashes dropped)

| v4 path (status) | +/− | Commits | Op |
|---|---|---|---|
| `docs/BACKUP-RESTORE.md` (M) | +2 / −2 | `70d2859d8` | overwrite |
| `docs/CHANGELOG.md` (M) | +60 / −963 | all non-merge commits (66) — `8832cfd20` and `7b4e1b9fa` MOVE the 4.11.0 / 4.12.0 sections out | overwrite (990 → 87 lines; opens `### 4.13-dev`) |
| `docs/CHANGELOG_V4.md` (M) | +1,598 / −0 | `8832cfd20` (4.11.0, 984), `7b4e1b9fa` (4.12.0, 614), `6121f59b9` | overwrite (13,728 → 15,326) |
| `docs/FEATURES.md` (M) | +3 / −3 | `a299de9f5`, `4f5047241` | overwrite |
| `docs/developer/API.md` (M) | +122 / −51 | `70d2859d8`, `600c2906f`, `a3d53eae1`, `604a082c4`, `c17033f04`, `a299de9f5`, `aec626eff`, `a0ed5ecad`, `fd165fce0`, `db6190b44` | overwrite |
| `docs/developer/BACKGROUND_JOBS_CHILD.md` (M) | +2 / −2 | `70d2859d8` | overwrite |
| `docs/developer/CLI.md` (M) | +3 / −3 | `70d2859d8` | overwrite |
| `docs/developer/DATABASE_ABSTRACTION.md` (M) | +2 / −2 | `70d2859d8` | overwrite |
| `docs/developer/DATABASE_ENCRYPTION.md` (M) | +20 / −13 | `70d2859d8` — **the ChaCha20 correction** ("ChaCha20-Poly1305 page encryption from SQLite3 Multiple Ciphers"; "No `PRAGMA cipher` is issued"; the `applySqlcipherKey()` chokepoint; the cipher table row "ChaCha20-Poly1305 per page … own KDF bypassed") | overwrite — see §B.3 |
| `docs/developer/DDL.md` (M) | +24 / −9 | `b09ca46bb` (`kind` row — vault-only, "no migration exists"), `70d2859d8`, `c539c82e0`, `d439f6dc9`, `db6190b44`, `902c6efe9` | overwrite |
| `docs/developer/DEAD-CODE-REPORT.md` (M) | +35 / −11 | `05264ace3`, `134961572`, `a299de9f5` | overwrite |
| `docs/developer/DEVELOPMENT.md` (M) | +6 / −14 | `70d2859d8`, `856ed4f2b`, `547f1a739` | overwrite |
| `docs/developer/FILE-SYSTEM-IMPLEMENTATION.md` (M) | +7 / −8 | `05264ace3` | overwrite |
| `docs/developer/ICON_INVENTORY.md` (M) | +1 / −0 | `df8d4d70a` | overwrite |
| `docs/developer/PROMPT_ARCHITECTURE.md` (M) | +6 / −1 | `c69a08882`, `db6190b44` | overwrite |
| `docs/developer/SYSTEM_FLOWCHARTS.md` (M) | +2 / −2 | `6121f59b9`, `db6190b44` | overwrite |
| `docs/developer/TESTING_GUIDE.md` (M) | +0 / −1 | `856ed4f2b` | overwrite |
| `docs/developer/features/ROADMAP.md` (M) | +15 / −0 | `604a082c4`, `29c1e2690`, `05264ace3`, `705b954c2`, `25a20221c`, `ba6c8fe0f`, `db6190b44`, `d03eb2d63`, `8832cfd20` | overwrite |
| `…/features/better-sqlite3-multiple-ciphers-v13-shell-notes.md` (A) | 50 | `8832cfd20` | add |
| `…/features/character-kind-and-roleplay-defaults.md` (A) | 329 | `9d6f155c5` (born, 229), `b09ca46bb`, `96156566b`, `d0201575a`, `3ae4cf582`, `c69a08882`, `7ff5fda89` | add — **the character-kind spec** (§12 = the live evaluation) |
| `…/features/complete/better-sqlite3-multiple-ciphers-v13.md` (A) | 193 | `8832cfd20` | add |
| `…/features/complete/custom-tool-builder.md` (M) | +1 / −1 | `a299de9f5` | overwrite |
| `…/features/complete/custom-tool-presets.md` (M) | +18 / −8 | `fd165fce0` | overwrite |
| `…/features/complete/dependency-upgrade-safe-batch.md` (M) | +1 / −1 | `8832cfd20` | overwrite |
| `…/features/complete/folder-browser.md` (A) | 415 | `a3d53eae1`, `c17033f04`, `a299de9f5` (born at `features/` and moved within the span — no baseline path to `git mv`) | add |
| `…/features/complete/pascal-custom-tools.md` (M) | +1 / −1 | `0c47f4539` | overwrite |
| `…/features/complete/qtap-uri.md` (M) | +1 / −1 | `a3d53eae1` | overwrite |
| `…/features/complete/realtime-updates.md` (M) | +14 / −0 | `600c2906f` | overwrite |
| `…/features/complete/salon-inform.md` (M) | +10 / −0 | `c69a08882` | overwrite |
| `…/features/complete/scriptorium-document-policy-frontmatter.md` (M) | +1 / −1 | `a3d53eae1` | overwrite |
| `…/features/complete/search-window-first-and-honest-empty.md` (A) | 324 | `6121f59b9` (bugfix's copy, via `afec0af10`) | add |
| `…/features/complete/svar-bridge-spike-findings.md` (M) | +1 / −1 | `a299de9f5` | overwrite |
| `…/features/complete/wardrobe-folders.md` (M) | +1 / −1 | `600c2906f` | overwrite |
| `…/features/composer-document-references.md` (M) | +1 / −1 | `705b954c2` | overwrite |
| `…/features/deus-ex-machina.md` (A) | 360 | `d03eb2d63` (born, 344), `db6190b44` | add |
| `…/features/prompt-trust-the-probation.md` (A) | 331 | `ba6c8fe0f` | add |
| `…/features/refactor-4-12.md` (A) | 45 | `25a20221c` + every refactor commit | add |
| `…/features/refactor-4-12-browser-family.md` (A) | 118 | `25a20221c`, `600c2906f` | add |
| `…/features/refactor-4-12-document-core.md` (A) | 119 | `25a20221c`, `a3d53eae1` | add |
| `…/features/refactor-4-12-image-transport-budget.md` (A) | 103 | `25a20221c`, `29c1e2690` | add |
| `…/features/refactor-4-12-native-watcher-core.md` (A) | 82 | `25a20221c`, `05264ace3` | add |
| `…/features/refactor-4-12-salon-stream-frames.md` (A) | 142 | `25a20221c`, `604a082c4` | add |
| `…/features/refactor-4-12-storybook-css-generated.md` (A) | 127 | `25a20221c`, `705b954c2` | add |
| `…/features/refactor-4-12-tier-walk.md` (A) | 103 | `25a20221c`, `0c47f4539` | add |
| `…/features/refactor-4-12/storybook-css-diff.md` (A) | 676 | `705b954c2`, `ec8d91667` | add (a NEW subdirectory) |
| `…/features/search-window-first-and-honest-empty.md` (A) | 315 | `763dfaceb` | add (main's copy — kept BESIDE the `complete/` one) |
| `…/features/svar-file-manager-adapter-contract.md` → `complete/…` (R097, +2 / −0) | | `a299de9f5` | **`git mv`** + overwrite |
| `…/features/svar-file-manager-implementation-plan.md` → `complete/…` (R097, +3 / −1) | | `a299de9f5` | **`git mv`** + overwrite |
| `docs/releases/4.12.0.md` (A) | 304 | `d2795f72c` (born, 258), `9b5bb27df`, `cbf4805c0`, `70d2859d8` (+ the release squash, `draft: false`) | add |
| `docs/releases/4.12.1.md` (A) | 49 | `6121f59b9` | add |
| `packages/quilltap/README.md` (M) → `docs/v4/packages-quilltap-README.md` | +3 / −3 | `8832cfd20` + `06efd9ca2` (the native-module paragraph: "prebuilt binaries … never compiled", glibc 2.35), `70d2859d8` (ChaCha20-Poly1305; `integrity_check` only) | overwrite |

The count is **29 overwrites + 19 adds + 2 moves + the README = 51
operations**, with 0 deletes. "The eight `refactor-4-12-*.md`" are, by
count, NINE paths: seven `refactor-4-12-*.md`, the `refactor-4-12.md`
overview, and `refactor-4-12/storybook-css-diff.md`.

Not in `docs/` and NOT vendored (NO-PORT infra, §C):
- `README.md`, `CLAUDE.md`, `.claude/**`, `.github/workflows/{ci,release}.yml`;
- `packages/theme-storybook/**`, `scripts/**`;
- `GEMINI.md`, `cicd/`, `process_qt.sh` (all deleted);
- `playwright.config.ts`, `jest.integration.config.ts`, `tsconfig.json`;
- `migrations/README.md`.

### B.3 The ChaCha20 correction lands in the mirror — a CLAUDE.md wording obligation

After the re-vendor, `docs/v4/developer/DATABASE_ENCRYPTION.md` is RIGHT.
Three v5 sentences are written against the old bytes:

- this repo's `CLAUDE.md:53` ("`DATABASE_ENCRYPTION.md` *wrongly* claims
  SQLCipher");
- `docs/developer/porting/phase-0.md:32,39`;
- the ledger's `70d2859d8` row ("re-vendor it; the CLAUDE.md warning can then
  cite v4's own correction").

The lane must NOT edit `CLAUDE.md` (it is the unifier's / human's). It hands
the sentence over as a §S item: "v4 `70d2859d8` corrected
`DATABASE_ENCRYPTION.md` to ChaCha20-Poly1305 (SQLite3MC default, raw key, no
`cipher=` pragma)". The rule itself stands unchanged. The mirror is never
corrected (P4.D360 R-12), and here it no longer needs to be.

---

## §C — The NO-PORT ratification record

Shape per P4.D339 R-9 / P4.D360 R-9. For each row: the `--stat`; the
code-path census `git show --stat --format= <sha> -- lib app packages
plugins components help public migrations themes` (it prints NOTHING for a
pure docs row); the decisive hunk; the v5 site; the proposed status. Every
row was re-measured on 2026-11-04 against its first parent.

### C.1 Whole rows — ratify `NO-PORT-RATIFIED`

| sha | subject | `--stat` | code-path census | evidence / residue |
|---|---|---|---|---|
| `9d6f155c5` | docs: design spec for character kind and roleplay defaults | 2 files +235 / −1 (spec 229, CHANGELOG) | NOTHING | spec born; reaches v5 at its pin bytes (B.2) |
| `763dfaceb` | docs: add feature spec for better searches | 1 file +315 | NOTHING | `features/search-window-first-and-honest-empty.md` (B.2; kept beside the `complete/` copy) |
| `624d0d25c` | ci: restore the 8 GB heap for the coverage run | 2 files +5 / −1 (`package.json` `test:coverage` gains `--max-old-space-size=8192`; CHANGELOG) | NOTHING | CI script only; the pin |
| `d03eb2d63` | Docs: Deus Ex Machina spec (#122) | 2 files +345 (ROADMAP +1, spec 344) | NOTHING | spec input for `db6190b44`'s lanes |
| `a299de9f5` | docs: close out the folder browser; retire the SVAR specs | 11 files +65 / −23 (`.claude`, `README.md`, CHANGELOG, `FEATURES.md`, `API.md`, DEAD-CODE, five `features/**` incl. the two R097 moves + `folder-browser.md`'s move) | NOTHING | B.2's two `git mv`s + overwrites |
| `f12bda3d1` | docs: changelog for the release-checklist test coverage pass | 1 file +11 | NOTHING | CHANGELOG only |
| `3d39c303c` | Use API response helpers in announcement and project mount-point routes (#123) | 2 files +6 / −6 | `app/api/v1/chats/[id]/actions/announcement.ts`, `app/api/v1/projects/[id]/mount-points/route.ts` | **Byte-identical on the wire, verified from the helper source at `3d39c303c:lib/api/responses.ts`.** `created<T>(data)` (`:207`) is `NextResponse.json(data, { status: 201 })`, the exact replaced call, ×2. `successResponse(data, status = 200)` (`:63-68`) is `NextResponse.json(data, { status })`, i.e. `{status: 200}`, against the replaced `NextResponse.json(data)`, whose default status is also 200. The init carries no headers, so `content-type: application/json` is unchanged. The remaining hunks are `import type` changes, erased at compile. Nothing for v5 to move. |
| `134961572` | Remove dead Inventory folder wrappers and unused type (#124) | 4 files +29 / −36 | `lib/inventory/folders.ts` (−6: `normalizeInventoryFolderName`), `lib/inventory/item-mutations.ts` (−22: `create/rename/deleteInventoryFolder`), `lib/services/announcer/writer.ts` (−3: type `AnnouncementRouteSender`) | Dead code in v4, so nothing for v5 production. v5 keeps its ports (`inventory/item_mutations.rs:1373` cites v4 `createInventoryFolder`) — leave them. **⚠ IMPORT-DEAD rider (finding 7): three oracle sites die** — `harness/oracle/cases/item-folders.ts:68` / `:309` and `inventory-keystone-tier2.ts:266` / `:269` / `:272`. Ratify WITH the re-aim, owned by whichever lane regenerates `inventory_folders_equivalence` / `inventory_keystone_tier2_equivalence` (or a stage-0 `REAIM` lane if no content lane does). |
| `f1c2b178d` | Test maxImagesPerRequest through backup restore (#125) | 2 files +43 (test 37, CHANGELOG) | NOTHING | spec input for `d439f6dc9`'s carrier lane (backup / restore field fidelity of `maxImagesPerRequest`) |
| `06efd9ca2` | cli: correct the README's native-module note (#126) | 5 files +10 / −4 (root README, CHANGELOG, root + CLI `package.json` stamps, CLI README) | `packages/quilltap/README.md` (docs), `packages/quilltap/package.json` (stamp) | **Touches the CLI README v5 vendors** → `docs/v4/packages-quilltap-README.md` (B.2). No CLI code. |
| `d2795f72c` | Docs: draft 4.12.0 release notes (#127) | 2 files +262 | NOTHING | `releases/4.12.0.md` born |
| `25a20221c` | docs: Refactor 4.12 specs (#130) | 11 files +793 (`.claude` +8, CHANGELOG, ROADMAP, eight refactor specs) | NOTHING | B.2 adds |
| `705b954c2` | Build: generate theme-storybook qt-components.css from the app source (#131) | 30 files +14,847 / −7,287 | `app/globals.css` (+61 / −…), NEW `app/styles/theme-tokens.css` (71), `packages/theme-storybook/**`, the CLI `package.json` stamp | **`globals.css` is a pure MOVE.** The removed `@variant dark` + `@theme inline` block, minus blank and comment lines, equals the new `theme-tokens.css` byte for byte, and `globals.css` `@import`s it (measured: identical sorted line sets). Storybook / generator scripts / release workflow: v5 has no storybook (precedent `drift-ledger.md` §6). NO-PORT. |
| `ec8d91667` | Storybook: stories use only the app's real qt-* classes (#132) | 9 files +190 / −64 | `packages/theme-storybook/**` only | NO-COUNTERPART |
| `547f1a739` | Cleanup: remove obsolete top-level files, cicd, and devssl | 7 files +9 / −209 (`GEMINI.md`, `cicd/deploy.sh`, `process_qt.sh` deleted; two `package.json` scripts; `DEVELOPMENT.md`) | `migrations/README.md` (+1 / −1, docs) | NOTHING runtime |
| `9b5bb27df` | Docs: bring 4.12.0 release notes up to date | 2 files +51 / −7 | NOTHING | same file |
| `cbf4805c0` | Docs: cover the Brahma lookup shortcuts in the 4.12.0 release notes | 2 files +15 / −3 | NOTHING | same file |
| `7bdb45873` | release: 4.12.0 | vs its parent `1ca45c171`: 737 files (the squash) | n/a | **VERIFIED BY CONTENT:** `git diff --stat 70d2859d8 7bdb45873` = `README.md`, `docs/CHANGELOG.md`, `package-lock.json`, `package.json` (+5 / −5). The code IS `70d2859d8`. |
| `b37769882` | merge: 4.12.0 back into main | first-parent diff vs `70d2859d8` = the same four stamp files (+5 / −5) | n/a (merge) | — |
| `013b99b5d` | dev: started 4.13.0 development | 4 files +8 / −4 (README, CHANGELOG `### 4.13-dev`, lock, `package.json`) | NOTHING | stamps |
| `7b4e1b9fa` | docs: CHANGELOG update for new version | 2 files +614 / −614 | NOTHING | the `### 4.12.0` section MOVED `CHANGELOG.md` → `CHANGELOG_V4.md` (identical heading lists both sides) |
| `7dedfa067` | deps: ESLint 10 | 5 files +201 / −340 (root manifest / lock, README, CHANGELOG, CLI stamp) | `packages/quilltap/package.json` (stamp) | tooling |
| `3e4f9dea9` | deps: TypeScript 6 | 17 files +754 / −1,225 — root, `create-quilltap-theme`, `plugin-types`, `plugin-utils` (`PLUGIN_UTILS_VERSION` 2.7.x), `theme-storybook` manifests / locks, `default-system-prompts` manifest (`package.json` only — NO `prompts/*.md`) | manifests / locks / a generated version constant | tooling. About's "TypeScript 6+" text is `70d2859d8`'s line, not this commit's (`-S` attribution). |
| `f56965ef9` | chore(deps): minor npm update to root repo | 10 files +71 / −64 — root manifest (`openai` 7.45.1, `@openrouter/sdk` 1.5.21, `@tanstack/react-virtual`), lock, eight one-line `plugins/dist/*/index.js` rebundles | NOTHING outside root manifest / lock / rebundles | interim root versions, superseded at the pin (7.47.0 / 1.5.34 — §G) |

### C.2 Rows the ledger lists as NO-PORT? that are NOT whole — ratify the named half only

| sha | the NO-PORT half (evidence) | the PORT / measure half and its proposed owner |
|---|---|---|
| `afec0af10` (merge: 4.12.1 back into main) | The merge commit itself has no feature of its own. First-parent diff = 4.12.1's whole content (32 files +1,717 / −78), which is `6121f59b9` / bugfix `182282af4`'s PORT row. | **NOT NO-PORT** (finding 6). (a) The window-first search goes to its own lane. (b) The `--cc` evil-merge hunk in `lib/tools/native-tool-prompt.ts` merges rule 6's extended sentence ("If the search lists conversations from that period, read the likeliest one with `read_conversation` before answering. Only if it finds nothing…") into the `NATIVE_TOOL_RULES` constant shared by `96156566b`'s `PREAMBLES.assistant` / `.roleplay`. Make ONE owner of the constant (recommend the character-kind lane, citing the 4.12.1 lane's sentence as a HANDOFF). (c) The merge DROPPED `package.json`'s `--max-old-space-size=8192`, and `624d0d25c` restores it: NO-PORT. |
| `856ed4f2b` (remove Playwright) | `__tests__/integration/**` (−756), `playwright.config.ts`, the jest-integration config, root deps (`@playwright/test` dropped), docs. v5's own Playwright suite is unaffected (it never ran v4's). | **About credits:** `AboutView.tsx` `Testing:` `Jest, Playwright, Storybook, Testing Library` → `Jest, Storybook, Testing Library`. v5 `about-page.ts:460` + `about-page.spec.ts` (§H.4 owner). |
| `db4a76e39` (drop chokidar) | `lib/file-storage/watcher.ts` (+159 / −50) and `lib/mount-index/watcher.ts` (+71 / −41) — **NO-COUNTERPART** (v5 has no watcher: `services/mount_index/file_ops.rs:12-13` "the chokidar-equivalent watcher deferral"); a comment in `mount-points/[id]/route.ts:297`; deps. | **About credits:** `Filesystem & Archives:` `chokidar, tar, yauzl, semver` → `tar, yauzl, semver`. v5 `about-page.ts:458` (§H.4). |
| `8f83041aa` (update npm deps) | `next` 16.4.4, `@tanstack/*`, `semver`, `@tailwindcss/postcss`; storybook `qt-components.css` (+46); twelve plugin `index.js` rebundles; `plugin-utils` package 2.7.4 (`version.generated.ts`) — all tooling / bundles. | **Three measure obligations.** (a) **`mermaid` ^12.2.0 → ^12.3.0** (root): v5's SPA pins `"mermaid": "12.2.0"` (`apps/web/package.json:47`; the P4.D353 diagram vectors). That is a regen event for the diagrams owner: re-record, and bump on evidence. (b) **The plugin-dir SDK floors**: `openai` ^7.47.0 ×6 dirs, `@anthropic-ai/sdk` ^1.4.0, `@openrouter/sdk` ^1.5.34, `@quilltap/plugin-utils` ^2.7.3 ×14, `plugin-types` ^2.10.0 → `provider_sdk_version_guard` (§G, ruling §H.1). (c) Root `@openrouter/sdk` → 1.5.34 → `INSTALLED_ROOT_OPENROUTER_SDK` (§G). |
| `9f761e157` (packages and plugins to root dependency levels) | `create-quilltap-theme` / `plugin-types` / `plugin-utils` / `theme-storybook` manifests and locks; every plugin manifest version; root `plugin-types` ^2.9.1 / `plugin-utils` ^2.7.2. `plugins/dist/qtap-plugin-anthropic/provider.ts` +20 / −4 pinned `anthropic-version: 2023-06-01` through a `clientHeaders()` helper, which **`544c7ab1e` superseded**: at the pin `clientHeaders()` returns only `User-Agent` and its doc says the version "is left to the SDK (2026-10-21 since SDK 1.0)". **No net hunk of this commit survives in `provider.ts`.** | **The plugin SDK MAJORS:** `@anthropic-ai/sdk` ^0.115.0 → ^1.3.0, `@google/genai` ^1.52.0 → **^2.32.0**, `openai` ^7.23.0 → ^7.45.1 (×6), `@openrouter/sdk` ^1.3.28 → ^1.5.21, `qtap-plugin-mcp` `zod` ^3.25.76 → ^4.6.7. Owned by the provider re-record ruling (§H.1). The Anthropic API-version consequence is `544c7ab1e`'s PORT row (v5's `provider_manifest/manifests/anthropic.json:17` still sends `"anthropic-version": "2023-06-01"`). |
| `3936fdf65` (croner 11, jest-fetch-mock 4.2) | `jest-fetch-mock` 3.2 → 4.2 and `@types/yauzl` → devDependencies (test-only); `emojibase-data` held at 17. | **PORT (finding 5): croner 10.0.1 → 11.0.0.** v4 calls `new Cron(expr).nextRun(date)` in `lib/services/chat-message/autonomous-room-cron.ts:17` and `components/new-chat/AutonomousRoomCard.tsx:4`. v5's `enclave/cron.rs` (a hand transcription of croner-JS 10.0.1, `:1-40`, `:350`) and `enclave_cron_equivalence.rs:31` `EXPECTED_CRONER = "10.0.1"` (a tripwire that fails by design at the pin) plus the SPA's `croner ^10.0.1` all go to the **enclave / autonomous-rooms owner**. The CHANGELOG's DST / `?` / non-matching-pattern changes are the audit list. |
| `eb90eb466` (Lexical 0.53) | Manifests / lock / README / CHANGELOG / the CLI stamp. v4 records "No code changes"; v5 uses ProseMirror, not Lexical. | **MEASURE (finding 8).** 0.53's `$convertToMarkdownString` "escapes literal brackets, `<` and `&`", while v4's `stripMarkdownEscapes` undoes only `* _ \` ~`. Re-record `harness/oracle/cases/table-transformer.test.ts` (it imports `lexical` / `@lexical/*` and is the recorder for `apps/web/src/app/editor/markdown-round-trip.spec.ts`'s TABLE corpus) at the pin and diff. A moved vector is a composer PORT; an unmoved one ratifies. `text-transforms.test.ts` imports only a `type` from `lexical` (erased). Owner: the composer / editor lane. |

The ledger's NO-PORT? list also names `05264ace3`'s watcher half. That is
not this survey's brief; its scanner walk is a PORT? beside `bf7a457f6`
(ledger §1).

### C.3 Census for the record

There are also uncatalogued infra paths with NO-PORT hunks:
`.claude/**`, `CLAUDE.md`, `README.md`, `.github/workflows/**`, `scripts/**`,
`packages/theme-storybook/**`, `playwright.config.ts`,
`jest.integration.config.ts`, `tsconfig.json`, `.dockerignore`,
`.gitignore`, `Dockerfile*`, and every `__tests__/**` / `lib/**/__tests__/**`.
Content lanes mirror the v4 tests they need as corpora; for example,
`f1c2b178d`'s restore-fidelity test feeds `d439f6dc9`'s carrier lane.

---

## §D — Public schemas and the specimen JSONs

| File | Baseline bytes / md5 | Pin bytes / md5 | Commits (hunk-attributed with `-S`) | v5 copy today | Constants that move |
|---|---|---|---|---|---|
| `public/schemas/qtap-export.schema.json` | 115,274 `8665e3cbffc8db5d808fcebe4bfe302b` | **117,110 `43817a725eba85bd74aa8ce94da00804`** (+18 / −1) | `b09ca46bb` (2026-11-04, the LAST writer): character `kind` `enum ["roleplay","assistant"]`; `db6190b44` (2026-10-29): message `isEstablishedFact` `["boolean","null"]` + the `systemKind` description's `'established-fact'` sentence; `d439f6dc9` (2026-11-01): connection-profile `supportsImageUpload` (boolean) + `maxImagesPerRequest` (integer 1..50, "Defaults to 4"); `c539c82e0` (2026-11-03): project `brahmaConnectionProfileId` `["string","null"]` | `crates/quilltap-core/src/generators/qtap-export.schema.json` 115,274 (md5 = the baseline's — measured) | `qtap_schema.rs:207` `assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(), 115_274)` → `117_110`; `qtap_schema_embed_guard.rs:104` `VENDORED_BYTES = 115_274` → `117_110` (+ a history line); `qtap_schema_validate_equivalence` rows for the five keys, red-first |
| `public/schemas/qtap-custom-tool.schema.json` | 66,469 | **unchanged** | — | `apps/web/public/schemas/` + `pascal/specimens/` | `public_schemas_vendor_guard.rs:58`, `custom_tool_schema_embed_guard.rs:33` unmoved |
| `public/schemas/qtap-progression.schema.json` | 5,963 | **unchanged** | — | `apps/web/public/schemas/` | `public_schemas_vendor_guard.rs:64` unmoved |
| `docs/developer/CUSTOM_TOOL_SPEC.json` / `_DICE.json` | — | **unchanged** | — | specimens + `docs/v4/developer/` | `custom_tool_schema_embed_guard.rs` unmoved |

**Who vendors it.** By the P4.D323 / P4.D339 / P4.D360 precedent, the
content lane that ports the schema's acceptance owns the bytes. The vendor
lane re-runs `public_schemas_vendor_guard`, `qtap_schema_embed_guard` and
`custom_tool_schema_embed_guard` on the union as a CHECK, with the
constants it expects recorded.

Four commits write it, so recommend the round's `.qtap` carrier owner (the
keystone / D23 lane if there is one, as with P4.D341). The other three
lanes (`kind`, Deus Ex Machina, image cap) cite the vendored bytes in their
`HANDOFF:`. The vendor lane must NOT touch the schema. On the vendor branch
alone, `qtap_schema_embed_guard` is RED by design against a pin checkout
(the P4.D339 §R.13 (o) shape).

---

## §E — Meeting points this slice exposes (for §R)

1. **Help prose.** Every row of A.1 is a content lane's prose. The vendor
   lane's bytes are canonical, and each content lane's `HANDOFF:` names its
   rows. Pre-measure each page's pin md5 at planning; the lane re-measures.
2. **`NATIVE_TOOL_RULES`** (`afec0af10`'s evil merge × `96156566b` × 4.12.1):
   one owner (C.2).
3. **The About page** (§H.4).
4. **`provider_sdk_version_guard`**: the five RECORDED constants + the two
   root constants. They need ONE owner, plus the re-record (§H.1).
5. **`enclave_cron_equivalence`**: `EXPECTED_CRONER` + `cron.rs` + the SPA
   `croner` (C.2).
6. **`builtin_prompt_templates_guard`**: 22 prompt files, an `A` and an
   R058. These belong to `d0201575a`'s sample-prompts lane, not the vendor
   lane.
7. **The theme packs** (`94f041764` five `theme.json` 1-line version bumps
   + five `styles.css`; `df8d4d70a` `madmans-box/theme.json` (+ the
   `deus-ex-machina` icon entry, version 1.1.9 → 1.1.10) + NEW
   `madmans-box/icons/deus-ex-machina.svg`). All six v5 `theme.json` /
   `tokens.json` are byte-equal to the baseline today. By last round's R-M,
   ONE owner, the theme / WCAG lane, takes all theme files. **No
   `tokens.json` moves.**
8. **`V4_APP_VERSION`**: the vendor lane, unless a content lane regenerates
   the AI-import family.
9. **`crates/quilltap-core/src/services/provisioning/migration_indexes.json:5`
   `"v4Version": "4.12.0-dev.0"`**: the D23 re-dump lane's, never the vendor
   lane's.
10. **CLI `db_help.txt:60,108`** still documents `cipher_integrity_check`.
    That is `70d2859d8`'s PORT (Tier R `db help`), which belongs to the CLI
    lane, not the vendor lane. The completion templates are unchanged in the
    span; `git diff 236c04e35 624d0d25c -- packages/quilltap/lib/completion`
    is empty.

---

## §F — The guards and what they read at the pin (summary table)

| Guard / constant | Site (v5 `ff1651177`) | Today | At `624d0d25c` | Owner (proposed) |
|---|---|---|---|---|
| `VENDORED_FILE_COUNT` | `help_tree_embed_guard.rs:121` | 137 | **138** | vendor |
| boot literal + message | `host_help_docs_boot.rs:135` (+ `:7`, `:14`, `:17`, `:132-134`) | 137 / "at v4 236c04e35" | **138 / "at v4 624d0d25c"** | vendor |
| `help_tree_equivalence` | regen | green at 137 | regen; RED at 137 before the copy | vendor |
| `help_section_size_equivalence` | regen | — | +7 sections from the new page, plus edits | vendor |
| `V4_APP_VERSION` | `ai_import_tier3_equivalence.rs:126` | `4.12.0-dev.0` | **`4.13.0-dev.8`** (regen) | vendor |
| `INSTALLED_ROOT_OPENAI_SDK` | `provider_sdk_version_guard.rs:132` | 7.45.0 | **7.47.0** (`d439f6dc9` / `8f83041aa`) | §H.1 |
| `INSTALLED_ROOT_OPENROUTER_SDK` | `:136` | 1.5.9 | **1.5.34** (`8f83041aa`) | §H.1 |
| `RECORDED_OPENAI_SDK` | `:116` | 7.23.0 | floor **7.47.0** | §H.1 (re-record) |
| `RECORDED_ANTHROPIC_SDK` | `:118` | 0.115.0 | floor **1.4.0** | §H.1 |
| `RECORDED_GOOGLE_GENAI_SDK` | `:120` | 1.52.0 | floor **2.32.0** | §H.1 |
| `RECORDED_OPENROUTER_SDK` | `:122` | 1.3.28 | floor **1.5.34** | §H.1 |
| `RECORDED_PLUGIN_UTILS` | `:148` | 2.7.1 | floor **2.7.3** | §H.1 |
| `RECORDED_NODE` | `:142` | v24.13.1 | unmoved | — |
| `RECORDED_ZOD_VERSION` | `zod_version_guard.rs:155` | 4.6.7 | **4.6.7** (root unmoved) | CHECK |
| `builtin_prompt_templates_guard` | reads `plugins/dist/qtap-plugin-default-system-prompts/prompts/*.md` | — | RED: 22 files (A `MODERN_ASSISTANT.md`, R058 `MODERN_GENERAL` → `MODERN_ROLEPLAY`) | sample-prompts lane |
| `EXPECTED_CRONER` | `enclave_cron_equivalence.rs:31` | 10.0.1 | **11.0.0** (RED by design) | enclave owner |
| schema bytes | `qtap_schema.rs:207`, `qtap_schema_embed_guard.rs:104` | 115_274 | **117_110** | carrier lane; vendor CHECKs |
| custom-tool / progression / specimens | `public_schemas_vendor_guard.rs:58` / `:64`, `custom_tool_schema_embed_guard.rs:33` | 66_469 / 5_963 | unmoved | CHECK |
| About credits | `apps/web/src/app/screens/about/about-page.ts:349-460`, `about-page.spec.ts` | `236c04e35` text | 11 lines (§H.4) | §H.4 |
| spelling | `harness/tools/check_spelling.py` | green | green (help has no hit; `docs/v4/` exempt) | CHECK |

`pin-plugin-sdks.sh`'s own header says: "Whenever the pin moves, re-check
that the floors printed at the end still equal the versions
`provider_sdk_version_guard` records". At this pin they do NOT.

---

## §G — Dependency facts this slice reads

The values below are measured from `git show 624d0d25c:package-lock.json`
and the manifests.

- **Root lock:** `openai` 7.47.0, `@openrouter/sdk` 1.5.34, `zod` 4.6.7,
  `mermaid` 12.3.0, `croner` 11.0.0, `js-tiktoken` 1.0.21. `lexical` is
  0.53.0 by the manifest, and the live `node_modules` agrees.
- **Root manifest moves in the span:** `better-sqlite3` → `^13.0.3`,
  `chokidar` REMOVED, `@svar-ui/react-filemanager` REMOVED,
  `@playwright/test` REMOVED, `typescript` ^6.0.3, `eslint` ^10.14.0,
  `pdfjs-dist` ^6.5.438, `plugin-types` ^2.10.0 / `plugin-utils` ^2.7.3, and
  `test:coverage` gains `--max-old-space-size=8192`.
- **Plugin-dir declarations at the pin:**
  - `openai` ^7.47.0 (six dirs): deepseek, grok, nanogpt, openai,
    openai-compatible, z-ai.
  - `@anthropic-ai/sdk` ^1.4.0 (anthropic).
  - `@google/genai` ^2.32.0 (google).
  - `@openrouter/sdk` ^1.5.34 (openrouter).
  - `@quilltap/plugin-utils` ^2.7.3 (every dir but builtin-embeddings).
- **The CLI manifest** (`packages/quilltap/package.json`): `4.13.0-dev.8`;
  the driver `^13.0.3`, `@napi-rs/canvas` ^1.1.0, `sharp` ^0.35.5. The
  driver move is the SQLite3MC 2.4.0 ruling the ledger already flags; it is
  not this lane's.

---

## §H — Rulings owed to the human

1. **The provider-SDK floors (finding 4).** At the pin, `pin-plugin-sdks.sh`
   installs SDKs past every `RECORDED_*` constant, including two majors:
   Anthropic 0.115 → 1.4, which changes the default API version, and genai
   1.52 → 2.32. Options:
   - **(a)** A provider re-record lane that re-records every provider corpus
     from the pin and moves the five constants. It sits beside, or stacked
     on, `544c7ab1e`'s Anthropic lane.
   - **(b)** Pin the OLD floors in the pin. This means v4's code, written
     for SDK 1.x / 2.x, runs on 0.115 / 1.52, which is the same skew class
     the 2026-11-02 ruling rejected.

   **Recommend (a).** Give the two `INSTALLED_ROOT_*` constants to the same
   owner, so ONE lane moves the guard.
2. **`3936fdf65` reclassified PORT** (croner 11 → `enclave/cron.rs` + the
   SPA). Confirm, and name the owner.
3. **`afec0af10` reclassified PORT** (the 4.12.1 vehicle + the
   `NATIVE_TOOL_RULES` evil-merge hunk). Name the ONE owner of the constant.
4. **The About page** (11 lines, four commits: `70d2859d8`, `c34986421`,
   `db4a76e39`, `856ed4f2b`). Recommend the vendor lane own
   `about-page.ts` + `about-page.spec.ts` WHOLE as a byte port, with an SPA
   version bump. This breaks the P4.D360 "no `apps/web/src`" posture, so it
   needs a deliberate ruling; the alternative is ONE content lane
   (`70d2859d8`'s CLI / docs lane holds seven of the eleven lines).
5. **`134961572`'s IMPORT-DEAD re-aim:** the content lane that regenerates
   the two Inventory families, or a stage-0 `REAIM` lane if none does.
6. **`eb90eb466` (Lexical 0.53):** a measure-first ratification (C.2). If
   the composer's markdown export moved, it is a composer PORT, and possibly
   a v4 bug worth filing (ask first).
7. **`8f83041aa`'s `mermaid` 12.3:** a re-record and bump obligation for the
   diagrams owner (v5 pins `12.2.0` exactly).
8. **The CLAUDE.md / phase-0 wording** after the ChaCha20 correction is
   mirrored (B.3). This is the unifier's edit, by the human's leave.

---

## §I — The vendor lane: proposed scope and owned paths

**It creates or edits EXACTLY:**

1. **`help/**`**: 137 overwritten (34 actually change) plus 1 created
   (`deus-ex-machina.md`). Every byte comes from
   `git show 624d0d25c:help/<p>`.
2. **The help guards:**
   - `crates/quilltap-harness/tests/help_tree_embed_guard.rs` (`:115-121`);
   - `crates/quilltap-host/tests/host_help_docs_boot.rs` (`:7`, `:14`,
     `:17`, `:132-135`);
   - the history comments of `help_tree_equivalence.rs` (`:151-153`) and
     `help_section_size_equivalence.rs`. Both families are regenerated from
     the pin; the NDJSON is not committed.
3. **`docs/v4/**`**: the 51 operations (B.2). That is 29 overwrites, 19 adds
   (incl. the NEW `developer/features/refactor-4-12/` directory), two
   `git mv` + overwrite, and `docs/v4/packages-quilltap-README.md`. The
   result is **537** tracked, verified by a full byte census (`docs/v4/<p>`
   = `git show 624d0d25c:docs/<p>` for all 536, plus the README).
4. **`crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:126`**:
   `V4_APP_VERSION` → `"4.13.0-dev.8"`, with the family regenerated at the
   pin.
5. *(If §H.4 rules so)* `apps/web/src/app/screens/about/about-page.ts` +
   `about-page.spec.ts`, the 11 lines, plus the SPA `package.json` /
   lockfile version bump.
6. The lane record in `status-log.md` (append-only), with the NO-PORT block:
   - C.1's 23 whole rows;
   - C.2's halves, each with its `--stat` and census re-run;
   - the `3d39c303c` helper-source evidence;
   - the `705b954c2` move proof;
   - the `7bdb45873` / `b37769882` content proofs.

**It must NOT touch:**
- any theme file (the theme lane, R-M precedent);
- the export schema or its two constants (the carrier lane);
- `provider_sdk_version_guard.rs` (the §H.1 owner);
- `enclave_cron_equivalence.rs` / `cron.rs` / the SPA `croner` (the enclave
  owner);
- `builtin_prompt_templates_guard` and the vendored prompts (the
  sample-prompts lane);
- `crates/quilltap-cli/src/help/*.txt` (the CLI lane, `70d2859d8`);
- `migration_indexes.json` (the D23 lane);
- any `harness/oracle/**` re-aim (§H.5);
- `CLAUDE.md`;
- `crates/*/src/**`.

**CHECKs on the union (no edit):** `public_schemas_vendor_guard`,
`qtap_schema_embed_guard` (expect 117_110), `custom_tool_schema_embed_guard`,
`zod_version_guard` (4.6.7), `help_doc_ensure_equivalence` (recipe re-run),
`help_web_routes`, `spelling_guard`.

**Tiering:**
- **Tier 1:** `help/` 138 byte-exact with the two literals and both regens
  green; `docs/v4/` 537 with the census; `V4_APP_VERSION`; the NO-PORT
  record.
- **Tier 2:** the HANDOFF byte-compare table (A.1's 35 rows plus the docs
  rows content lanes cite), with pin md5s; the schema CHECK; the About port
  (if ruled).
- **Tier 3:** none new. The lane has no runtime surface.

**Stacking:** none. It cuts from `main` (wave 0) and is picked LATE in
unification, so its help / docs guards judge a union that carries every
server feature.

---

## §J — Dogfood rows this lane queues

- The Guide lists 138 pages, with "Deus Ex Machina — declaring a thing
  true" in position. `/salon`'s context help still resolves `ariel.md`
  (first match).
- `help_docs` on the copy's first boot after the re-vendor: 137 → 138 rows
  and 34 `updatedAt` moves. Section chunks are re-embedded only if an
  embedding profile is configured 💸; note the enqueue count.
- If §H.4 rules the vendor lane owns it, the About page shows the
  ChaCha20-Poly1305 / SQLite3 Multiple Ciphers lines and no SVAR, chokidar
  or Playwright.
- NOT a row: `self_inventory`'s `quilltap.releaseNotes` for 4.12.0 / 4.12.1.
  `docs_dir` is `None`, so the tool answers "not found" by design.
