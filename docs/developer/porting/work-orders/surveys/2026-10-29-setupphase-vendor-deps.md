# Setupphase survey — the vendor lane (P4.D360), the NO-PORT ratification record, the IMPORT-DEAD / runtime-break census, and the round-wide dependency facts for `322a5488e..236c04e35`

**Date:** 2026-10-29. **Read-only survey** (nothing built, run or written
except this file; the only commands with side effects were `npm cache ls`
and `npm pack --offline --dry-run` into the scratchpad).

**Pin:** v4 `236c04e35` ("dev: started 4.12.0 development", v4 `main`
2026-10-28 19:30 −0500, `package.json` `"version": "4.12.0-dev.0"`;
code-identical to the 4.11.0 release `1ca45c171` and to `7789ebed6` —
`git diff --stat 7789ebed6 236c04e35` = `README.md`, `docs/CHANGELOG.md`,
`docs/releases/4.11.0.md`, `package-lock.json`, `package.json`, +8 / −6).
**Baseline:** `322a5488e` (`4.11.0-dev.53`). **Span:** 69 commits
(`git log --oneline 322a5488e..236c04e35 | wc -l`), three of them merges
(`5e18ede6a`, `744596158`, `9e01ebd25`). **v5 main:** `f6cdceedd` (core
0.0.1379, host 0.0.217, web 0.0.251, cli 0.0.35, tauri 0.0.9, SPA 0.5.907).

**v4 facts** come only from `git -C ~/source/quilltap-server show
236c04e35:<path>`, `git diff 322a5488e 236c04e35`, `git log --full-history
--no-merges 322a5488e..236c04e35 -- <path>` (plain `git log` follows ONE
parent through the `9e01ebd25` merge and under-attributes — e.g. it listed
`docs/CHANGELOG.md` as two commits where `--full-history` lists 63; every
attribution below is the full-history one with the `1ca45c171` squash
artefact dropped), and `grep` inside the read-only pin worktree
`/tmp/qt-v4-pin-setupphase-7789ebed6` (HEAD `7789ebed6`). **v5 line
numbers** are at `f6cdceedd`. Lanes informed: **P4.D360** (this lane) and,
through §E / §F, every regenerating lane; the theme split binds **P4.D357**;
the schema ownership binds **P4.D341 / P4.D345**; the CLI facts bind
**P4.D354 / P4.D356**; §I is the round's LIVE-DATA preamble.

---

## Headline findings (the ten a planner must not miss)

1. **⚠ `npm ci --offline` at the pin FAILS — the npm cache does NOT hold
   the pin's lockfile.** The human's live checkout is at `4.12.0-dev.5`
   (`node_modules/.package-lock.json`) with `openai` **7.45.1** and
   `better-sqlite3-multiple-ciphers` **13.0.3** (past `8832cfd20`), so the
   cache skipped the pin's versions: of the 188 tarballs the lock adds or
   moves, **44 are `ENOTCACHED`**, among them `openai@7.45.0`,
   `@openrouter/sdk@1.5.9`, `pdfjs-dist@6.4.299`, `next@16.4.3`,
   `katex@0.19.1`, `@tanstack/react-query@5.105.0`, `dotenv@18.0.9`,
   `@napi-rs/canvas@1.0.13` (§F.4; `npm pack <spec> --offline --dry-run`
   confirms each). The 2026-10-07 rule's safe default (replace the
   `node_modules` symlink with `npm ci --offline`) cannot be followed as
   written. **RULING OWED (§J.1):** one ONLINE `npm ci --no-audit --no-fund`
   in the pin worktree (network once; the cache is then populated for every
   lane), or the symlink class (1) with the WRONG root versions (7.45.1 /
   1.5.21 / driver v13 — `provider_sdk_version_guard` would then read 7.45.1
   against whatever the lane pins, and v4's code at the pin would run on a
   driver version it was not written for).
2. **IMPORT-DEAD: SEVEN leaf cases + ONE `require.resolve`; ZERO builders;
   NO stage-0 `REAIM` lane — but the brief's list is wrong in one place.**
   `cases/canonicalize.ts` + `cases/tool-definitions-canonical.ts` are
   ALIVE: `UniversalTool` is exported from
   `lib/plugins/interfaces/tool-plugin.ts:23` at the pin and both cases
   import it from THAT module (`tool-definitions-canonical.ts:17`). The dead
   ones, each re-aimed by its content lane (§E.1): `context-compression.ts`
   (`shouldApplyCompression` → RENAMED + RESHAPED
   `shouldApplyBudgetCompression(totalEstimatedTokens, maxAvailable,
   settings, bypass)` at `compression.ts:117` — P4.D347), `memory-injector.ts`
   (`formatSummaryForContext` GONE, no replacement export — P4.D347),
   `skip-signal.ts` (`buildTurnSkipInstruction` moved to
   `lib/chat/turn-manager/skip-signal.ts:428` — P4.D348), `item-folders.ts`
   (`inventoryFolderStorePath` / `normalizeInventoryFolderPath` GONE;
   `itemFolderStorePath(rules, folder)` / `normalizeItemFolderPath(rules, …)`
   at `item-folders.ts:224` / `:99` and
   `normalizeInventoryFolderPathOrRoot` at `inventory/folders.ts:97` —
   P4.D350), `context-feeders-leaves.ts` (`findIntroducedOffSceneCharacterIds`
   GONE, no replacement — P4.D347), `mentioned-characters.ts` (module
   DELETED — P4.D347), `tool-build.test.ts` (`lib/tools/destructive-tools`
   DELETED — P4.D343), `file-content-extractor.test.ts:149`
   (`require.resolve('pdf-parse')` — P4.D354). Every one of the 61 removed
   `export` lines and the four deleted modules was intersected with
   `harness/**`, `crates/**`, `apps/web/src/**`: the only hits are those
   eight `cases/*` files (§E.1); `harness/oracle/lib/**` and
   `harness/oracle/fixtures/**` import none of them.
3. **GARBLE class: ONE positional contract broke** — `buildTools` went from
   positional `(connectionProfile, imageProfileId, imageProfile, userId,
   projectId?, …)` to ONE `(options: BuildToolsOptions)`
   (`streaming.service.ts:285`); the `tool-build.test.ts` re-aim must switch
   to the options object or it emits well-formed rows for an `undefined`
   profile. Seven other oracle-called signatures WIDENED (additive, old
   positional calls survive, the families REDDEN not garble): `generateRenderingPatterns` / `generateFormattingPromptHint` (a trailing
   `options`, `narrationDelimiters` now `| null` — P4.D351),
   `generateGreetingMessage` (`connectionProfileId` — P4.D348),
   `finalizeMessageResponse` (`storyteller` — P4.D343), `isRecordOnlyMessage`
   (`content` — P4.D347), `policyProtectionOf` (`keystone` — P4.D341),
   `sceneDescriptionParts` (`humanoid` in the Pick — P4.D349) (§E.3).
4. **`help/`: 135 → 137** (+`diagrams.md` 132 lines, +`storyteller-mode.md`
   120 lines; 35 pages modified; 37 files +662 / −78). The four guards move
   135 → 137 (`help_tree_embed_guard.rs:114`, `host_help_docs_boot.rs:131`,
   + the two regenerated families). `custom-tools.md` carries FOUR commits'
   hunks, `salon-host-introductions.md` four, `memory-recall-relevance.md`
   three — per-lane patches refused by construction; re-vendor WHOLE (§A).
5. **`docs/v4/`: 42 operations** — 18 overwrites, 19 adds (the 12 fixed-bug
   files 258–269, `docker-test-plan.md`, four `complete/` specs,
   `composer-document-references.md`, `releases/4.11.0.md`), **3 deletes**
   and **2 renames** (`b09ff5c86`; the ledger row says "six mirrored paths
   deleted / moved" — measured FIVE source paths: 3 `D` + 2 `R`, plus ONE add)
   — v4 `docs/` 501 → 517; v5 `docs/v4/` 502 → **518** (517 + the CLI
   README, itself overwritten: +5 / −4 from `539bb4012` / `8cb584eff` /
   `a5efdbb97`). **No runtime reads the mirror:** v5's `self_inventory`
   release-notes seam is `quilltap-host::env::production_self_inventory_env(
   version, docs_dir, db)` (`env.rs:238`) and production passes `docs_dir:
   None` (`spine.rs:3752`) — `docs/v4/releases/4.11.0.md` is reference-only
   (§B.3).
6. **Theme packs — the "byte copy" premise is HALF true.** Of v5's 18
   `apps/web/public/themes/<id>/{styles.css,theme.json,tokens.json}`, the 12
   `theme.json` + `tokens.json` are byte-equal to v4's baseline files, but
   THREE `styles.css` (art-deco, great-estate, madmans-box) are NOT: v5
   rewrites every texture URL `/api/themes/assets/bundle:<id>/…` →
   `/themes/<id>/…` and madmans-box carries a v5-local `[data-icon=
   "thinking"]` block (P4.d17). All 18 move in the span (four commits:
   `58120a7fe` touches all 18, `cce44e098` 14, `10e09b7c3` 6, `86cc25482` 4;
   six `theme.json` version bumps, NO other `theme.json` hunk). **The rule
   (§D.3):** the vendor lane owns the 12 PURE byte copies (`theme.json` +
   `tokens.json` at the pin — and `tokens.json` IS read at runtime by
   `theme-preview-modal.ts:244`); P4.D357 owns the six `styles.css` (a
   rewrite, not a copy). No two lanes touch one file.
7. **Both public schemas move; the two specimen JSONs do NOT.**
   `qtap-export.schema.json` 109,559 → **115,274** (SEVEN commits, last
   writer `6588972d5`); `qtap-custom-tool.schema.json` 63,595 → **66,469**
   (`839575d89` alone — the `intervention` block);
   `CUSTOM_TOOL_SPEC.json` / `_DICE.json` / `qtap-progression.schema.json`
   unchanged. Four constants move (§G); ownership by the P4.D323 / P4.D339
   precedent goes to content lanes (export → P4.D341, custom-tool →
   P4.D345), the vendor lane re-runs the three guards on the union as a
   CHECK.
8. **NO-PORT ratifications ready with evidence (§C):** 22 whole rows
   (`332c650b9`, `4a25f7eab`, `505a524fa`, `cb94e6e99`, `5e18ede6a`,
   `a0905454b`, `3c9ae1fc3`, `8bf856050`, `8becfc6ce`, `7d7e38d2f`,
   `66f8c4d41`, `539bb4012`, `744596158`, `c50cdddc6`, `dd200c695`,
   `b09ff5c86`, `b892227eb`, `3a98a6cca`, `3c818c856`, `1ca45c171`,
   `9e01ebd25`, `236c04e35`) and 13 halves. ONE row the ledger classes
   NO-PORT? needs a content lane's eye before ratification: `cb94e6e99`
   also deletes `TurnToolSlate.enabledToolOptions` and `encodeDebugInfo` /
   `StreamDebugInfo` — v5 keeps `determine_enabled_tool_options`
   (`pseudo_tool_prompts_equivalence`), and the ledger asks for
   `orchestrator_tier3_equivalence.rs:1325`'s frame-drop to be retired as a
   rider (P4.D348's, not a pure ratification).
9. **`V4_APP_VERSION` lives at ONE site** —
   `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:126`
   `"4.11.0-dev.53"` → `"4.12.0-dev.0"` (§F.6). Note the CLI manifest
   `packages/quilltap/package.json` is **`4.11.0-dev.112`** at the pin (the
   release trio never touched it) — Tier R rows that print the CLI's own
   version must be checked against THAT string, not `4.12.0-dev.0`.
10. **Version guards:** `provider_sdk_version_guard.rs:128`
    `INSTALLED_ROOT_OPENAI_SDK = "7.34.0"` → `7.45.0` and `:132`
    `INSTALLED_ROOT_OPENROUTER_SDK = "1.4.25"` → `1.5.9` (ROOT only — every
    plugin dir still declares `openai ^7.23.0` / `@openrouter/sdk ^1.3.28` /
    `plugin-utils ^2.7.1`, and `plugins/dist` has NO tracked lockfile, so
    the recorded SDK constants and `RECORDED_PLUGIN_UTILS` do NOT move);
    `zod_version_guard` stays 4.6.7; `builtin_prompt_templates_guard`
    unmoved (zero span hunks under
    `plugins/dist/qtap-plugin-default-system-prompts/prompts/`). Whether the
    guard reads 7.45.0 or 7.45.1 depends on finding 1.

---

## §0 — The span for this slice, in one paragraph

Across `322a5488e..236c04e35` v4 shipped 4.11.0: keystone memories, the
inner circle and the importance rubric (`8cb584eff`, `a5efdbb97`,
`6fd2d5ffb`), Storyteller mode + declared tool effects (`b15fe9404`,
`946a50a04`), Pascal operator intervention (`839575d89`, `8ae8a127c`,
`b02d902f3`), the context-builder and turn-pipeline refactors with bugs
258–265 (`036b32a78`, `a8214b253`, `bdd09b75b`, `fa126bde8`, `f7a1789df`,
`d68c1af88`, `9a88f862f`), the three vault keys (`b27e5089a`, `eb90d9ed8`,
`6588972d5`), diagrams (`6588972d5`, `9faed0c7e`, `8bf856050`), the
Inventory chokepoints (`0276d10d9`, `265b711dd`, `074e1e01a`), the Novel
template + punctuation lint + prose mode (`778b97b2b`, `1dd0a0c01`,
`2b2b190b6`), the mount-index / files fixes (`40fab1c54`, `82a2f6536`,
`faeae2b71`, `026222176`), the Log in backups + terminal bugs 267/268
(`cc5f2347c`, `85c002822`), the WCAG programme (`58120a7fe`, `dff76ebef`,
`eed72d60c`, `68340008b`, `10e09b7c3`, `cce44e098`, `86cc25482`,
`2041bb875`), the settings fixes (`3b8b6f42b`, `7789ebed6`), NanoGPT bug 269
(`4b0d1cb04`), the CLI `--count` fix (`40cc6ffda`), and ~20 docs / process /
test / packaging commits. For THIS slice that means: `help/` grew by two
pages and moved thirty-five; `docs/` grew by sixteen net files through 42
operations (a feature-spec cleanup deleted three and moved two); both
public schemas moved; all 18 bundled-theme files moved; the root lockfile
moved 117 adds / 4 removals / 72 version changes with `pdf-parse` gone and
four diagram deps in; five plugin manifests bumped; the CLI package gained
a `keystone` verb, a Docker base dir and a `--count` fix; and four modules
were deleted from `lib/` / `components/`.

---

## §A — `help/`

### A.1 Counts and the diff

- `git ls-tree -r --name-only 322a5488e -- help | wc -l` = **135**; at
  `236c04e35` = **137**. v5 `help/` = 135 tracked, every file byte-equal to
  `git show 322a5488e:help/<p>` (re-measured 2026-10-29: 0 diffs).
- `git diff --stat 322a5488e 236c04e35 -- help/`: **37 files, +662 / −78**;
  `--name-status`: 2 `A`, 35 `M`, 0 `D` / `R`. No `url:` / `title:`
  front-matter line moves on an existing page (the only `url:` hunks are
  the two new pages').
- No spaced filename exists under `help/` at either end.

| Page | +/− | Commits (newest first, full-history) | New `##` / `###` headings | Content lane whose prose it is |
|---|---|---|---|---|
| `help/diagrams.md` (A) | +132 | `9faed0c7e`, `6588972d5` | `url: /aurora/:id/edit`; `# Diagrams`; `## Granting the Pen`; `## The Three Kinds of Drawing` (`### Flowcharts, Sequences, Timelines and the Like`, `### Webs of Connection`, `### Freeform Drawings`); `## Around Each Drawing`; `## The Viewing Room`; `## Whose Hand?`; `## While a Reply Is Arriving`; `## When the Pen Blots`; `## Diagrams and Quick-Hide`; `## Where Diagrams Render`; `## In-Chat Navigation`; `## Related Pages` | P4.D349 (server) / P4.D353 (SPA) |
| `help/storyteller-mode.md` (A) | +120 | `946a50a04`, `b15fe9404` | `url: /salon/:id`; `# Storyteller Mode`; `## Choosing the Mode`; `## A Round, Step by Step`; `## Bids and Their Tiers`; `## What the Chosen Character Is Told`; `## What Is Kept, and What Is Not`; `## Things That Skip the Board`; `## Silent Characters`; `## In-Chat Navigation`; `## Related Pages` | P4.D343 / P4.D344 |
| `help/appearance-settings.md` | +14 / −0 | `10e09b7c3`, `dff76ebef`, `58120a7fe` | "Legibility and Motion" (per the ledger) | P4.D357 / P4.D358 |
| `help/ariel.md` | +6 / −2 | `10e09b7c3`, `85c002822`, `c50cdddc6` | — | P4.D355 (267/268), P4.D358 (F6), `c50cdddc6` NO-PORT half (the backups sentence) |
| `help/autonomous-rooms.md` | +1 / −1 | `b15fe9404` | — | P4.D343 |
| `help/brahma-console.md` | +20 / −6 | `99463fc56` | — | P4.D359 |
| `help/character-editing.md` | +23 / −2 | `6588972d5`, `b27e5089a` | — | P4.D349 |
| `help/character-system-transparency.md` | +1 / −1 | `074e1e01a` | — | P4.D350 |
| `help/chat-multi-character.md` | +20 / −0 | `fa126bde8`, `b15fe9404`, `036b32a78` | — | P4.D347, P4.D343, P4.D348 |
| `help/chat-participants.md` | +19 / −6 | `b15fe9404` | — | P4.D343 / P4.D344 |
| `help/chat-settings-ai-services.md` | +2 / −0 | `bdd09b75b` | — | P4.D347 |
| `help/chat-settings-composer.md` | +18 / −0 | `2b2b190b6`, `1dd0a0c01` | — | P4.D352 |
| `help/chat-settings.md` | +1 / −0 | `839575d89` | — | P4.D345 |
| `help/chat-turn-manager.md` | +1 / −1 | `a8214b253` | — | P4.D348 |
| `help/cli-memories.md` | +15 / −1 | `8cb584eff` | — | P4.D341 (the `keystone` verb — NO-PORT by R-AL; the page is vendored regardless) |
| `help/cli-sync.md` | +3 / −1 | `faeae2b71` | — | P4.D354 |
| `help/connection-profiles.md` | +1 / −0 | `bdd09b75b` | — | P4.D347 |
| `help/custom-tools.md` | +38 / −0 | `b02d902f3`, `8ae8a127c`, `839575d89`, `b15fe9404` | — | P4.D345 (three), P4.D343 (one) |
| `help/data-directory.md` | +7 / −0 | `faeae2b71` | — | P4.D354 |
| `help/document-editing-tools.md` | +3 / −3 | `40fab1c54` | — | P4.D354 |
| `help/file-uploads.md` | +11 / −0 | `40fab1c54` | — | P4.D354 |
| `help/groups.md` | +8 / −0 | `a5efdbb97` | — | P4.D342 |
| `help/image-generation-profiles.md` | +2 / −0 | `4b0d1cb04` | — | P4.D356 |
| `help/inventory.md` | +4 / −2 | `0276d10d9`, `074e1e01a`, `265b711dd` | — | P4.D350 |
| `help/memory-maintenance.md` | +5 / −0 | `8cb584eff` | — | P4.D341 |
| `help/memory-recall-relevance.md` | +41 / −0 | `8cb584eff`, `a5efdbb97`, `6fd2d5ffb` | — | P4.D341, P4.D342 |
| `help/mount-points.md` | +6 / −0 | `faeae2b71` | — | P4.D354 |
| `help/pascals-workbench.md` | +4 / −1 | `cce44e098`, `839575d89`, `b15fe9404` | — | P4.D345, P4.D343, P4.D357 (one sentence) |
| `help/quick-hide.md` | +1 / −0 | `6588972d5` | — | P4.D353 |
| `help/roleplay-templates-settings.md` | +34 / −4 | `778b97b2b` | — | P4.D351 / P4.D352 |
| `help/roleplay-templates.md` | +22 / −5 | `2b2b190b6`, `778b97b2b` | — | P4.D352, P4.D351 |
| `help/salon-host-introductions.md` | +26 / −30 | `d64b25e96`, `9a88f862f`, `d68c1af88`, `f7a1789df` | — | P4.D347 (+ `d64b25e96`'s pronoun fix, help-only) |
| `help/system-backup-restore.md` | +3 / −0 | `c50cdddc6`, `cc5f2347c` | — | P4.D355 (+ the `c50cdddc6` half) |
| `help/system-llm-logs.md` | +11 / −8 | `eb2d87788` | — | P4.D348 |
| `help/the-log.md` | +1 / −1 | `cc5f2347c` | — | P4.D355 |
| `help/tools-settings.md` | +10 / −0 | `b15fe9404` | — | P4.D343 |
| `help/wardrobe.md` | +28 / −3 | `265b711dd`, `eb90d9ed8` | — | P4.D349, P4.D350 |

### A.2 `url:` front matter — the context resolver is unaffected

v5's resolver is first-match-wins in `readdirSync` order
(`crates/quilltap-host/build.rs` header; `files_store.rs`
`find_markdown_files`). At the pin `url: /aurora/:id/edit` is matched by
`character-editing.md`, `character-progressions.md`,
`character-subprompts.md`, `character-system-prompts.md`,
`character-system-transparency.md` BEFORE `diagrams.md`; `url: /salon/:id`
by `agent-mode.md` … `shared-character-vaults.md` BEFORE
`storyteller-mode.md` (18 pages share it). So neither new page changes any
resolved context; the Guide LIST order and the `help_docs` rowid order DO
change (two rows interleaved alphabetically — `diagrams` after
`data-directory`, `storyteller-mode` after `shared-character-vaults`), which
is what `help_tree_equivalence` re-measures.

### A.3 Where v5's copy lives and how it is guarded

- **The tree:** top-level `help/` (135 files, byte-equal to the baseline).
- **The embed:** `crates/quilltap-host/build.rs` walks `<repo>/help/**/*.md`
  in `readdirSync` order and writes `help_embedded.rs` into `OUT_DIR`;
  `crates/quilltap-host/src/help_content.rs` `include!`s it as
  `EMBEDDED_HELP`; `embedded_help_count()` DERIVES the count.
- **Guards (the four that move 135 → 137):**
  1. `crates/quilltap-harness/tests/help_tree_embed_guard.rs:114`
     `const VENDORED_FILE_COUNT: usize = 135;` (the doc comment at `:108-113`
     is the running history — append a `**137 at v4 \`236c04e35\` (135 + 2,
     P4.D360)**` line naming `diagrams.md` + `storyteller-mode.md`); the
     assert messages at `:131` / `:137` interpolate the constant.
  2. `crates/quilltap-host/tests/host_help_docs_boot.rs:131`
     `assert_eq!(expected, 135, "the vendored tree at v4 322a5488e");` — move
     the literal AND the message (`"… at v4 236c04e35"`), and the comment at
     `:127-130` + the header's `//!` lines 7 / 14 / 17 ("135 rows").
  3. `crates/quilltap-harness/tests/help_tree_equivalence.rs` — regenerated
     from the pin by `npx jest … harness/oracle/cases/help-tree-sync.test.ts`
     (recipe `:21-30`, `QT_ORACLE_HELP_TREE`); the history comment at
     `:125-160` gets its P4.D360 line ("the count MOVED 135 → 137; measured
     RED at the count before the copy; green after").
  4. `crates/quilltap-harness/tests/help_section_size_equivalence.rs` —
     regenerated by `npx tsx harness/oracle/cases/help-section-size.ts`
     (`:24-28`, `QT_ORACLE_HELP_SECTION_SIZE`; needs `js-tiktoken` 1.0.21 in
     the pin's `node_modules` — present in the cache); the two new pages add
     23 H2/H3 sections (14 + 9).
- **Readers that DERIVE the count (no edit):**
  `crates/quilltap-web/tests/help_web_routes.rs`,
  `crates/quilltap-harness/tests/help_doc_ensure_equivalence.rs`. The only
  other `135` literals under `crates/` are unrelated line references
  (`help_chat/summary_check.rs:2`, `orchestrator.rs:137`).
- The SPA hard-codes no help slug list (`ggrep -rn "'storyteller-mode'\|
  'diagrams'" apps/web/src` is empty).

### A.4 The rule, unchanged

P4.D323 R-1 / P4.D339 R-P: `help/` WHOLE, from the object store, nothing
shared — `for p in $(git -C ~/source/quilltap-server ls-tree -r
--name-only 236c04e35 -- help); do git -C ~/source/quilltap-server show
236c04e35:$p > $p; done` (no spaced names under `help/`, so the bare `for`
is safe here; use `while IFS= read -r` for `docs/`), in ONE commit with the
two literals, then regenerate the two equivalence families from the pin.
Each content lane's `HANDOFF:` cites the A.1 row for its prose; the unifier's
§S.5 byte-compare reads the vendored file.

---

## §B — `docs/` → v5 `docs/v4/`

### B.1 Counts

- v4 `docs/`: **501 at `322a5488e`, 517 at `236c04e35`** (`git ls-tree -r`):
  +19 adds, −3 deletes, 2 renames.
- v5 `docs/v4/`: **502 tracked** = 501 + `docs/v4/packages-quilltap-README.md`
  (v4's `packages/quilltap/README.md`). Re-measured 2026-10-29: every
  mirrored path is byte-equal to `git show 322a5488e:docs/<p>`; no MISSING;
  the README is the only v5-only file.
- After this round: 517 + 1 = **518** (`git ls-files docs/v4 | wc -l`).
- **Spaced filename:** `docs/developer/features/complete/Wardrobe UX
  Overhaul.md` is unchanged in the span; the copy loop must still be
  `git … ls-tree -r --name-only 236c04e35 -- docs | while IFS= read -r p`.
- **The release trio's bytes:** `docs/releases/4.11.0.md` is IDENTICAL at
  `1ca45c171` and `236c04e35` (`git diff 1ca45c171 236c04e35 -- docs/
  releases/4.11.0.md` is empty; the 7789ebed6 → 236c04e35 two-line change is
  `draft: true` → `false` + the heading); `docs/CHANGELOG.md` at the pin
  opens `### 4.12-dev` (empty) above `### 4.11.0`. Vendor every `docs/` byte
  from `236c04e35`.

### B.2 The operations (`git diff --name-status -M --numstat`, full-history attribution)

| v4 path (status) | +/− | Commits (full-history, newest first) | Op |
|---|---|---|---|
| `docs/CHANGELOG.md` (M) | +584 / −1 | all 63 non-merge commits (incl. `236c04e35`'s `### 4.12-dev` heading and `1ca45c171`'s `### 4.11.0`) | overwrite |
| `docs/FEATURES.md` (M) | +3 / −3 | `6588972d5`, `074e1e01a`, `b15fe9404` | overwrite |
| `docs/developer/API.md` (M) | +211 / −2 | `a4945d8d9` (+182, sixteen action sections), `0276d10d9`, `9a88f862f`, `946a50a04`, `839575d89`, `b15fe9404` | overwrite |
| `docs/developer/BACKGROUND_JOBS_CHILD.md` (M) | +2 / −0 | `b15fe9404` | overwrite |
| `docs/developer/CLI.md` (M) | +7 / −1 | `8cb584eff` (the `keystone` verb) | overwrite |
| `docs/developer/DDL.md` (M) | +56 / −6 | `6588972d5`, `1dd0a0c01`, `b27e5089a`, `778b97b2b`, `839575d89`, `b15fe9404`, `8cb584eff` — the five D23 moves' prose (P4.D341 reads it; this lane copies it) | overwrite |
| `docs/developer/DEAD-CODE-REPORT.md` (M) | +34 / −6 | `8becfc6ce` | overwrite |
| `docs/developer/DEVELOPMENT.md` (M) | +20 / −6 | `a4945d8d9`, `3a98a6cca` | overwrite |
| `docs/developer/PROMPT_ARCHITECTURE.md` (M) | +26 / −10 | `bdd09b75b`, `9a88f862f`, `d68c1af88` | overwrite |
| `docs/developer/SYSTEM_FLOWCHARTS.md` (M) | +70 / −8 | `bdd09b75b`, `a8214b253` | overwrite |
| `docs/developer/bugs.md` (M) | +43 / −24 | `dd200c695` (the FREEZE header), `4b0d1cb04`, `85c002822`, `026222176`, `bdd09b75b`, `a8214b253`, `40fab1c54`, `fa126bde8`, `036b32a78` | overwrite |
| `docs/developer/bugs/fixed/bug-258-whisper-starts-chain.md` (A) | 93 | `036b32a78` | add |
| `…/bug-259-rehydrated-attachment-cap.md` (A) | 65 | `fa126bde8` | add |
| `…/bug-260-uploads-unreadable-by-doc-tools.md` (A) | 86 | `40fab1c54` | add |
| `…/bug-261-list-files-glob-broken.md` (A) | 53 | `40fab1c54` | add |
| `…/bug-262-fair-rotation-thin-post.md` (A) | 73 | `a8214b253` | add |
| `…/bug-263-budget-blind-to-trailing-sections.md` (A) | 74 | `bdd09b75b` | add |
| `…/bug-264-group-compression-kept-history.md` (A) | 52 | `bdd09b75b` | add |
| `…/bug-265-compression-window-miscount.md` (A) | 43 | `bdd09b75b` | add |
| `…/bug-266-pdf-preview-render-race.md` (A) | 51 | `026222176` | add |
| `…/bug-267-orphaned-terminal-transcripts.md` (A) | 59 | `85c002822` | add |
| `…/bug-268-terminal-kill-ignored.md` (A) | 46 | `85c002822` | add |
| `…/bug-269-nanogpt-size-not-fitted.md` (A) | 84 | `4b0d1cb04` | add — the LAST catalogue entry (`dd200c695` froze it) |
| `docs/developer/docker-test-plan.md` (A) | 253 | `faeae2b71` | add |
| `docs/developer/features/ROADMAP.md` (M) | +6 / −31 | `b09ff5c86`, `6588972d5`, `839575d89`, `8cb584eff` | overwrite |
| `docs/developer/features/comfy_ui_local_image.md` (D) | −355 | `b09ff5c86` | **delete** `docs/v4/developer/features/comfy_ui_local_image.md` (present in v5 today) |
| `docs/developer/features/complete/capabilities-report-update.md` (M) | +0 / −1 | `b09ff5c86` | overwrite |
| `…/complete/composer-emoji.md` (M) | +4 / −4 | `b09ff5c86` | overwrite |
| `…/complete/composer-slash-commands.md` (M) | +1 / −1 | `b09ff5c86` | overwrite |
| `docs/developer/features/composer-typeahead.md` → `complete/composer-typeahead.md` (R084, +10 / −116) | 84 % | `b09ff5c86` | **move** (`git mv` the mirror, then overwrite with the pin's bytes — its Phase 3 was cut) |
| `…/complete/diagrams.md` (A) | 252 | `9faed0c7e`, `6588972d5` | add |
| `…/complete/inner-circle-recall.md` (A) | 699 | `8cb584eff` (born at `6fd2d5ffb` as the 560-line spec, moved + grown) | add |
| `…/complete/inventory.md` (M) | +8 / −0 | `0276d10d9` | overwrite |
| `…/complete/nanogpt-lora.md` (M) | +1 / −2 | `b09ff5c86` | overwrite |
| `…/complete/pascal-operator-intervention.md` (A) | 516 | `b02d902f3`, `839575d89` (born `332c650b9` at `features/`, moved by `839575d89`) | add at the FINAL path |
| `…/complete/storyteller-mode.md` (A) | 487 | `946a50a04`, `b15fe9404` | add |
| `docs/developer/features/svar-bridge-spike-findings.md` → `complete/svar-bridge-spike-findings.md` (R096, +2 / −2) | 96 % | `b09ff5c86` | **move** + overwrite |
| `docs/developer/features/composer-document-references.md` (A) | 112 | `b09ff5c86` | add (a proposal — nothing shipped) |
| `docs/developer/features/roleplay-block-narration.md` (D) | −189 | `b09ff5c86` | **delete** |
| `docs/developer/features/svar-file-manager-implementation-plan.md` (M) | +1 / −1 | `b09ff5c86` | overwrite |
| `docs/developer/features/usage-tracking.md` (D) | −119 | `b09ff5c86` | **delete** |
| `docs/releases/4.11.0.md` (A) | 416 | `1ca45c171` (`draft: false`), `a4945d8d9`, `b892227eb`, `3c9ae1fc3`, `505a524fa`, `4a25f7eab` | add |
| `packages/quilltap/README.md` (M, mirrored as `docs/v4/packages-quilltap-README.md`) | +5 / −4 | `539bb4012`, `8cb584eff`, `a5efdbb97` | overwrite |

**18 overwrites + 19 adds + 3 deletes + 2 moves + the README overwrite = 43
operations.** The ledger's `b09ff5c86` row ("six mirrored paths deleted /
moved and two `complete/` pages edited") under-counts: FIVE source paths
vanish or move, ONE page is born, SIX `complete/` / `features/` pages are
edited (`ROADMAP`, `capabilities-report-update`, `composer-emoji`,
`composer-slash-commands`, `nanogpt-lora`, `svar-file-manager-
implementation-plan`). Not in `docs/` and NOT vendored (NO-PORT infra, §C):
`README.md`, `CLAUDE.md`, `.claude/**`, `.github/ISSUE_TEMPLATE/bug.yml`,
`packages/theme-storybook/**`, `scripts/**`, `next.config.js`,
`tsconfig.build.json`, `knip.json`, `public/pdf.worker.mjs`.

### B.3 The `self_inventory` release-notes seam (reference-only confirmed)

`crates/quilltap-core/src/tools/self_inventory.rs:30-45` documents the env
seam: `SelfInventoryEnv.release_notes: Option<(String, String)>` (`:100`) and
`changelog` are host-environment inputs. The host builds them in
`crates/quilltap-host/src/env.rs:238` `production_self_inventory_env(version,
docs_dir: Option<&Path>, db)` → `find_release_notes(docs_dir, version)`
(`:184`, reads `<docs_dir>/releases/*.md`, "newest at or below" the version)
and `read_changelog` (`:216`, `<docs_dir>/CHANGELOG.md`). The ONLY production
caller is `crates/quilltap-host/src/spine.rs:3781`, whose struct field
`docs_dir: Option<PathBuf>` (`:3737`) defaults to **`None`** (`:3752`) and
nothing sets it (`ggrep -rn docs_dir crates/quilltap-web/src
crates/quilltap-tauri/src` is empty). So v5 answers `quilltap.releaseNotes`
/ `quilltap.changelog` as "not found" today and `docs/v4/` is NOT a build or
runtime input — the mirror stays reference-only; vendoring `4.11.0.md` is a
pure mirror op. (Whether a host should point `docs_dir` at a bundled
`docs/v4/` is a standing deferral, not this round's — §J.4.)

---

## §C — The NO-PORT ratification record (for §S.6)

The record's home is the lane's `status-log.md` block "### NO-PORT
ratifications (for §S.6)" (the P4.D339 precedent at `status-log.md:192265`;
shape per P4.D339 R-9: per row the `--stat`, the code-path census
`git show --stat --format= <sha> -- lib app packages plugins components help
public migrations themes` which must print NOTHING for a whole row, the
decisive hunk, the v5 site or "nothing in v5 corresponds", the residue and
its owner, the proposed §6 status). Re-measured 2026-10-29; every count is
`git show --stat --format=` against the commit's first parent.

### C.1 Whole rows (22) — ratify `NO-PORT-RATIFIED(P4.D360)`

| sha | subject | `--stat` | code-path census | evidence / residue |
|---|---|---|---|---|
| `332c650b9` | Docs: spec for operator intervention | 2 files +498 (`features/pascal-operator-intervention.md` NEW 492, CHANGELOG) | NOTHING | superseded by `839575d89`'s move to `complete/`; reaches v5 at the final path (B.2) |
| `4a25f7eab` | Docs: draft 4.11.0 release notes | 2 files +329 | NOTHING | `docs/releases/4.11.0.md` born; vendored at its `236c04e35` bytes (B.2) |
| `505a524fa` | Docs: release notes corrections | 2 files +8 / −3 | NOTHING | same file |
| `3c9ae1fc3` | Docs: release notes corrections, part 2 | 1 file +84 / −11 | NOTHING | same file |
| `b892227eb` | Docs: release notes corrections, part 3 | 1 file +23 / −7 | NOTHING | same file |
| `cb94e6e99` | Remove the unread `debugLLMRequest` SSE frame | 10 files +5 / −159 (prod: `streaming.service.ts` −`encodeDebugInfo` / `StreamDebugInfo`, `types.ts`, the slate's `enabledToolOptions`) | **NOT nothing** — it is a code commit whose frame v5 never emitted (`orchestrator.rs` / `chat_events.rs` call it "unported") | ratify as NO-PORT **with P4.D348's rider**: `orchestrator_tier3_equivalence.rs:1325` drops v4's frame — vacuous at this pin, retire the drop; keep `pseudo_tool::determine_enabled_tool_options` (v4 keeps the export, `pseudo_tool_prompts_equivalence` still drives it) |
| `5e18ede6a` | Merge origin/main into main | conflicts in README / CHANGELOG / both `package.json`s / the lock only; dropped the four diagram deps | n/a (merge) | restored by `a0905454b`; nothing to port |
| `a0905454b` | Fix: restore diagram dependencies dropped by merge | 5 files +1,256 / −88; `git diff 6588972d5 a0905454b -- package.json` = the version line | NOTHING outside `package.json` / the lock | the deps themselves are `6588972d5`'s ruling (P4.D353) |
| `8bf856050` | Chore: prune diagram trace logging | 4 files +4 / −4 (three client components) | client-only `console.debug('[diagrams] …')` lines | P4.D353 ports the post-prune shape |
| `8becfc6ce` | Chore: dead code sweep for v4.11 | 6 files +38 / −43 — DELETES `components/inventory/index.ts` (barrel), `resolveMemoryMaintenance` (`lib/memory/settings.ts`), `isComputedTool` (`lib/tools/effects.ts`), `ProjectFileInfo` (`project-info-tool.ts`); `DEAD-CODE-REPORT.md` | four deletions, no behaviour | **IMPORT-DEAD census re-run: the oracle side imports NONE of the four** (§E.1); P4.D343 ports `effects.ts` without `isComputedTool` |
| `7d7e38d2f` | Test: toast stacking + storyteller keepOrder | 3 files +144 (two tests + CHANGELOG) | NOTHING | spec input for P4.D343's planner and P4.D358's toast selector |
| `66f8c4d41` | Bundle plugin-types in two plugins | 9 files +16 / −17 — two `plugins/dist/*/index.js` rebundles, manifests `default-system-prompts` 1.1.26 → **1.1.27**, `openai-compatible` 1.0.49 → **1.0.50** | plugin bundling only | v5 has no JS plugins; `builtin_prompt_templates_guard` reads `prompts/*.md` (unmoved) |
| `539bb4012` | Docs: CLI README lists memories keystone | 2 files +6 / −2 | NOTHING (`packages/quilltap/README.md` is docs) | mirrored as `docs/v4/packages-quilltap-README.md` (B.2) |
| `744596158` | Merge remote-tracking branch 'origin/main' | conflict ONLY in `docs/CHANGELOG.md` | n/a (merge) | — |
| `c50cdddc6` | Document terminal sessions as excluded from backups | 6 files +238 / −1 — `help/ariel.md`, `help/system-backup-restore.md`, a release-checklist line, NEW `terminal-sessions-excluded.test.ts`, a mock fix | NOTHING under `lib app …` (help is vendored, not code) | ratify the CODE half; the help halves ride A.1. **Confirm at unification** that v5's backup leaves `terminal_sessions` out (the pin's rule — expected) |
| `dd200c695` | Docs: move bug filing to GitHub Issues; freeze at Bug 269 | 5 files +140 / −38 — NEW `.github/ISSUE_TEMPLATE/bug.yml` (90), v4 `CLAUDE.md`, `bugs.md` FROZEN header, `update-documentation.md`, CHANGELOG | NOTHING | a PROCESS change already absorbed by this port (CLAUDE.md "v4 bugs found during the port are GitHub issues"; memory `v4-bugs-are-github-issues`); `docs/v4/developer/bugs.md` re-vendors (B.2) |
| `b09ff5c86` | Docs: feature-spec cleanup | 19 files +149 / −830; ONE source hunk = a doc-path comment in `components/chat/lexical/typeahead/types.ts` | ONE comment line | B.2's 3 deletes + 2 moves + 1 add + 6 edits |
| `3a98a6cca` | Docs: release checklist item 13 (UI / WCAG); docs → 14 | 16 files +84 / −19 — NEW `.claude/commands/release-checklist-13-ui.md`, twelve renumbered checklists, `DEVELOPMENT.md` | NOTHING | the checklist is P4.D357 / P4.D358's *specification* (read it); `DEVELOPMENT.md` re-vendors |
| `3c818c856` | Docs: release-notes procedure in checklist 14 | 3 files +65 / −2 | NOTHING | CHANGELOG re-vendor only |
| `1ca45c171` | release: 4.11.0 | vs `7789ebed6`: 5 files +6 / −6 (`package.json` / lock `4.11.0`, README badge, CHANGELOG heading, `draft: false`) — its stat against its parent `2432b9057` is the whole 4.10 → 4.11 squash | n/a | VERIFIED BY CONTENT; the code IS `7789ebed6` |
| `9e01ebd25` | merge: 4.11.0 back into main | first-parent diff = the five stamp files (+6 / −6) | n/a | the release commit is now an ANCESTOR of `main` |
| `236c04e35` | dev: started 4.12.0 development | 4 files +6 / −4 (`4.12.0-dev.0` ×3, `### 4.12-dev`) | NOTHING | the pin |

### C.2 PARTIAL NO-PORT halves of PORT rows (13) — ratify the half, name the owner of the rest

| sha | the NO-PORT half (evidence) | the PORT half's owner |
|---|---|---|
| `82a2f6536` | `lib/background-jobs/child/child-entry.ts` + NEW `child-startup.ts` (connect the forked child's DB backend before plugins) — a Node fork artefact; v5's in-process job runner shares the host's connections, there is no child (`crates/quilltap-host/src/spine.rs`) | P4.D354 (`requireMountIndexDb()` ahead of `docMountChunks.findById` in `handleMountChunkEmbedding`) |
| `026222176` | `pdf-parse` REMOVED + `pdfjs-dist` 5.7.284 → 6.4.299 + `public/pdf.worker.mjs` (+12,180 / −10,272) + eight `plugins/dist/*/index.js` rebundles + the dependency moves (§F) + `FilePreviewPdf.tsx`'s canvas race (bug 266 — v5's Files preview has NO `pdfjs-dist` / no canvas arm: `apps/web/src/app/screens/files/file-preview-modal.ts:39` "PDF renders a download prompt (v4 uses `pdfjs-dist`, not a v5 dependency)" — the only `pdfjs` mention under `apps/web/src`) | P4.D354 (the WARN text `PDF reader found no text, using native fallback extraction` at `generators/file_content.rs:341` / `:499` + the converter spec + the `file-content-extractor` re-aim) |
| `faeae2b71` | `scripts/start-quilltap-docker.ts` (`docker stop -t 30` before `rm -f`), `next.config.js` + NEW `tsconfig.build.json`, a `.claude` command, `packages/quilltap/lib/__tests__/platform-paths.test.js` — v4 build / script only; the recreate-lockout half is v4-script-only (v5's `quilltap-web` releases the lock on SIGTERM, dogfood #153) | P4.D354 (the persistent-target guard; `browse-directory`'s Docker default; the CLI base dir — §H) |
| `66f8c4d41` | whole (C.1) | — |
| `cb94e6e99` | whole (C.1) with P4.D348's rider | P4.D348 |
| `8becfc6ce` | whole (C.1) | P4.D343 (`effects.ts` without `isComputedTool`) |
| `7d7e38d2f` | whole (C.1) | — |
| `3a98a6cca` | whole (C.1) | — |
| `3c818c856` | whole (C.1) | — |
| `a4945d8d9` | `CLAUDE.md` (a WCAG dialog rule), `README.md`, `docs/developer/API.md` (+182), `DEVELOPMENT.md`, `releases/4.11.0.md` — docs / process; mirrored by B.2 | P4.D359 (`app/about/AboutView.tsx`: the "Markdown & Documents" credit line grows; NEW "Diagrams:" and "Terminals & Realtime:" lines — a judgment call against v5's own credits, not a byte port) |
| `dd200c695` | whole (C.1) — the process change is already this port's rule | — |
| `58120a7fe` | the `create-quilltap-theme` dependency DROP (`package.json`; lock entry removed) + `knip.json` (−1), the storybook 1.0.89 and starter templates, the toast-stacking test's selector | P4.D357 (tokens / CSS / six theme `styles.css`) + P4.D360 (the 12 `theme.json` / `tokens.json` byte copies — §D) + P4.D358 (toast role, modal focus) |
| `dff76ebef` / `68340008b` / `eed72d60c` / `10e09b7c3` | the `plugins/dist/*/index.js` rebundles (nine plugins +2,935 / −2,311 each across the span — `dff76ebef` rebuilt them, `68340008b` REVERTED that rebuild; nets to the `66f8c4d41` state), `scripts/generate-icon-css.ts` (+10, the forced-colors rule's generator — P4.D358 ports the generated `_icons.css` rule, not the script), storybook 1.0.90 | P4.D358 (semantics), P4.D357 (CSS / themes) |
| `cce44e098` / `86cc25482` | `packages/theme-storybook/**` 1.0.87 / 1.0.88 mirrors (`qt-components.css` +982 across the span, `quilltap-defaults.css` +32 / −…, CHANGELOG +15, manifest + lock) — v5 has no storybook (precedent: `drift-ledger.md:619,1566`) | P4.D357 |
| `dd200c695`'s `.github/ISSUE_TEMPLATE/bug.yml`, every `.claude/commands/*` (10 files in the span), v4 `CLAUDE.md` (+34 / −…), `README.md` (+21 / −…; the badge `dev.53` → `4.12.0-dev.0` plus prose), every `__tests__/**` + `lib/**/__tests__/**` + `__snapshots__` (**140 test paths; 139 files +11,329 / −482**), `packages/quilltap/lib/__tests__/platform-paths.test.js` | v4-internal; each content lane mirrors the cases it needs as tier-3 corpora (e.g. `7d7e38d2f`'s `plan-run-order.test.ts` for P4.D343) | — |

The storybook / `.claude` / `CLAUDE.md` / `README.md` / `scripts` / build-
config census for the record: `git diff --stat 322a5488e 236c04e35 --
.claude CLAUDE.md README.md scripts .github next.config.js
tsconfig.build.json knip.json packages/theme-storybook themes/tools` = **31
files, +1,369 / −104** (no `themes/tools` hunk this span).

### C.3 Ledger corrections carried forward

- `b09ff5c86`: "six mirrored paths deleted / moved" → **five** source paths
  (3 `D`, 2 `R`) + one `A` + six edits (B.2).
- The ledger §1 "Help / docs" bullet's "~20 pages edited" → **35 `M` + 2
  `A`** at the pin (A.1); its list of commits per page was the 10-27 probe's.
- `cb94e6e99` is classed `NO-PORT?` but is a CODE commit on the chat spine
  (it also deletes `TurnToolSlate.enabledToolOptions`) — ratify with
  P4.D348's rider, not blind (C.1).
- `8cb584eff`'s R-AL note ("the verb is NO-PORT, but v5's captured
  completion templates list memories verbs and will move") is confirmed by
  the bytes: three completion templates gain `keystone` / `--keystone` /
  `--off` lines (§H).
- `026222176`'s row lists `next` 16.4.3 — the lock moves 16.4.2 → 16.4.3
  (correct); the row omits `@openrouter/sdk` 1.4.25 → **1.5.9** at the lock
  level (it names 1.5.9 only as the manifest range) — §F.1 has the full
  delta.

---

## §D — Theme packs

### D.1 What moves in v4

`git diff --stat 322a5488e 236c04e35 -- themes/`: **18 files, +241 / −142**
— every one of the six bundled themes' `styles.css`, `theme.json`,
`tokens.json` (`themes/bundled/madmans-box/{README,PROMPTS-forge-calliope}.md`
and every `icons/` / `fonts/` / `textures/` file are UNMOVED). Per file
(full-history, `1ca45c171` dropped):

| File | Commits | Note |
|---|---|---|
| `art-deco/styles.css` +… | `58120a7fe`, `cce44e098` | light gold → NEW `--deco-bronze` |
| `art-deco/theme.json` | `58120a7fe`, `cce44e098` | `"version": "1.0.11"` → **`"1.0.13"`** — the ONLY hunk |
| `art-deco/tokens.json` | `58120a7fe` | 24 lines (12 token values) |
| `earl-grey/styles.css` | `10e09b7c3`, `58120a7fe`, `cce44e098` | `:focus-visible` → `var(--color-ring)` |
| `earl-grey/theme.json` | `10e09b7c3`, `58120a7fe`, `cce44e098` | `1.3.7` → **`1.3.10`** |
| `earl-grey/tokens.json` | `58120a7fe` | 22 lines |
| `great-estate/styles.css` | `10e09b7c3`, `58120a7fe`, `86cc25482`, `cce44e098` | +31 light-mode sidebar; `--estate-bronze` |
| `great-estate/theme.json` | same four | `1.0.6` → **`1.0.10`** |
| `great-estate/tokens.json` | `58120a7fe`, `cce44e098` | 24 lines |
| `madmans-box/styles.css` | `58120a7fe`, `cce44e098` | +18 / −1 (the diagram pane — rides UNPORTED `6588972d5`) |
| `madmans-box/theme.json` | `58120a7fe`, `cce44e098` | `1.1.7` → **`1.1.9`** |
| `madmans-box/tokens.json` | `58120a7fe` | 12 lines |
| `old-school/styles.css` | `58120a7fe`, `cce44e098` | 10 lines |
| `old-school/theme.json` | `58120a7fe`, `cce44e098` | `1.0.12` → **`1.0.14`** |
| `old-school/tokens.json` | `58120a7fe`, `cce44e098` | 24 lines |
| `rains/styles.css` | `58120a7fe`, `86cc25482`, `cce44e098` | `--qt-chat-user-fg` → `hsl(var(--text-000))` |
| `rains/theme.json` | `10e09b7c3`, `58120a7fe`, `86cc25482`, `cce44e098` | `1.3.10` → **`1.3.14`** |
| `rains/tokens.json` | `10e09b7c3`, `58120a7fe`, `cce44e098` | 36 lines (dark mutedForeground 59.2% → 62%) |

`git diff 322a5488e 236c04e35 -- 'themes/bundled/*/theme.json' | grep -v
'"version"'` is EMPTY — the six manifests move by their version string only.

### D.2 What v5 holds today (measured)

`apps/web/public/themes/<id>/` holds `styles.css`, `theme.json`,
`tokens.json` (+ `fonts/`, `textures/`; madmans-box also
`icons/{brand,hand-raised,inventory,stop-sign}.svg`, all four byte-equal to
v4 at both ends). Byte comparison against v4's baseline:

- **`theme.json` ×6 and `tokens.json` ×6: EQUAL to the baseline** (pure
  byte copies, P4.5 onward).
- **`styles.css`: earl-grey, old-school, rains EQUAL; art-deco,
  great-estate, madmans-box NOT EQUAL** — the diffs are (a) every texture
  URL rewritten `url('/api/themes/assets/bundle:<id>/textures/…')` →
  `url('/themes/<id>/textures/…')` (art-deco `:593` / `:603`, great-estate
  `:587` / `:671`, madmans-box `:553`), and (b) madmans-box's v5-local
  `[data-theme="madmans-box"] [data-icon="thinking"] { --_qt-icon-mask:
  url("/themes/madmans-box/icons/brand.svg"); … }` block + three comment
  edits (P4.d17 unit 5, `d1021fdd3`; the whisper-label WCAG fix `2fba14a97`
  of 2026-08-01 is already in v4's baseline bytes). So a v5 `styles.css` is
  v4's bytes **after a documented URL rewrite** — not a byte copy, and the
  WCAG lane must apply the rewrite to the pin's bytes (or `sed` the three
  URL forms) rather than copy.
- **Runtime readers:** `apps/web/src/app/theme/theme.service.ts:221` loads
  `/themes/<id>/styles.css` and `:248` the fonts; `screens/settings/
  appearance/theme-preview-modal.ts:244` fetches `/themes/<id>/tokens.json`
  (the preview card renders the token values — so `tokens.json` moves are
  USER-VISIBLE in v5); no v5 code reads `theme.json`'s `version` (it is a
  manifest for hosts that read it; `scoped-theme-css.ts:119` documents the
  `tokens.json` shape).

### D.3 The ownership rule (stated for the WCAG survey)

- **P4.D360 (vendor) owns the 12 PURE byte copies:**
  `apps/web/public/themes/{art-deco,earl-grey,great-estate,madmans-box,
  old-school,rains}/{theme.json,tokens.json}` = `git show
  236c04e35:themes/bundled/<id>/<file>`, byte-exact (six version bumps, six
  token files). ONE commit, SPA bump by the round's rule (the P4.D339
  precedent bumped the SPA for the theme commit).
- **P4.D357 (WCAG tokens / CSS) owns the six `styles.css`** — v4's pin bytes
  with the v5 URL rewrite re-applied and madmans-box's local block kept
  (a transformation, not vendoring; the lane must state the three-URL
  rewrite as a rule and prove it with a `diff` that shows ONLY those lines
  + the local block against the pin).
- **Nobody edits `apps/web/public/images/icons/`** this round
  (`git diff --stat 322a5488e 236c04e35 -- public/images` is EMPTY — no new
  icon in the span).
- The alternative (ONE owner for all 18 — P4.D357) is simpler for the
  unifier if the planner prefers it; the cost is that the token values and
  manifests then land only with the CSS lane. Either way no two lanes touch
  one theme file. The WCAG survey should claim the six `styles.css` and
  cite this section for the twelve it does not claim.

---

## §E — The IMPORT-DEAD + RUNTIME-BREAK census (round-wide)

### E.0 Method and verdict

- Files: `git diff --name-status -M --diff-filter=DRC 322a5488e 236c04e35
  -- lib app packages plugins migrations components` → **4 `D`, 0 `R` / `C`**:
  `components/inventory/index.ts` (`8becfc6ce`), `lib/chat/context/index.ts`
  (the barrel, `bdd09b75b`), `lib/chat/context/mentioned-characters.ts`
  (`9a88f862f`), `lib/tools/destructive-tools.ts` (`b15fe9404`).
- Names: `git diff -U0 … | grep '^-\s*export'` → **61 removed `export`
  lines** (file-attributed; the full list is in the scratchpad's
  `removed-exports.txt`); each checked for an export of the same name
  ANYWHERE under `lib app components packages` at the pin
  (`ggrep -rn -P '^\s*export\b[^;]*\bNAME\b'` in the pin worktree).
- Importers: every `import … from '@/…'` (static, 1,342 named non-type
  pairs over 408 distinct specifiers) and every `await import('@/…')`
  destructuring (5,923 pairs) under `harness/**` and `crates/**` was
  resolved against the pin's modules; `apps/web/src` imports nothing from
  v4. Dynamic-import names that resolve through `export * from` barrels
  (`@/lib/schemas/types`, `@/lib/chat/turn-manager`, `@/lib/memory/
  extraction`, `@/lib/memory/index-store`, `@/lib/services/chat-message`,
  `@/lib/tools`, `@/lib/pascal/dice`, `@/lib/database/backends/sqlite`)
  were each spot-verified at their real home (all alive).
- **Verdict: 8 dead sites, all leaf `cases/*`; 0 in `harness/oracle/lib/**`
  or `harness/oracle/fixtures/**`; 0 in `crates/**/tests` recipes beyond the
  case names; no `REAIM` lane.** 1 GARBLE, 7 REDDEN signature widenings.

### E.1 The dead sites, each with its owner

| Oracle case (v5 family; env) | Dead import | At the pin | Re-aim | Owner |
|---|---|---|---|---|
| `cases/context-compression.ts:` (`context_compression_equivalence.rs`; `QT_ORACLE_CONTEXT_COMPRESSION`) | `shouldApplyCompression` from `@/lib/chat/context/compression` | RENAMED + RESHAPED: `compression.ts:117 shouldApplyBudgetCompression(totalEstimatedTokens: number, maxAvailable: number, settings: ContextCompressionSettings, bypassCompression: boolean)` (the baseline's `(messageCount, settings, bypass)` is gone — bugs 263 / 265 changed WHAT is tested, not just the name) | re-aim the case at the new predicate with the new inputs; v5's `services/build_context.rs` twin changes with it | **P4.D347** |
| `cases/memory-injector.ts` (`memory_injector_equivalence.rs`; `QT_ORACLE_MEMORY_INJECTOR`) | `formatSummaryForContext` from `@/lib/chat/context/scene-state` | GONE (the pin's `scene-state.ts` exports `SceneStateEmissionEntry`, `formatCurrentSceneState` only; the summary formatting moved into `turn-scene-state.ts` / `turn-recall.ts` un-exported) | the family's rows for that function retire or re-aim at the turn module; v5 `memory_injector.rs` | **P4.D347** |
| `cases/skip-signal.ts:25` (`skip_signal_equivalence.rs`, `turn_control_equivalence.rs`; `QT_ORACLE_SKIP_SIGNAL`) | `buildTurnSkipInstruction` from `@/lib/chat/context-manager` | MOVED: `lib/chat/turn-manager/skip-signal.ts:428` (same two positional params `characterName, recentlyAddressed`) | change the specifier | **P4.D348** |
| `cases/item-folders.ts` (`inventory_folders_equivalence.rs`) | `inventoryFolderStorePath`, `normalizeInventoryFolderPath` from `@/lib/inventory/folders` | GONE (`0276d10d9`): the rules-parameterised `lib/mount-index/item-folders.ts:224 itemFolderStorePath(rules, folder)` / `:99 normalizeItemFolderPath(rules, …)` / `:132 normalizeItemFolderPathOrRoot(rules, …)` and the Inventory-specific `lib/inventory/folders.ts:97 normalizeInventoryFolderPathOrRoot(input)` | re-aim through `INVENTORY_FOLDER_RULES` | **P4.D350** |
| `cases/context-feeders-leaves.ts` (`context_feeders_leaves_equivalence.rs`; `QT_ORACLE_CONTEXT_FEEDERS_LEAVES`) | `findIntroducedOffSceneCharacterIds` from `@/lib/services/host-notifications/writer` | GONE (`9a88f862f` deleted the per-turn off-scene posting; `ggrep -rn introducedOffScene lib` is empty) | the rows retire; v5 `services/off_scene.rs` follows the server port | **P4.D347** |
| `cases/mentioned-characters.ts:18` (`mentioned_characters_equivalence.rs`; `QT_ORACLE_MENTIONED_CHARACTERS`) | `findMentionedCharacterIds` from `@/lib/chat/context/mentioned-characters` | MODULE DELETED (`9a88f862f`; no `mentionedCharacter` export anywhere under `lib/` at the pin — the referent ledger replaces it) | the family retires or re-aims at `referent-ledger.ts`; v5 has TWO twins to rule on — `crates/quilltap-core/src/mentioned_characters.rs` and the SPA's TS port at `apps/web/src/app/chat/skip-signal-helpers.ts:164` (its doc comment `:8` still names the deleted module; "no client consumer since v4 `e22f7b36`") | **P4.D347** |
| `cases/tool-build.test.ts:54` (`tool_build_equivalence.rs`; `QT_ORACLE_TOOL_BUILD`) | `DESTRUCTIVE_TOOL_NAMES` from `@/lib/tools/destructive-tools` | MODULE DELETED (`b15fe9404`); the ceiling lives in `lib/tools/effects.ts` (`BUILT_IN_TOOL_EFFECTS :107`, `toolEffectSpecOf :191`, `effectOfToolCall :199`, `maxEffectOfTool :210`, `highestToolEffect :217`, `describeCeilingRefusal :226`) over `effect-types.ts` (`ToolEffect = 'none' \| 'read' \| 'write' \| 'destroy'`, `TOOL_EFFECT_RANK`, `UNDECLARED_TOOL_EFFECT = 'write'`, `computedToolEffect`) | re-aim — AND see E.3's `buildTools` GARBLE | **P4.D343** |
| `cases/file-content-extractor.test.ts:149` (`file_content_extractor_equivalence.rs`, `common_helpers_selftest.rs`, `common/mod.rs`; `QT_ORACLE_FILE_CONTENT_EXTRACTOR`) | `require.resolve('pdf-parse', { paths: [process.cwd()] })` (a jest mock target) | PACKAGE REMOVED (`026222176`); `convertPdfBufferToText` reads `pdfjs-dist/legacy/build/pdf.mjs` directly | mock the new reader module instead; the WARN text moves | **P4.D354** |

Alive at the pin despite the brief (no re-aim): `cases/canonicalize.ts` and
`cases/tool-definitions-canonical.ts:17` import `UniversalTool` from
`@/lib/plugins/interfaces/tool-plugin` — exported at `:23` (a type, erased
by tsx anyway); `orchestrator-tier3.test.ts:756`'s `const { handleSendMessage
} = await import('@/lib/services/chat-message/orchestrator.service')` —
`orchestrator.service.ts:154` still exports it (the `-export` line was a
header re-declaration); `cases/post-office-host.ts`'s `buildTurnYieldContent`
— `writer.ts:384`. `willExceedContextLimit` (removed from
`context-manager.ts`) has NO oracle importer.

### E.2 Type-only removals with oracle references (erased by tsx; no action)

`MessageServiceContext`, `StreamDebugInfo`, `BuildToolsExtras`
(`streaming.service.ts` / `types.ts` — `cb94e6e99` / `a8214b253`),
`MemoryCreateInput` (re-declared at `memories.repository.ts:27`),
`InventoryState` / `INVENTORY_STATES` / `InventoryStateValueSchema` /
`INVENTORY_STATE_VALUES` (`0276d10d9` — moved; the Inventory cases import
the schema module, which loads), `PolicyProtection` type (re-declared),
`SelfInventoryVaultIncludedParts`, `ProjectFileInfo`,
`PLUGIN_TYPES_VERSION = '2.8.0'` → `'2.9.0'` (`packages/plugin-types/src/
index.ts`; v5 has no consumer).

### E.3 Exported-function signatures the oracles call — the diff across the span

Method: for every non-type name an oracle case imports (static or dynamic),
the `export (async )?function NAME(…)` header's parameter text at
`322a5488e` vs `236c04e35`.

| Function (module) | Baseline params | Pin params | Class | Oracle caller → owner |
|---|---|---|---|---|
| `buildTools` (`lib/services/chat-message/streaming.service`) | `(connectionProfile, imageProfileId, imageProfile, userId, projectId?, …)` positional | **`(options: BuildToolsOptions)`** | **GARBLE** — a positional call compiles under tsx and runs with `options = connectionProfile`; every other field `undefined` | `cases/tool-build.test.ts` (already IMPORT-DEAD on `DESTRUCTIVE_TOOL_NAMES`) → **P4.D343** |
| `generateRenderingPatterns` (`lib/chat/annotations`) | `(delimiters, narrationDelimiters?)` | `(delimiters, narrationDelimiters?: NarrationDelimiters \| null, options: TemplateRenderingOptions = {})` | REDDEN (additive; `null` now legal) | `annotations-rendering-patterns.ts` → **P4.D351** |
| `generateFormattingPromptHint` (`lib/chat/template-prompt-hint`) | `(delimiters, narrationDelimiters?)` | `(…, narrationDelimiters?: … \| null, options: { markdownAsterisks?: boolean } = {})` | REDDEN | `template-prompt-hint.ts` → **P4.D351** |
| `generateGreetingMessage` (`lib/chat/initial-greeting`) | `GreetingRequest` without `connectionProfileId` | `+ connectionProfileId` | REDDEN (the greeting's `llm_logs` row — `eb2d87788`) | `initial-greeting.test.ts` → **P4.D348** |
| `finalizeMessageResponse` (`message-finalizer.service`) | options object | `+ storyteller` | REDDEN | `answer-confirmation-tier3.test.ts`, `message-finalizer-tier3.test.ts` → **P4.D343** (with P4.D348 on the spine) |
| `isRecordOnlyMessage` (`context-builder.service`) | `{ systemSender?, systemKind? }` | `+ content?: string \| null` | REDDEN (a content-based rule — `d68c1af88`'s `off-scene-left-behind`) | `is-record-only-message.ts` → **P4.D347** |
| `policyProtectionOf` (`lib/memory/policy-protection`) | `Pick<Memory,'source'\|'tier'>` | `& Partial<Pick<Memory,'keystone'>>` | REDDEN (keystones protected) | `policy-protection.ts` → **P4.D341** |
| `sceneDescriptionParts` (`image-gen/appearance-resolution`) | Pick of three | `+ 'humanoid'` | REDDEN (type; the output moves with `b27e5089a`) | `inventory-image-description.test.ts` → **P4.D349** |
| `extractVisibleConversation` (`cheap-llm-tasks/chat-tasks`) | inline array type | `Array<VisibleConversationCandidate>` | type-only | `chat-tasks.ts` — none |
| `resolveWardrobeMount` and the other 29 of the 2026-10-20 list | — | unchanged this span | — | — |

### E.4 Runtime consequences no single lane owns

- **E.4.1 — The lockfile vs the cache (§F.4 / §J.1)** blocks EVERY
  regenerating lane's "safe default" `npm ci --offline` at the pin. Decide
  before the orders are written; the pin recipe in every order must carry
  the ruling.
- **E.4.2 — Five migrations + the `narrationDelimiters` seed rewrite**
  (`778b97b2b`) make `provisioning_equivalence` (1d, `QT_FRESH_SCHEMA_LIVE`)
  and `migration_index_backfill_equivalence` RED at the pin — the D23
  tripwire for moves 17–21, owned by the keystone P4.D341 (the keystone
  survey has the DDL). Every lane whose regen runs v4's full migration
  registry or dumps `sqlite_master` stacks on the keystone.
- **E.4.3 — `log_entries` in backups (`cc5f2347c`)** flips P4.D327's "no
  `log_entries` file in any archive" pins and the restore census
  (`RESTORE_TS_MESSAGES`, TS + Rust) BY DESIGN — P4.D355's, named here so
  the unifier does not read the red as a vendor-lane fault.
- **E.4.4 — `components/inventory/index.ts` deleted:** no oracle imports it;
  `apps/web` imports nothing from v4. No consequence.
- **E.4.5 — `lib/chat/context/index.ts` (the barrel) deleted:** no oracle
  specifier is `@/lib/chat/context` bare (every context case imports a leaf
  module); the leaves `system-prompt-builder`, `scene-state`,
  `message-attribution`, `message-selector` still exist.

---

## §F — Dependency facts (for §R.3)

### F.1 The manifest (`git diff 322a5488e 236c04e35 -- package.json`)

`version` `4.11.0-dev.53` → `4.12.0-dev.0`. Dependencies: `+@hpcc-js/
wasm-graphviz ^1.30.0`, `@quilltap/plugin-types ^2.8.0 → ^2.9.0`
(`b15fe9404`), `@tanstack/react-query` / `-devtools` `^5.103.2 → ^5.105.0`,
`@tanstack/react-virtual ^3.14.13 → ^3.14.23`, `−create-quilltap-theme
^2.0.20` (`58120a7fe`), `+dompurify ^3.4.17`, `katex ^0.19.0 → ^0.19.1`,
`mammoth ^1.12.3 → ^1.13.0`, `+mermaid ^12.2.0`, `next ^16.3.6 → ^16.4.3`,
`openai ^7.23.0 → ^7.45.0`, `−pdf-parse ^2.4.5`, `pdfjs-dist ^5.7.284 →
^6.4.299`, `react` / `react-dom ^19.3.0 → ^19.3.1`, `react-syntax-highlighter
^16.1.1 → ^16.1.2`, `sharp ^0.35.4 → ^0.35.5`, `+svg2roughjs ^3.2.3`, `ws
^8.21.3 → ^8.22.0`, `zod ^4.6.5 → ^4.6.7` (range only; the lock stays 4.6.7).
DevDependencies: `@openrouter/sdk ^1.3.28 → ^1.5.9`, `@playwright/test
^1.63.0 → ^1.64.1`, `@tanstack/eslint-plugin-query → ^5.105.0`,
`@testing-library/user-event → ^14.6.8`, `@types/node ^24.13.6 → ^24.19.2`,
`@types/ws → ^8.18.2`, `autoprefixer → ^10.6.2`, `dotenv → ^18.0.9`,
`eslint-config-next → ^16.4.3`, `postcss → ^8.5.32`; the `overrides` entry
`pdfjs-dist: $pdfjs-dist` and `@napi-rs/canvas ^0.1.100` removed; the
`@openrouter/sdk@1.4.25: true` key → `@openrouter/sdk@1.5.9: true`.
**Attribution:** every line above is `026222176`'s except
`@quilltap/plugin-types` (`b15fe9404`), the four diagram deps
(`6588972d5`, re-added by `a0905454b`) and the `create-quilltap-theme` drop
(`58120a7fe`).

### F.2 The lockfile delta (`package-lock.json` `packages` map)

**117 added, 4 removed, 72 version changes** (the full list:
`<scratchpad>/lock-delta.txt`). The packages the oracles load at runtime:

| Package | Baseline → pin | Commit |
|---|---|---|
| `openai` (root) | 7.34.0 → **7.45.0** | `026222176` |
| `@openrouter/sdk` (root) | 1.4.25 → **1.5.9** | `026222176` |
| `zod` | 4.6.7 → 4.6.7 | — |
| `pdfjs-dist` | 5.7.284 → **6.4.299** | `026222176` |
| `pdf-parse` | 2.4.5 → **REMOVED** | `026222176` |
| `@quilltap/plugin-types` | 2.8.0 → **2.9.0** | `b15fe9404` |
| `@quilltap/plugin-utils` | 2.7.1 → 2.7.1 | — |
| `@xterm/headless` | 6.0.0 → 6.0.0 | — |
| `better-sqlite3` (alias of `better-sqlite3-multiple-ciphers`) | 12.11.1 → 12.11.1 | — (13.0.3 is `8832cfd20`, past the pin) |
| `sharp` | 0.35.5 → 0.35.5 | range only |
| `mammoth` | 1.13.0 → 1.13.0 | range only |
| `katex` | 0.19.0 → **0.19.1** | `026222176` |
| `next` | 16.4.2 → **16.4.3** | `026222176` |
| `js-tiktoken` | 1.0.21 → 1.0.21 | — |
| `mermaid` / `@hpcc-js/wasm-graphviz` / `svg2roughjs` / `dompurify` | — → **12.2.0 / 1.30.0 / 3.2.3 / 3.4.17** (+ ~110 transitive: `@mermaid-js/parser`, `chevrotain`, `cytoscape*`, `d3*`, `@iconify/*`, `@braintree/sanitize-url`, …) | `6588972d5` / `a0905454b` |
| `@napi-rs/canvas` (root + `packages/quilltap`) | 0.1.100 → **1.0.13** | `026222176` (the CLI manifest `^0.1.100 → ^1.0.13`) |
| `create-quilltap-theme` | 2.0.20 → REMOVED | `58120a7fe` |

### F.3 Plugin manifests (`plugins/dist/*/package.json`) and `packages/*`

| Plugin | Baseline → pin | Commit |
|---|---|---|
| `qtap-plugin-curl` | 1.0.31 → **1.0.32** (`plugin-types ^2.9.0`) | `b15fe9404` |
| `qtap-plugin-mcp` | 1.1.44 → **1.1.45** (`plugin-types ^2.9.0`) | `b15fe9404` |
| `qtap-plugin-default-system-prompts` | 1.1.26 → **1.1.27** | `66f8c4d41` |
| `qtap-plugin-openai-compatible` | 1.0.49 → **1.0.50** | `66f8c4d41` |
| `qtap-plugin-nanogpt` | 1.2.10 → **1.2.11** | `4b0d1cb04` |

Every plugin still declares `openai ^7.23.0` (deepseek, grok, nanogpt,
openai-compatible, openai, z-ai), `@openrouter/sdk ^1.3.28` (openrouter),
`@anthropic-ai/sdk ^0.115.0`, `@google/genai ^1.52.0`, `plugin-utils ^2.7.1`;
**no `plugins/dist/*/package-lock.json` is tracked** (0 at the pin), so the
plugin dirs' installed versions come from the live tree's
`plugins/dist/*/node_modules` (symlink class 3) — unchanged by this span,
but check `plugins/dist/qtap-plugin-openai/node_modules/openai/package.json`
is still 7.23.0 at lane start (the guard does). Nine `plugins/dist/*/
index.js` bundles were rebuilt in the span (+2,935 / −2,311 each; `dff76ebef`
then `68340008b`'s revert, `66f8c4d41`'s two) — NO-PORT. `packages/
plugin-types` 2.8.0 → 2.9.0 (`src/plugins/tool.ts` +33: the `effect`
declaration — P4.D343's spec), `packages/plugin-utils` unmoved,
`packages/theme-storybook` 1.0.77 → 1.0.90, `packages/quilltap`
`4.11.0-dev.53` → **`4.11.0-dev.112`** (NOT `4.12.0-dev.0` — the release
trio stamps only the root).

### F.4 Does the npm cache hold the pin? NO — 44 tarballs missing

Method (the brief's): `npm pack <pkg>@<ver> --offline --dry-run
--ignore-scripts` from a scratch dir
(`/private/tmp/claude-503/-Volumes-External-1-source-overflow-quilltap-v5/
bcb37637-ec49-4047-bd92-0f8ab89e00ab/scratchpad/npm-pack-probe/`), plus
`npm cache ls` (4,744 entries) grepped for every `resolved` URL the pin's
lock adds or moves (188 top-level tarballs):

| Spec | `npm pack --offline` |
|---|---|
| `openai@7.45.0` | **ENOTCACHED** (cache holds 7.34.0 and **7.45.1**) |
| `@openrouter/sdk@1.5.9` | **ENOTCACHED** (holds 1.4.25, 1.5.16, 1.5.21) |
| `pdfjs-dist@6.4.299` | **ENOTCACHED** (holds 5.7.284 only) |
| `next@16.4.3` | **ENOTCACHED** (holds 16.4.2) |
| `katex@0.19.1` | **ENOTCACHED** (holds 0.19.0) |
| `@tanstack/react-query@5.105.0` (+ `query-core`, `-devtools`, `react-virtual@3.14.23`, `virtual-core@3.19.8`) | **ENOTCACHED** |
| `dotenv@18.0.9` | **ENOTCACHED** |
| `@napi-rs/canvas@1.0.13` (+ nine platform binaries, `@napi-rs/wasm-runtime@1.2.6`) | **ENOTCACHED** (the packument; the darwin-arm64 tarball alone is cached) |
| `@next/env@16.4.3` + nine `@next/swc-*@16.4.3` | missing |
| `autoprefixer@10.6.2`, `baseline-browser-mapping@2.11.33`, `caniuse-lite@1.0.30001819`, `electron-to-chromium@1.5.464`, `emojibase@18.0.0`, `hast-util-to-text@4.0.3`, `mdast-util-find-and-replace@3.0.4`, `node-releases@2.0.60`, `seroval@1.6.12`, `seroval-plugins@1.6.12`, `tabbable@6.5.1` | missing |
| `@quilltap/plugin-types@2.9.0`, `mermaid@12.2.0`, `@hpcc-js/wasm-graphviz@1.30.0`, `svg2roughjs@3.2.3`, `dompurify@3.4.17`, `zod@4.6.7`, `openai@7.34.0`, `@xterm/headless@6.0.0`, `@quilltap/plugin-utils@2.7.1` | OK |

Why: the live checkout's `node_modules/.package-lock.json` is stamped
`4.12.0-dev.5` with `openai` **7.45.1** and `better-sqlite3` **13.0.3** — the
human installed past the pin (`8832cfd20` and later) and never at `7789ebed6`
/ `236c04e35`, so the cache has neither side's exact set. Consequences:

- The 2026-10-07 safe default (`rm "$PIN/node_modules" && npm ci --offline`)
  FAILS at the pin.
- Symlink class (1) (`$PIN/node_modules → live`) gives v4's pin code
  `openai` 7.45.1 (not 7.45.0), `@openrouter/sdk` 1.5.21 / 1.5.16 (not
  1.5.9), `better-sqlite3-multiple-ciphers` **13.0.3** (the `8832cfd20`
  driver move the pin's `sqlcipher-key.ts` was not written for — the
  ledger's `8832cfd20` row says v12/v13 files interoperate, but the
  `PRAGMA` surface may differ), `next` 16.4.3+, `pdfjs-dist` 6.4.299 —
  every oracle that runs still runs, but `provider_sdk_version_guard`
  would pin against 7.45.1 and the Stainless `x-stainless-package-version`
  stamps in any RE-RECORDED corpus would carry 7.45.1.
- **Ruling (§J.1):** run ONE online `npm ci --no-audit --no-fund` in the
  round's shared pin worktree before the lanes cut (it populates the cache;
  every lane's own pin then `npm ci --offline` cleanly) — OR rule the symlink
  class acceptable for this round with the guard constants pinned at what
  the live tree holds (7.45.1 / 1.5.21 — moving targets). Recommend the
  former; it is the only path that makes `INSTALLED_ROOT_OPENAI_SDK =
  "7.45.0"` true.

### F.5 The guards and what they read

- `crates/quilltap-harness/tests/provider_sdk_version_guard.rs:128`
  `const INSTALLED_ROOT_OPENAI_SDK: &str = "7.34.0";` → **`"7.45.0"`**;
  `:132` `INSTALLED_ROOT_OPENROUTER_SDK = "1.4.25"` → **`"1.5.9"`** (both
  read the ROOT `node_modules/<pkg>/package.json` of the checkout at
  `QT_V4_CHECKOUT` / `QT_V4_ROOT`); `RECORDED_OPENAI_SDK = "7.23.0"` (`:116`),
  `RECORDED_ANTHROPIC_SDK`, `RECORDED_GOOGLE_GENAI_SDK`,
  `RECORDED_OPENROUTER_SDK = "1.3.28"` (`:122`), `RECORDED_PLUGIN_UTILS =
  "2.7.1"` (`:144`), `RECORDED_NODE = "v24.13.1"` (`:138`) — UNMOVED (the
  plugin dirs' versions did not move; the recorders run FROM plugin dirs).
  Per the P4.D260 R-F shape, a root move alone is "a regen event to MEASURE
  (does a recorder now resolve the root?), not a re-record" — the lane that
  moves the constants must state that measurement.
- `zod_version_guard.rs:155` `RECORDED_ZOD_VERSION = "4.6.7"` — reads
  `<checkout>/node_modules/zod/package.json`; UNMOVED (the lock stays
  4.6.7).
- `builtin_prompt_templates_guard.rs:76` reads
  `<checkout>/plugins/dist/qtap-plugin-default-system-prompts/prompts/*.md`
  — `git diff --stat 322a5488e 236c04e35 -- plugins/dist/qtap-plugin-
  default-system-prompts/prompts` is EMPTY; UNMOVED (the manifest bump
  1.1.27 is the bundling commit's).
- **Owner:** by precedent the vendor lane carries the two `INSTALLED_ROOT_*`
  constants when no content lane re-records (P4.D260 / P4.D311 did it in a
  dependency-move lane); this round NO lane re-records a provider corpus
  (P4.D356's NanoGPT bug-269 fix is a request-shape change on the image
  dialect — check whether its oracle re-records through the nanogpt plugin
  dir, which still declares `openai ^7.23.0`). Give the two constants to
  **P4.D360** with the "root move measured, no recorder resolves the root"
  sentence, conditional on §J.1.

### F.6 `V4_APP_VERSION`

ONE site: `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:126`
`const V4_APP_VERSION: &str = "4.11.0-dev.53"; // v4 \`322a5488e\` …` →
`"4.12.0-dev.0"` with the comment re-pointed at `236c04e35`; the family
regenerated at the pin (`ai-import-tier3.test.ts` emits v4's `appVersion`
from the root `package.json`). No other `4.11.0-dev` / `4.12.0-dev` literal
under `crates/`, `apps/web/src`, `harness/oracle` (`ggrep -rln` finds only
`custom_tool_schema.rs:7`'s doc comment "(`4.11.0-dev.53`)" — update the
comment when P4.D345 moves that embed). Owner: **P4.D360** (no content lane
touches the AI-import family this round — confirm against P4.D354's
`file-content-extractor` re-aim, which shares `common/mod.rs`).

---

## §G — Public schemas and the specimen JSONs

| File | Baseline bytes / md5 | Pin bytes / md5 | Commits (newest first) | v5 copy today | Constants that move |
|---|---|---|---|---|---|
| `public/schemas/qtap-export.schema.json` | 109,559 `10b72fd1160722b77a49fe6f7abb5b79` | **115,274 `8665e3cbffc8db5d808fcebe4bfe302b`** (+81 / −5) | `6588972d5` (`canDrawDiagrams`, `diagramStyle`), `b27e5089a` (`humanoid`), `eb90d9ed8` (`pronounGuidance` enum `he/she/they/it`), `778b97b2b` (`unmarkedStyle`, `markdownAsterisks`, `DelimiterAddOns` — `tokenPattern`, `matchCurlyQuotes`, `closeOptional`, `addOns`), `839575d89` (`pascalMeta.intervention`: `decision deal/as-rolled`, `dealt`, `changed[]`, `reasks`, `resolvedAt`, `staleAnswer`, `callerParticipantId`), `b15fe9404` (`turnMode`, `storytellerInviteFilter`, `storytellerBid`: `roundId`, `plan`, `tier simple/lookup/act/destroy`, `toolsNamed[]`), `8cb584eff` (`keystone`) | `crates/quilltap-core/src/generators/qtap-export.schema.json` 109,559 (`include_str!` at `qtap_schema.rs:59`) | `qtap_schema.rs:204` `assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(), 109_559)` → `115_274`; `qtap_schema_embed_guard.rs:93` `VENDORED_BYTES = 109_559` → `115_274` (+ a P4.D341 history line); `qtap_schema_validate_equivalence.rs` rows for the new keys (red-first) |
| `public/schemas/qtap-custom-tool.schema.json` | 63,595 `670ade1d0accb312d58f6c6c034285ae` | **66,469 `454fa04e7a11caa98becd877212f20e2`** (+62) | `839575d89` ALONE — the `intervention` object (`enabled` required; `fields.{roll,value,llmPrompt,…}` booleans; `additionalProperties: false`) | `apps/web/public/schemas/qtap-custom-tool.schema.json` 63,595 AND `crates/quilltap-core/src/pascal/specimens/qtap-custom-tool.schema.json` (`custom_tool_schema.rs:23` `QTAP_CUSTOM_TOOL_SCHEMA_JSON`) | `public_schemas_vendor_guard.rs:57` `bytes: 63_595` → `66_469`; `custom_tool_schema_embed_guard.rs:30` `SCHEMA_BYTES = 63_595` → `66_469` (its three-way leg compares the engine embed, the SPA asset and `docs/v4/developer/` — the two specimens below) |
| `public/schemas/qtap-progression.schema.json` | 5,963 | 5,963 (unchanged) | — | — | `public_schemas_vendor_guard.rs:63` `bytes: 5_963` unmoved |
| `docs/developer/CUSTOM_TOOL_SPEC.json` | 12,660 `9e7954b83f3f…` | **unchanged** | — | `crates/quilltap-core/src/pascal/specimens/CUSTOM_TOOL_SPEC.json` + `docs/v4/developer/CUSTOM_TOOL_SPEC.json` | `custom_tool_schema_embed_guard.rs:31` `SPEC_BYTES = 12_660` unmoved |
| `docs/developer/CUSTOM_TOOL_SPEC_DICE.json` | 1,951 | **unchanged** | — | both copies | `:32` `DICE_BYTES = 1_951` unmoved |

**Ownership (the P4.D323 / P4.D339 precedent — content lanes own the
schema whose acceptance they port; the vendor lane re-runs the guards on
the union as a CHECK):**

- `qtap-export.schema.json` + `qtap_schema.rs:204` + `VENDORED_BYTES` +
  the validate rows → **P4.D341** (the keystone owns the `.qtap` carriers
  for `keystone` and the Storyteller / template / intervention columns'
  readers; the seven writers' lanes — P4.D343, P4.D345, P4.D349, P4.D351 —
  cite the vendored bytes in their `HANDOFF:`; the FINAL writer `6588972d5`
  is P4.D349's, but P4.D349 is wave 0 and P4.D341 is the stack root every
  carrier lane branches from, so the keystone is the cleaner home).
- `qtap-custom-tool.schema.json` (BOTH v5 copies — the SPA asset and the
  engine specimen) + `public_schemas_vendor_guard.rs:57` +
  `custom_tool_schema_embed_guard.rs:30` → **P4.D345** (`839575d89` is its
  sole writer; P4.D346 serves the SPA asset but must not touch the file —
  a `HANDOFF:` sentence).
- The vendor lane copies NEITHER schema; it re-runs `public_schemas_vendor_
  guard`, `qtap_schema_embed_guard`, `custom_tool_schema_embed_guard` on the
  union with `QT_V4_CHECKOUT` = the pin and records the expected constants.
  On the vendor branch alone the two v4-equality tests are RED by design
  (the P4.D339 §R.13 (o) shape).

---

## §H — The CLI (`packages/quilltap/**`) and Tier R

`git diff --stat 322a5488e 236c04e35 -- packages/quilltap/`: **14 files,
+266 / −60.**

| File | +/− | Commit | Hunk (what it DOES) | v5 counterpart / Tier R consequence | Owner |
|---|---|---|---|---|---|
| `bin/quilltap.js` | 1 / 1 | `40cc6ffda` | `console.log(row.count)` → `console.log(String(row.count))` — a string is never coloured under `FORCE_COLOR` | `crates/quilltap-cli/src/db_cmd.rs:339` (`SELECT count(*) as count …`) prints the number as text already; Tier R rows `db count` / `--count widgets --json` / `--count nope` / `--llm-logs --count llm_logs` / `--count chat_messages` (`cli_differential.rs:2480-2711`) run without `FORCE_COLOR` → expected to STAY equal; the lane adds a `FORCE_COLOR=1` pre-hook row if it wants the fix PROVEN | **P4.D356** (verify-only) |
| `lib/platform-paths.js` (NEW, 45) + `lib/db-helpers.js` (7 / 3) + `lib/lock-helpers.js` (5 / 4) + `lib/theme-commands.js` (6 / 2) | — | `faeae2b71` | `isDockerContainer()` = `DOCKER_CONTAINER === 'true'` or `/.dockerenv` exists (deliberately WITHOUT the server's `/app` heuristic); `getDefaultBaseDir()` = `/app/quilltap` in a container, else the platform dir (macOS `~/Library/Application Support/Quilltap`, Windows `%APPDATA%/Quilltap`, else `~/.quilltap`); `db-helpers` / `lock-helpers` / `theme-commands` route through it (`isDocker` → `'docker'` runtime label) | `crates/quilltap-cli/src/resolve.rs:36-70` `resolve_data_dir` has NO container branch (override → `QUILLTAP_DATA_DIR` → platform dir); `crates/quilltap-host/src/paths.rs:22-29` already has the `/app/quilltap` docker default for the HOST. Tier R runs on macOS with no `/.dockerenv`, so no row moves; a `DOCKER_CONTAINER=true` pre-hook row proves the branch | **P4.D354** |
| `lib/memories-commands.js` (+109 / −…) + `lib/memories-server.js` (59) + `lib/completion/{bash,fish,zsh}.template` (4 / 5 / 5) | — | `8cb584eff` | NEW `keystone` verb (server-backed, `--off` to unmark, `--write` to apply); `ls --keystone` filter (`m.keystone = 1` when the column exists, else `0`); the SELECT template substitutes `0 AS keystone` on pre-migration instances; completion: bash `mem_verbs` gains `keystone`, `--keystone --off` flags; fish three `complete -c` lines; zsh three lines | v5's `memories` verb family is a **named deferral (R-J)** — `main.rs:62` lists it in `SUBCOMMANDS` and the completion templates advertise v4's lines "because they are v4's bytes" (`completion_behavior.rs:564-570`). **The three templates under `crates/quilltap-cli/src/help/completion/` must move to the pin's bytes** (`bash.template:324` still `… validate maintain`) or Tier R's `completion bash/zsh/fish` rows (`cli_differential.rs:4818-4820`) go RED. The verb itself stays NO-PORT by R-AL | **P4.D341** (the row's R-AL note) — or P4.D356 as a smalls rider; ONE owner for the three templates |
| `lib/recall-replay-command.js` | 4 / 1 | `a5efdbb97` | `--help` text: the `--tuning` keys list gains `innerCircleShare, innerCircleAllowance ([3,2,1] = digests per member by tier)` | `crates/quilltap-cli/src/help/recall_replay_help.txt:37` still ends `anchorOrder, backgroundReserve.` → the Tier R `recall-replay help` row (`:5039`) goes RED until the text moves | **P4.D342** |
| `lib/__tests__/platform-paths.test.js` (NEW 62) | — | `faeae2b71` | v4's own test | NO-PORT (C.2) | — |
| `README.md` | 5 / 4 | `539bb4012`, `8cb584eff`, `a5efdbb97` | the server-backed verbs list | `docs/v4/packages-quilltap-README.md` (B.2) | **P4.D360** |
| `package.json` | 2 / 2 | `026222176` (`@napi-rs/canvas ^1.0.13`), every stamped commit | `4.11.0-dev.112` | a stamp; if any Tier R row prints the CLI version it compares against `4.11.0-dev.112` | — |

Tier R today: `crates/quilltap-cli/tests/cli_differential.rs` has 293
`ctx.case*` registrations (65 `case` + 228 `case_with`; the round records
count 422 ROWS because several registrations expand); `docs` rows at
`:2460` / `:4045` / `:4201` are unaffected (no `docs-commands` hunk in the
span — the brief's "`docs-commands` etc." has no diff).

---

## §I — The LIVE-DATA statement for the round (draft for every order's preamble)

Drafted from the ledger §1 block (`drift-ledger.md:330-407`) and the
measurements above; the keystone / vault-key / files surveys may sharpen the
column shapes.

- **A v5 at `236c04e35` reads and writes:** `memories.keystone` (17th move,
  `NOT NULL DEFAULT 0` + the partial index `idx_memories_keystone` as a boot
  ensure), `chats.turnMode` / `storytellerInviteFilter`,
  `chat_messages.storytellerBid`, the NEW table `storyteller_rounds` (with
  its FK + index as ensures), `llm_logs.storytellerRoundId` (the second
  llm-logs column ensure), `chat_settings.customToolIntervention` (default
  ON), `roleplay_templates.unmarkedStyle` / `markdownAsterisks` — and runs
  `778b97b2b`'s every-boot seed that re-encodes the built-in templates'
  `narrationDelimiters` as JSON and inserts the Novel template with it
  NULL. **The first v5 boot of a pre-`8cb584eff` Friday copy ADDS the seven
  columns + one table (column-gated, v4's shapes, stamping per the
  ping-pong rule — the keystone survey decides each) and REWRITES the
  built-in templates' delimiter cells; a copy v4 migrated sees NO-OP
  ensures and a no-op seed (measure: the seed must be idempotent against
  v4's own output).**
- **The THREE vault-key ERASURE hazards RETIRE** once P4.D349 lands: v5's
  `render_physical_prompts_json` carries `humanoid` (so a v4 `"humanoid":
  false` survives a v5 physical write and the P4.82 backfill respects it),
  `PropertiesJson` carries `canDrawDiagrams` / `diagramStyle`, and
  `build_wardrobe_item_file` carries `pronounGuidance`. Until that lane is
  on the union build, **no v5 write to a character v4 edited past
  `6588972d5` / `b27e5089a` / `eb90d9ed8`** (reads stay safe).
- **The Log is carried** (`cc5f2347c`, P4.D355): a v5 backup writes
  `data/log-entries.json`, a `replace` restore / delete-all clears
  `log_entries`, a `new-account` restore remaps `id` / `chatId` /
  `characterId`; `.qtap` still omits it. A pre-round v5 backup has no Log
  file — restoring it on the union leaves the Log empty (expected).
- **Keystones protected:** maintenance, demotion, consolidation, fold and
  chat-wipe skip `keystone = 1` rows; recall lifts them into the circle;
  a v5 backup / `.qtap` carries the column. A pre-round v5 build must NOT
  run maintenance on a copy v4 keystoned (the standing rule).
- **Terminals:** kill sends SIGHUP then SIGKILL after 3 s; a deleted chat
  terminates its shells; the daily sweep unlinks orphaned transcripts
  older than 1 h (`85c002822`, P4.D355). **Settings:** the chat-settings PUT
  refuses unknown keys with a 400 (`3b8b6f42b`) — a v5 SPA caller that
  echoed a GET row back now fails (P4.D356's census). **Files:** ingest
  fills `doc_mount_blobs.extractedText*` for native-text uploads and a boot
  backfill fills old rows (`40fab1c54`, P4.D354 — a DATA WRITE on first boot
  of any copy with uploads: measure the row count).
- **NOT this round (`8832cfd20`, `bf7a457f6`):** v4 HEAD links SQLite3MC
  2.4.0 / SQLite 3.53.4; v5 stays 2.3.5 / 3.53.2 — the first v5 open of a
  post-`8832cfd20` Friday copy is the measurement (ledger §1); the iCloud
  placeholder scan divergence stands as recorded.
- **Standing:** no v5 build against live Friday; dogfood a COPY; do not
  round-trip v4-written characters through v5 writes while comparing with
  v4.

---

## §1 — Per commit, the shipped hunks in THIS lane's trees (help / docs / themes / lockfile / CLI docs)

Only the vendor-lane slice; code hunks are the content surveys'. `docs/
CHANGELOG.md` rides every non-merge commit and is omitted per row.

| sha | `help/` | `docs/` (paths under `docs/developer/` unless noted) | themes / deps / CLI docs |
|---|---|---|---|
| `6fd2d5ffb` | `memory-recall-relevance.md` | (`features/inner-circle-recall.md` born here, moved by `8cb584eff`) | — |
| `a5efdbb97` | `groups.md`, `memory-recall-relevance.md` | — | `packages/quilltap/README.md`, `recall-replay-command.js` help text |
| `8cb584eff` | `cli-memories.md`, `memory-maintenance.md`, `memory-recall-relevance.md` | `CLI.md`, `DDL.md`, `features/ROADMAP.md`, `features/complete/inner-circle-recall.md` | `packages/quilltap/README.md`, the three completion templates, `memories-commands.js` |
| `036b32a78` | `chat-multi-character.md` | `bugs.md`, `bugs/fixed/bug-258-*.md` | — |
| `b15fe9404` | `autonomous-rooms.md`, `chat-multi-character.md`, `chat-participants.md`, `custom-tools.md`, `pascals-workbench.md`, `storyteller-mode.md` (A), `tools-settings.md` | `FEATURES.md`, `API.md`, `BACKGROUND_JOBS_CHILD.md`, `DDL.md`, `features/complete/storyteller-mode.md` (A) | `@quilltap/plugin-types` 2.9.0; curl 1.0.32, mcp 1.1.45 |
| `332c650b9` | — | `features/pascal-operator-intervention.md` (born; moved by `839575d89`) | — |
| `eb2d87788` | `system-llm-logs.md` | — | — |
| `839575d89` | `chat-settings.md`, `custom-tools.md`, `pascals-workbench.md` | `API.md`, `DDL.md`, `features/ROADMAP.md`, `features/complete/pascal-operator-intervention.md` (moved + as-built) | `qtap-custom-tool.schema.json` (+62), `qtap-export.schema.json` |
| `946a50a04` | `storyteller-mode.md` | `API.md`, `features/complete/storyteller-mode.md` | — |
| `778b97b2b` | `roleplay-templates-settings.md`, `roleplay-templates.md` | `DDL.md` | `qtap-export.schema.json` |
| `eb90d9ed8` | `wardrobe.md` | — | `qtap-export.schema.json` |
| `8ae8a127c` | `custom-tools.md` | — | — |
| `99463fc56` | `brahma-console.md` | — | — |
| `265b711dd` | `inventory.md`, `wardrobe.md` | — | — |
| `4a25f7eab` / `505a524fa` / `3c9ae1fc3` / `b892227eb` | — | `docs/releases/4.11.0.md` | — |
| `fa126bde8` | `chat-multi-character.md` | `bugs.md`, `bug-259-*.md` | — |
| `40fab1c54` | `document-editing-tools.md`, `file-uploads.md` | `bugs.md`, `bug-260-*.md`, `bug-261-*.md` | — |
| `074e1e01a` | `character-system-transparency.md`, `inventory.md` | `FEATURES.md` | — |
| `b02d902f3` | `custom-tools.md` | `features/complete/pascal-operator-intervention.md` | — |
| `f7a1789df` / `d68c1af88` / `9a88f862f` | `salon-host-introductions.md` | `PROMPT_ARCHITECTURE.md` (`d68c1af88`, `9a88f862f`), `API.md` (`9a88f862f`) | — |
| `b27e5089a` | `character-editing.md` | `DDL.md` | `qtap-export.schema.json` |
| `1dd0a0c01` | `chat-settings-composer.md` | `DDL.md` | — |
| `0276d10d9` | `inventory.md` | `API.md`, `features/complete/inventory.md` | — |
| `a8214b253` | `chat-turn-manager.md` | `SYSTEM_FLOWCHARTS.md`, `bugs.md`, `bug-262-*.md` | — |
| `cb94e6e99` | — | — | — |
| `bdd09b75b` | `chat-settings-ai-services.md`, `connection-profiles.md` | `PROMPT_ARCHITECTURE.md`, `SYSTEM_FLOWCHARTS.md`, `bugs.md`, `bug-263/264/265-*.md` | — |
| `6588972d5` | `character-editing.md`, `diagrams.md` (A), `quick-hide.md` | `FEATURES.md`, `DDL.md`, `features/ROADMAP.md`, `features/complete/diagrams.md` (A) | the four client deps; `qtap-export.schema.json` |
| `5e18ede6a` / `a0905454b` | — | — | the deps dropped / restored |
| `9faed0c7e` | `diagrams.md` | `features/complete/diagrams.md` | — |
| `2041bb875` / `8bf856050` / `7d7e38d2f` | — | — | — |
| `8becfc6ce` | — | `DEAD-CODE-REPORT.md` | — |
| `66f8c4d41` | — | — | default-system-prompts 1.1.27, openai-compatible 1.0.50 |
| `40cc6ffda` | — | — | `bin/quilltap.js` |
| `539bb4012` | — | — | `packages/quilltap/README.md` |
| `cc5f2347c` | `system-backup-restore.md`, `the-log.md` | — | — |
| `026222176` | — | `bugs.md`, `bug-266-*.md` | the lockfile (F.1 / F.2), `public/pdf.worker.mjs`, eight bundles, `packages/quilltap/package.json` canvas |
| `744596158` | — | — | — |
| `c50cdddc6` | `ariel.md`, `system-backup-restore.md` | — | — |
| `85c002822` | `ariel.md` | `bugs.md`, `bug-267-*.md`, `bug-268-*.md` | — |
| `4b0d1cb04` | `image-generation-profiles.md` | `bugs.md`, `bug-269-*.md` | nanogpt 1.2.11 |
| `2b2b190b6` | `chat-settings-composer.md`, `roleplay-templates.md` | — | — |
| `cce44e098` | `pascals-workbench.md` | — | 14 theme files |
| `dd200c695` | — | `bugs.md` (the FREEZE) | — |
| `86cc25482` | — | — | great-estate + rains (4 files) |
| `b09ff5c86` | — | B.2's 12 feature-spec operations | — |
| `d64b25e96` | `salon-host-introductions.md` | — | — |
| `3a98a6cca` | — | `DEVELOPMENT.md` | — |
| `58120a7fe` | `appearance-settings.md` | — | all 18 theme files; `create-quilltap-theme` dropped |
| `dff76ebef` | `appearance-settings.md` | — | (bundles) |
| `eed72d60c` / `68340008b` | — | — | (bundles) |
| `10e09b7c3` | `appearance-settings.md`, `ariel.md` | — | earl-grey, great-estate, rains (6 files) |
| `3c818c856` | — | — | — |
| `a4945d8d9` | — | `API.md` (+182), `DEVELOPMENT.md`, `docs/releases/4.11.0.md` | — |
| `82a2f6536` | — | — | — |
| `faeae2b71` | `cli-sync.md`, `data-directory.md`, `mount-points.md` | `docker-test-plan.md` (A) | `platform-paths.js` + three helpers |
| `3b8b6f42b` / `7789ebed6` | — | — | — |
| `1ca45c171` / `9e01ebd25` / `236c04e35` | — | `docs/releases/4.11.0.md` (`draft: false`), `CHANGELOG.md` headings | the stamps |

---

## §2 — The v5 surface map for this lane

| v4 surface | v5 counterpart | Current shape (measured) |
|---|---|---|
| `help/**` (137 at the pin) | `help/**` (135, embedded by `crates/quilltap-host/build.rs` → `help_content.rs` `EMBEDDED_HELP`) | byte-equal to `322a5488e`; guards at 135 (A.3) |
| `docs/**` (517) + `packages/quilltap/README.md` | `docs/v4/**` (502) | byte-equal to `322a5488e`; reference-only — `spine.rs:3752` `docs_dir: None` (B.3) |
| `themes/bundled/*/{theme.json,tokens.json}` | `apps/web/public/themes/*/{theme.json,tokens.json}` | byte copies at the baseline (D.2) |
| `themes/bundled/*/styles.css` | `apps/web/public/themes/*/styles.css` | v4 bytes + the URL rewrite (+ madmans-box's local block) — P4.D357's (D.2 / D.3) |
| `package.json` `version` | `ai_import_tier3_equivalence.rs:126` | `"4.11.0-dev.53"` (F.6) |
| root `node_modules/openai`, `@openrouter/sdk` | `provider_sdk_version_guard.rs:128` / `:132` | `7.34.0` / `1.4.25` (F.5) |
| `public/schemas/*.json`, `CUSTOM_TOOL_SPEC*.json` | NOT this lane (§G) | guards re-run as CHECK |
| the NO-PORT rows | `status-log.md` lane record "### NO-PORT ratifications (for §S.6)" | the P4.D339 block at `status-log.md:192265` is the template |

## §3 — Schema / D23

None for this lane. The five migrations + the seed rewrite are P4.D341's
(the keystone survey); this lane vendors `docs/v4/developer/DDL.md`'s prose
for them (+56 / −6) and nothing else.

## §4 — Log lines

None in this lane's trees. (The CLI `--count` fix emits no log; the Docker
base-dir change emits none.)

## §5 — Harness / oracle impact

- Regenerated from the pin by THIS lane: `help_tree_equivalence`
  (`help-tree-sync.test.ts`, jest), `help_section_size_equivalence`
  (`help-section-size.ts`, tsx, needs `js-tiktoken`), `ai_import_tier3_
  equivalence` (`ai-import-tier3.test.ts` — unless P4.D354 claims it with the
  `file-content-extractor` re-aim; ONE owner).
- Literal moves by this lane: `help_tree_embed_guard.rs:114`,
  `host_help_docs_boot.rs:131` (+ its message and header comments),
  `provider_sdk_version_guard.rs:128` / `:132` (conditional on §J.1),
  `ai_import_tier3_equivalence.rs:126`.
- Guards this lane re-runs as a CHECK (no edit): `public_schemas_vendor_
  guard`, `qtap_schema_embed_guard`, `custom_tool_schema_embed_guard`,
  `zod_version_guard`, `builtin_prompt_templates_guard`,
  `help_doc_ensure_equivalence`, `help_web_routes`.
- Round-wide: §E.1's eight dead sites (seven leaf cases + one
  `require.resolve`), §E.3's one GARBLE + seven REDDENs, §E.4's three
  structural reds (D23 tripwire, the Log carrier flip, the cache gap).
- Committed fixtures: none consumed or delivered by this lane; no rebuild.

## §6 — SPA

No component. Files: the 12 theme byte copies (D.3). The `theme-preview-
modal.ts:244` token read makes the `tokens.json` copies user-visible (the
appearance settings preview); `aa-foundation` / theme e2e specs should be
run by the lane as a control (the P4.D339 record notes they were NOT
exercised last time).

## §7 — The lane split by FILE OWNERSHIP (P4.D360)

**P4.D360 creates / edits EXACTLY:**
1. `help/**` — 135 overwritten (35 actually change) + 2 created, every byte
   from `git show 236c04e35:help/<p>`.
2. `crates/quilltap-harness/tests/help_tree_embed_guard.rs` (`:108-114`
   history + constant), `crates/quilltap-host/tests/host_help_docs_boot.rs`
   (`:7`, `:14`, `:17`, `:127-131`), `crates/quilltap-harness/tests/
   help_tree_equivalence.rs` (history comment only — the oracle NDJSON is
   regenerated, not committed), `help_section_size_equivalence.rs` (same).
3. `docs/v4/**` — 43 operations (B.2) incl. `git rm` ×3, `git mv` ×2,
   `docs/v4/packages-quilltap-README.md`.
4. `apps/web/public/themes/*/theme.json` ×6 + `tokens.json` ×6 (D.3) +
   `apps/web/package.json` / `package-lock.json` SPA bump (the only shared
   file — the SPA version bump is sequenced by the unifier as every round).
5. `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:126`.
6. `crates/quilltap-harness/tests/provider_sdk_version_guard.rs:128` /
   `:132` (conditional on §J.1 — if the symlink class is ruled, the lane
   records the live numbers instead and the planner accepts a moving pin).
7. `crates/quilltap-harness/Cargo.toml` / `crates/quilltap-host/Cargo.toml`
   version bumps if the round's rule bumps a crate for a test-only change
   (P4.D339 bumped none).
8. The lane record in `status-log.md` (append-only; the unifier's shared
   file) with the NO-PORT ratification block (§C).

**Files this lane must NOT touch (the owners):** `apps/web/public/themes/
*/styles.css` (P4.D357); `apps/web/public/schemas/qtap-custom-tool.schema.
json` + `crates/quilltap-core/src/pascal/specimens/*` +
`custom_tool_schema_embed_guard.rs` + `public_schemas_vendor_guard.rs`
(P4.D345); `crates/quilltap-core/src/generators/qtap-export.schema.json` +
`qtap_schema.rs` + `qtap_schema_embed_guard.rs` (P4.D341);
`crates/quilltap-cli/src/help/completion/*.template` (P4.D341 by R-AL, or
P4.D356 — ONE owner), `crates/quilltap-cli/src/help/recall_replay_help.txt`
(P4.D342), `resolve.rs` (P4.D354), `db_cmd.rs` (P4.D356); every
`harness/oracle/**` file (the re-aims are the content lanes' — E.1);
`apps/web/src/**`; `crates/*/src/**`.

**Collision check across the 20 lanes for the files above:** no other lane
in the table is scoped to `help/`, `docs/v4/`, the two help guards, the
theme manifests / tokens, `ai_import_tier3_equivalence.rs` or the two
`INSTALLED_ROOT_*` constants. The only contact points are (a) the SPA
`package.json` bump (every SPA lane bumps — the unifier's standing
sequence), (b) `status-log.md` (append-only), (c) the `help/` `HANDOFF:`
sentences the 14 content lanes write for the unifier's §S.5 byte-compare
(the A.1 table is the pre-measured map).

**Stacking:** none. P4.D360 cuts from `main` (wave 0) and compiles against
`main` alone; it regenerates no family that imports a deleted name.
Pick it LATE in the unification order (the P4.D339 precedent: fourteenth)
so its help / docs guards judge a union carrying every server feature and
its schema-guard CHECK sees the final constants.

## §8 — Tiering (P4.D360)

- **Tier 1 (must land):** `help/` 137 byte-exact + the two literals + the
  two regenerated help families green from the pin; `docs/v4/` 518 with the
  byte census (`docs/v4/<p>` = `git show 236c04e35:docs/<p>` for all 517 +
  the README) and the three deletes / two moves; the 12 theme byte copies;
  `V4_APP_VERSION` + the AI-import regen; the NO-PORT record (C.1 + C.2 with
  the re-run censuses); the import-dead census's conclusion recorded (E.0).
- **Tier 2 (should land):** the two `INSTALLED_ROOT_*` constants with the
  "root move measured" sentence (needs §J.1); the schema-guard CHECK on the
  union with the expected constants recorded; the `aa-foundation` + theme
  e2e control run; the `HANDOFF:` byte-compare table for every A.1 row
  (the P4.D339 "44 rows" shape).
- **Tier 3 (loud refusals, never silent):** none new — the lane has no
  runtime surface. The one standing refusal it touches is the `memories`
  verb family (R-J), which stays a named deferral; the completion templates
  move with their owner, not here.

## §9 — Fixtures

None consumed, none delivered. Oracles regenerated by this lane:
`/tmp/oracle-help-tree.ndjson`, `/tmp/oracle-help-section-size.ndjson`, the
AI-import tier-3 NDJSON (all `/tmp/p4d360/…`, `rm -f` first). No committed
`.db` is touched.

## §10 — Rulings owed to the human (§J)

1. **The npm cache gap (F.4):** authorise ONE online `npm ci --no-audit
   --no-fund` in the round's pin worktree (then every lane's `npm ci
   --offline` works), or rule the live-`node_modules` symlink acceptable for
   this round with its 7.45.1 / 1.5.21 / driver-13.0.3 skew. Recommend the
   online install; record the exact `node_modules/.package-lock.json` stamp
   afterwards.
2. **The theme split (D.3):** vendor lane = 12 byte copies, P4.D357 = six
   `styles.css` — or ONE owner (P4.D357) for all 18. Either is clean; the
   WCAG survey should be told which.
3. **Schema ownership (§G):** `qtap-export.schema.json` → P4.D341,
   `qtap-custom-tool.schema.json` → P4.D345 (sole writer), the vendor lane
   CHECKs. Confirm.
4. **`docs_dir` stays `None` (B.3):** the `self_inventory` release-notes /
   changelog reads remain "not found" in production; wiring a bundled
   `docs/v4/` is a standing deferral — confirm it is NOT this round's
   (`a4945d8d9`'s About credits are P4.D359's judgment call regardless).
5. **The `keystone` CLI verb (R-AL) vs the completion templates:** the verb
   stays NO-PORT; the three templates move to the pin's bytes (Tier R).
   Name the owner (P4.D341 per the row, or P4.D356).
6. **`cb94e6e99`'s ratification with P4.D348's rider (C.1):** retire
   `orchestrator_tier3_equivalence.rs:1325`'s frame drop as vacuous; keep
   `determine_enabled_tool_options`.
7. **The 13 halves (C.2)** — each `NO-PORT-RATIFIED(P4.D360)` for the half
   named, the rest to the lane named; and `c50cdddc6`'s "v5's backup also
   leaves `terminal_sessions` out" confirmation at unification.

## §11 — Dogfood rows this lane queues

- The Guide lists 137 pages with `Diagrams` and `Storyteller Mode` in
  alphabetical position; `/aurora/<id>/edit`'s context help still resolves
  `character-editing.md` (first match) and `/salon/<id>`'s `agent-mode.md`
  — no behaviour change from the two new `url:`s.
- `help_docs` on the Friday copy's first boot after the re-vendor: 135 →
  137 rows, 35 `updatedAt` moves, the section chunks re-embedded (💸 only if
  an embedding profile is configured — note the enqueue count).
- Appearance settings → theme preview for each of the six themes shows the
  pin's token values (the `tokens.json` byte copies; with P4.D357's CSS on
  the union the whole pass is that lane's row).
- `quilltap completion bash | grep keystone` on the union binary (the
  templates' owner's row); `quilltap recall-replay --help | tail` shows
  `innerCircleShare, innerCircleAllowance` (P4.D342's row).
- NOT a row: `self_inventory`'s `quilltap.releaseNotes` (B.3 — `docs_dir`
  is `None`; the tool answers "not found" by design).
