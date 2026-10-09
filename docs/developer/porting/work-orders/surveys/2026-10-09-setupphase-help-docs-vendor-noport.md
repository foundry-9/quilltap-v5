# Survey — the vendored trees (`help/**`, `docs/v4/**`, the export schema, the CLI help/completion templates), the live-checkout guards, and the NO-PORT? ratifications across `f5e953a3f..01a83539d`

Read-only planning survey, 2026-10-09. v4 = `~/source/quilltap-server` at HEAD `01a83539d` (`4.10.0-dev.143`, tree CLEAN, unmodified by this survey); v5 = `quilltap-v5` main at `96cfdaaf7`. Every fact below was measured from the hunks (`git show <sha> -- <path>`, `git diff f5e953a3f 01a83539d -- <path>`) or by `md5`/`wc -c` against `git show <rev>:<path>`; one v4 CLI run (`node packages/quilltap/bin/quilltap.js --help` / `recall-replay --help`, output under `$TMPDIR/setupphase-survey-help-docs-vendor-noport/`, v4 tree verified clean afterwards). No cargo run.

Slug: `help-docs-vendor-noport`. The thirteen commits: `1825bfd53`, `3f7320138`, `da98ca58b`, `f7d8064be`, `d58548051`, `70f9b495c`, `197104649`, `7e9eaf42c`, `783385873`, `5abcd01ea`, `3c56a41e7`, `ed8b15b50`, `01a83539d`.

---

## 1. `help/**`

### v4 at HEAD — facts

`git diff --stat f5e953a3f 01a83539d -- help/`: **9 files, +177 / −22; 130 → 131 files** (`git ls-tree -r --name-only` counts: 130 at `f5e953a3f`, 131 at `01a83539d`). Per commit:

| commit | pages (± lines) |
|---|---|
| `3f7320138` (F1–F9) | `memory-housekeeping.md` (+7/−1), `memory-recall-relevance.md` (+8) |
| `f7d8064be` (recall retuning) | `episodic-memory.md` (+1/−1), `memory-recall-relevance.md` (+5/−1) |
| `d58548051` (tiers + consolidation) | `cli-memories.md` (+15/−3), **NEW `memory-consolidation.md` (+82)**, `memory-housekeeping.md` (+22/−12), `memory-recall-relevance.md` (+10/−1), `memory-regenerate.md` (+1) |
| `ed8b15b50` (bugs 183–186) | `mount-points.md` (+3/−1), `system-backup-restore.md` (+11), `wardrobe.md` (+3/−3) |
| `01a83539d` (picture viewer) | `wardrobe.md` (+11/−1 — new `### Viewing a Picture Properly`) |
| the other eight | none |

Byte sizes (v5 today = baseline, md5-identical; target):

| page | v5 / `f5e953a3f` | `01a83539d` |
|---|---|---|
| `help/cli-memories.md` | 15,953 | 17,576 |
| `help/episodic-memory.md` | 6,404 | 6,708 |
| `help/memory-consolidation.md` | — | 7,426 (NEW) |
| `help/memory-housekeeping.md` | 8,890 | 11,461 |
| `help/memory-recall-relevance.md` | 10,955 | 14,858 |
| `help/memory-regenerate.md` | 4,796 | 5,288 |
| `help/mount-points.md` | 28,607 | 29,770 |
| `help/system-backup-restore.md` | 16,730 | 17,468 |
| `help/wardrobe.md` | 55,388 | 57,049 |

- Measured: every OTHER of the 131 target files is md5-identical to v5's `help/` today; v5 has no stray file (130 on disk, all present at the target).
- Front matter: the ONLY `url:` line added/changed in the span is the new page's `url: /settings?tab=memory&section=memory-consolidation` (`memory-consolidation.md:2`, its `help_navigate` at `:75`). Headings of the new page: `# Memory Consolidation` `:5`, `## What a Run Does` `:15`, `## When It Runs` `:27`, `## Settings` `:38`, `## Memories About Others` `:51`, `## The Commonplace Book List` `:63`, `## In-Chat Navigation` `:71`, `## Related Settings` `:77`. Renamed heading: `memory-housekeeping.md` `### Also merge semantically similar memories during the sweep` → `### Merging similar memories (retired)`; added `## The Archive (Active and Archived Memories)`, `### Deleting archived memories`; `memory-recall-relevance.md` adds `## How Much the Book Whispers`, `## Digests First, Fragments After`; `cli-memories.md` adds `` ## `consolidate` — Fold Clusters into Digests ``.
- Walk order (libuv `strcmp`): `help/memory-consolidation.md` lands at index 69 (1-based), between `help/math-notation.md` and `help/memory-housekeeping.md` — every later row's `help_docs` rowid shifts by one in `help_tree_equivalence`'s walk-order comparand.
- `lib/help-guide/categories.ts` and `lib/help/**` UNMOVED in the span — the new page sits in no Guide category in v4 either (v5's `apps/web/src/app/help/help-categories.ts` needs NO move).

### v5 today — counterparts

- Vendored tree: `<repo>/help/` (130 files), embedded at compile time by `crates/quilltap-host/build.rs` (`find_markdown_files` mirror of v4's `findMarkdownFiles`, `include_str!` table into `OUT_DIR/help_embedded.rs`, `cargo:rerun-if-changed` per file) → `quilltap_host::help_content::EMBEDDED_HELP` → `files_store::embedded_help_source_files()`; consumed by the boot reconcile (`host.rs`, `HelpDocReconcileGate::ensure`) and `services/embedding_reindex_job.rs`.
- Count pins (all three move 130 → 131 together):
  - `crates/quilltap-harness/tests/help_tree_embed_guard.rs:84` `const VENDORED_FILE_COUNT: usize = 130;` (+ the history doc paragraphs `:22-83`; add a "131 at v4 `01a83539d` (130 + 1)" paragraph).
  - `crates/quilltap-host/tests/host_help_docs_boot.rs:108` `assert_eq!(expected, 130, "the vendored tree at v4 f5e953a3f");` + history comment `:106-107` + module doc "130" at `:7`, `:14`, `:17`.
  - `crates/quilltap-harness/tests/help_tree_equivalence.rs:124-128` compares `files.len()` against the ORACLE's `count` (history comment `:117-123`) — no literal; reddens by regen.
- Tree-dependent regen families: `help_tree_equivalence` (`QT_ORACLE_HELP_TREE`, case `harness/oracle/cases/help-tree-sync.test.ts`, jest recipe at the file's `:19-32`) and `help_section_size_equivalence` (`QT_ORACLE_HELP_SECTION_SIZE`, `harness/oracle/cases/help-section-size.ts` via `npx tsx`; needs `js-tiktoken` in the pin's `node_modules`).
- Tree-INDEPENDENT (fixture-isolated, no move): `help_doc_sync_equivalence`, `help_doc_sync_guards_equivalence`, `help_doc_ensure_equivalence`, `embedding_remainder_equivalence`, `help_context_resolver_equivalence` (committed corpus), `help_chat_orchestrator_tier3_equivalence` (the oracle stubs `ensureHelpDocsSynced` — `help-chat-orchestrator-tier3.test.ts:138-149`), `help_chats_routes_equivalence`; `crates/quilltap-web/tests/help_web_routes.rs:102` reads the count dynamically.
- SPA: `apps/web/src/app/screens/settings/memory/memory-housekeeping-card.ts:132` carries the retired checkbox label "Also merge semantically similar memories during the sweep" — that is the MEMORY-SPA lane's hunk (`d58548051` drops the checkbox), not a help-tree reference. Nothing in the SPA links `section=memory-consolidation` yet: the help page's `help_navigate` target needs a `memory-consolidation` section anchor on the Settings Memory tab — the memory SPA lane's.

### What the ledger got wrong

Nothing on help: the §1 "130 → 131 with `help/memory-consolidation.md`, plus five modified pages … `ed8b15b50` modifies three more … `wardrobe.md` again in `01a83539d`" is exact (9 files).

### Differential plan

- Predicted RED on a target-pin regen until re-vendored: `help_tree_equivalence` (131 vs 130 + walk order + docs/chunks of the 9 pages), `help_section_size_equivalence` (`meta.files` 131 + the new/changed sections' texts). `help_tree_embed_guard` and `host_help_docs_boot` stay GREEN until the copy lands, then RED until their constants move (the mutation: re-vendor, observe 131 ≠ 130, move the constants).
- No new case needed — the existing tree-sync oracle IS the comparand (v4's real `ensureHelpDocsSynced` over the pin's tree). v4's own `help-doc-size.test.ts` holds the new sections under `HELP_SECTION_EMBEDDING_MAX_TOKENS`; the section-size family re-proves it for v5's composed texts.

---

## 2. `docs/v4/**` (the v4 `docs/` mirror)

### Mirror rule (measured)

v5's `docs/v4/` = v4's WHOLE `docs/` tree with the `docs/` prefix stripped (409 files at `f5e953a3f`, **all 409 md5-identical today**) + `docs/v4/packages-quilltap-README.md` (= v4 `packages/quilltap/README.md`; 410 tracked files). NOT mirrored: v4 root `README.md`, `CLAUDE.md`, `.claude/**` (P4.D260 R-E). Three untracked `.DS_Store` files sit in the on-disk mirror (`docs/v4/`, `developer/`, `developer/features/`) — never commit them.

### Paths the span moved (all to re-vendor, byte-exact from `01a83539d`)

| v4 path (→ `docs/v4/<path minus docs/>`) | status | bytes @target | commits |
|---|---|---|---|
| `docs/CHANGELOG.md` | M | 222,069 | `3f7320138` `da98ca58b` `f7d8064be` `d58548051` `70f9b495c` `197104649` `7e9eaf42c` `ed8b15b50` `01a83539d` |
| `docs/developer/API.md` | M | 244,841 | `3f7320138` `f7d8064be` `d58548051` `01a83539d` |
| `docs/developer/BACKGROUND_JOBS_CHILD.md` | M | 31,139 | `d58548051` |
| `docs/developer/CLI.md` | M | 6,558 | `3f7320138` |
| `docs/developer/DDL.md` | M | 138,940 | `d58548051` `ed8b15b50` |
| `docs/developer/bugs.md` | M | 328,855 | `3f7320138` `70f9b495c` `5abcd01ea` `3c56a41e7` `ed8b15b50` |
| `docs/developer/bugs/fixed/bug-182-reinforcement-count-race.md` | A | 5,074 | `70f9b495c` |
| `docs/developer/bugs/fixed/bug-183-wear-line-last-last-week.md` | A | 4,641 | `5abcd01ea` (at `bugs/`) → `ed8b15b50` (moved to `fixed/`, edited) |
| `docs/developer/bugs/fixed/bug-184-wardrobe-tools-household-wear-as-own.md` | A | 6,601 | same |
| `docs/developer/bugs/fixed/bug-185-restore-orphans-archived-stores.md` | A | 8,199 | `3c56a41e7` (at `bugs/`) → `ed8b15b50` (deleted + re-added under `fixed/`) |
| `docs/developer/bugs/fixed/bug-186-duplicate-store-names.md` | A | 6,846 | `ed8b15b50` |
| `docs/developer/features/complete/wardrobe-item-images.md` | M | 27,393 | `01a83539d` |
| `docs/developer/features/complete/wardrobe-wear-ledger.md` | M | 28,035 | `ed8b15b50` |
| `docs/developer/features/memory-consolidation-and-tiers.md` | A | 26,371 | `1825bfd53` `d58548051` |
| `docs/developer/features/memory-recall-and-housekeeping-fixes.md` | A | 17,405 | `1825bfd53` `3f7320138` `da98ca58b` `70f9b495c` |
| `docs/developer/features/recall-multiplier-retuning.md` | A | 15,757 | `da98ca58b` `f7d8064be` |
| `docs/developer/features/recall-probe-set-runbook.md` | A | 79,384 | `f7d8064be` `70f9b495c` |
| `docs/releases/4.10.0.md` | M | 69,192 | `783385873` |
| `packages/quilltap/README.md` (→ `docs/v4/packages-quilltap-README.md`) | M | 38,969 | `3f7320138` `f7d8064be` `d58548051` |

= **19 paths** (10 modified, 9 new). At the target no file remains at `docs/developer/bugs/bug-18{3,4,5}-*.md` (the intermediate paths) — vendor only the `fixed/` copies. Count after: `docs/developer/bugs/fixed/` 181 → 186 in the mirror (v4 target: 186).

Unmirrored files the span also moved (record, don't copy): root `README.md` (`7e9eaf42c` badge + others; 9 commits touch one of the three), `CLAUDE.md` (`ed8b15b50` — ONE added bullet: the store-names namespace rule, `reconcileStoreNames`), `.claude/commands/update-documentation.md` (`da98ca58b` +1, later +4).

**`DDL.md` (the bug-186 reversal, `ed8b15b50`):** the "`name` uniqueness (case-insensitive, app-level) … There is deliberately no DB unique index" paragraph is REPLACED by a `CREATE UNIQUE INDEX "idx_doc_mount_points_name_nocase" ON "doc_mount_points" ("name" COLLATE NOCASE);` block + "`name` uniqueness (case-insensitive)" + a new "Character-vault names" paragraph. `d58548051` adds `chats."otherExtractionWatermarkMessageId"` (documented between `commonplaceRecallHistory` and `timelineMode` — the doc's placement, NOT generateDDL's; do not read column order from DDL.md), the four `memories` tier columns, `idx_memories_character_tier`, and a "Tiers (4.10+)" paragraph. Documentation only — the D23 re-dump comes from `generateDDL`, never from DDL.md.

### v5 consumers of the mirror

None at run time: `grep docs/v4` finds only a comment in `crates/quilltap-cli/tests/cli_differential.rs:1812` and the spelling-checker allow-prefix `harness/tools/check_spelling.py:63`. No guard pins mirror bytes — the obligation is held by the lane record's `cmp` list only.

### What the ledger got wrong

- §1 "`docs/v4/`: … residual only `packages-quilltap-README.md`" is **stale/false at the baseline**: measured, `docs/v4/packages-quilltap-README.md` is md5-identical to `f5e953a3f:packages/quilltap/README.md` (the mirror was fully current at the baseline). It DOES move in this span (three memory commits) and joins the re-vendor.
- The brief/ledger name three new `features/` specs; there are **four** — `recall-probe-set-runbook.md` (79 KB, `f7d8064be`, amended by `70f9b495c`) is the one not named.

---

## 3. `qtap-export.schema.json` (and the other vendored schemas / templates)

### v4 facts

- `public/schemas/qtap-export.schema.json`: **moved ONLY by `d58548051`** (`git log f5e953a3f..01a83539d -- public/` = that one commit; +23 / −1). Bytes: 101,092 @`f5e953a3f` (md5 `d04df340…`, = v5's copy) → **102,523 @`d58548051` = @`01a83539d`** (md5 `6990efe6…`).
- The hunks: `$defs.ExportedChat.properties` gains `otherExtractionWatermarkMessageId` `{type: ["string","null"], description}` (target `:553`, after `coreWhisperInterval`, before `timelineMode`); `$defs.Memory.properties.source` gains a description ("AUTO | MANUAL | CONSOLIDATED …" — no enum); NEW `tier` `{type:"string", enum:["hot","cold"]}` (`:992`), `supersededById` `anyOf [UUID, null]`, `consolidatedFrom` `array of UUID`, `consolidatedAt` `anyOf [Timestamp, null]` — between `source` and `occurredAt`. Both `$defs.Memory` (`:981`) and `$defs.ExportedChat` (`:511`) are `additionalProperties: true`, so the move is NOT a new-key refusal; it is an **acceptance TIGHTENING for present keys** (a `tier` off-enum, a non-uuid `supersededById`/`consolidatedFrom` item, a non-string watermark now fail).
- Validation consumers (both sides): ONLY the AI import (`lib/services/ai-import.service.ts:1144,1214` → v5 `generators/ai_import.rs:1480,1949`). v5's `.qtap` import/export/backup do NOT read the schema; export KEY ORDER comes from `services/qtap_export/schema-key-order.json` (a dump of v4's ZOD schemas via `harness/oracle/fixtures/dump-export-key-order.ts`, `key_order.rs:18-27`) — which `d58548051` ALSO moves (`MemorySchema` +4, `ChatMetadataSchema` +1) but which is the memory-carriers lane's, not a vendor copy.
- Other vendored schemas: `qtap-custom-tool.schema.json` (39,471) and `qtap-progression.schema.json` (5,963) — **md5-identical** at the target to `apps/web/public/schemas/`. `plugin-manifest`, `qtap-export-ndjson`, `qtap-theme` schemas: not vendored by v5, unmoved anyway.
- Built-in prompt templates (`plugins/dist/qtap-plugin-default-system-prompts/prompts/*.md`, 21 files): `plugins/` UNTOUCHED by all thirteen.

### v5 counterparts

- `crates/quilltap-core/src/generators/qtap-export.schema.json` (101,092 B), `include_str!` at `generators/qtap_schema.rs:59`; size pins **`generators/qtap_schema.rs:184`** `assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(), 101_092);` (history comment `:167-183`) and **`crates/quilltap-harness/tests/qtap_schema_embed_guard.rs:48`** `const VENDORED_BYTES: usize = 101_092;` (history `:23-47`). Both → **102,523**.
- `qtap_schema_validate_equivalence` (+ `harness/oracle/cases/qtap-schema-validate.test.ts`, corpus `harness/oracle/fixtures/qtap-schema-validate.json`: 6+ seeds from v4's REAL exporter over the committed `system-data-*` fixtures, 32 mutations incl. `memory_missing_required_summary` / `memory_chat_id_*`, 12 literals). The oracle validates with the PIN's schema — a target-pin regen against v5's stale copy diverges on any row touching the new keys.

### Differential plan

- Re-vendor byte-exact; move both size literals; regen `qtap_schema_validate_equivalence` at the target pin. Proposed new MUTATION rows (v4's exact `valid` + `errorSections` + instance paths): `memory_tier_off_enum` (`tier: "warm"`), `memory_superseded_by_not_uuid`, `memory_consolidated_from_item_not_uuid`, `memory_consolidated_at_bad_date`, `chat_watermark_not_string` (`otherExtractionWatermarkMessageId: 5`), and a VALID `memory_digest_well_formed` (`source: "CONSOLIDATED"`, `tier: "hot"`, `consolidatedFrom: [uuid,uuid]`).
- ⚠ Measure first: the seeds run v4's TARGET exporter over BASELINE-vintage committed fixtures that lack `memories.tier` & co. If v4's export path issues a hot-only `COALESCE(tier,'hot')` read, the seed generation fails `no such column` on those fixtures (the rebuilt-fixture-vintage trap). Whoever regens must check the seed rows still generate; if not, that is a carriers/fixture HANDOFF, not a vendor fix.

---

## 4. CLI help + completion templates (vendored from `packages/quilltap/`)

### v4 facts

`git diff --stat f5e953a3f 01a83539d -- packages/quilltap/`: 12 files, +705 / −35 — `README.md`, `bin/quilltap.js` (+9/−1, `3f7320138`), NEW `lib/anchor-probe-command.js` (191, `3f7320138`), `lib/completion/{bash,fish,zsh}.template` (`3f7320138` + `f7d8064be` + `d58548051`), `lib/memories-commands.js` (`d58548051`), NEW `lib/memories-consolidate-command.js` (131, `d58548051`), `lib/recall-replay-command.js` (`3f7320138` + `f7d8064be`), `lib/__tests__/completion-coverage.test.js` (+1: `'anchor-probe': ['lib/anchor-probe-command.js','printAnchorProbeHelp']`), NEW `lib/__tests__/memories-consolidate-command.test.js`, `package.json` (version only).
- Top-level help (`bin/quilltap.js:106`): ONE line `  anchor-probe <characterId>    Probe whether episodic anchor lines suppress Memory Gate reinforcement` after `recall-replay`; `SUBCOMMANDS` (`:1174`) gains `'anchor-probe'` after `'recall-replay'`; dispatch arm `:1246-1251`.
- bash template: `top_cmds` gains `anchor-probe` after `recall-replay`; `vf_memories` gains `--tier`; `vf_recall_replay` → `" --turn --char --limit --memory-budget --tuning --tuning-file --signals-from --port "`; NEW `vf_anchor_probe=" --limit --port "` + its `case` arm; `--source` → `AUTO MANUAL CONSOLIDATED`; NEW `--tier` → `hot cold`; `mem_verbs` gains `consolidate`; `mem_flags` gains `--tier`, `--dry-run`; `rr_flags` gains `--memory-budget --tuning --tuning-file --signals-from --as-of`; NEW `anchor-probe)` arm `ap_flags="--limit --port --json --help"`. (fish/zsh: the same additions in their dialects.)
- `recall-replay --help` grows 25 lines (`--limit` default re-worded, `--memory-budget`, `--tuning`, `--tuning-file`, `--signals-from`, `--as-of`, two examples); the table render gains `Harness` and `Tuning` header lines and `, head N` in each path title.

### v5 counterparts

- `crates/quilltap-cli/src/help/completion/{bash,fish,zsh}.template` — md5-identical to `f5e953a3f`, **DIFF at `01a83539d`** (measured); `include_str!` at `completion_cmd.rs:15-17` and `tests/completion_behavior.rs:24-25,297`.
- `crates/quilltap-cli/src/help/main_help.txt` — measured `diff` vs v4 HEAD's `--help`: exactly the one `anchor-probe` line missing (after `:18`).
- `crates/quilltap-cli/src/main.rs:47-61` `SUBCOMMANDS` (no `anchor-probe`); `recall_replay_cmd.rs` inline `const HELP` (the memory/CLI lane's).
- Couplings: `completion_behavior.rs::completions_offer_every_flag_the_help_text_advertises` (`:428`) requires every flag in each DISPATCHED subcommand's help to appear in all three templates → templates may land BEFORE the recall-replay help grows (superset is fine), never after. `help_sources_cover_every_dispatched_subcommand` (`:399`) parses `SUBCOMMANDS` + `"x" => ` dispatch arms → adding `anchor-probe` as a REAL dispatch needs a help source there; adding it as `not_yet_available` does not.
- Ownership options: (a) **the CLI unit of the memory catch-up owns all of it** (templates, `main_help.txt`, `SUBCOMMANDS`, `recall_replay_cmd.rs` help) — recommended: the templates advertise `anchor-probe` / `memories consolidate` / `--tier` / the replay flags, and Tier R only goes green when the help texts and templates move together; (b) the vendoring lane copies the three templates byte-exact (zero code; `completion bash|zsh|fish` Tier R cases go green) and the CLI unit owns `main_help.txt` + `SUBCOMMANDS` + `recall_replay_cmd.rs` — workable, but splits one Tier R surface across two lanes.

---

## 5. Dependencies — confirmed NONE moved

`git diff f5e953a3f 01a83539d -- package.json packages/quilltap/package.json package-lock.json`: version stamps ONLY (`4.10.0-dev.117` → `4.10.0-dev.143`, three lines in the lock); `plugins/**` and every `packages/*/package.json` dependency block untouched. Per-commit versions: `.117` (`1825bfd53`), `.120` (`3f7320138`, `da98ca58b`), `.137` (`f7d8064be`), `.138` (`d58548051`), `.139` (`70f9b495c`), `.140` (`197104649`), `.141` (`7e9eaf42c` … `3c56a41e7`), `.142` (`ed8b15b50`), `.143` (`01a83539d`). Installed (live checkout, measured): root `openai` 7.30.0, `@openrouter/sdk` 1.4.25, `zod` 4.6.5; plugin `openai` 7.23.0 ×6, `@anthropic-ai/sdk` 0.115.0, `@google/genai` 1.52.0, `@openrouter/sdk` 1.3.28; Node v24.13.1 — every one equals the recorded constant.

---

## 6. NO-PORT? ratification evidence

- **`1825bfd53`** "docs: memory improvement specs" — `--stat`: 2 NEW files, `docs/developer/features/memory-consolidation-and-tiers.md` (+465), `…/memory-recall-and-housekeeping-fixes.md` (+199); no code, version `.117` unchanged. Vendor the TARGET copies (both amended later: `d58548051`; `3f7320138`/`da98ca58b`/`70f9b495c`). **RATIFY NO-PORT.**
- **`da98ca58b`** "docs(memory): F2 anchor-probe result + recall multiplier retuning spec (#84)" — `--stat`: `.claude/commands/update-documentation.md` (+1, unmirrored), `docs/CHANGELOG.md` (+17), `memory-recall-and-housekeeping-fixes.md` (+46/−5), NEW `recall-multiplier-retuning.md` (+241). **F2's result (the hunk):** Status line now "F2 is decided: no gate change"; implementation note "Run on Friday 2026-10-08; the gate stays as it is"; the commit body: "Friday, 50 most recent rows: nothing reaches 0.85/0.90 with or without the anchor line; anchor-free scores are lower in 45 of 50. Restatements of one fact score ~0.65-0.73, which is left to the consolidation spec." → the Memory Gate thresholds/comparison text do NOT move; the probe (`anchor-gate-probe.ts`, the route action, `quilltap anchor-probe`) stays a measurement tool (PORT-NEW in `3f7320138`'s row). No code. **RATIFY NO-PORT.**
- **`7e9eaf42c`** "test: release checklist 2" — 27 files: **19 NEW test files** + ONE new source `app/salon/[id]/hooks/all-llm-pause-actions.ts` (24 lines) + `SalonView.tsx` (+6/−5) + `useChatControls.ts` (−11) + `README.md` badge + `docs/CHANGELOG.md` (+16) + version stamps.
  - **The refactor, line by line.** v4 before (`SalonView.tsx:1455-1459` @base): `modals.setAllLLMPauseModalOpen(false); await chatControls.setPauseState(false); await turnManagement.handleContinue()`. After: `continueAllLLMRoom({ closeModal, setPauseState, handleContinue })` = `closeModal(); await setPauseState(false); await handleContinue();` (`all-llm-pause-actions.ts:16-24`) — identical order and awaits. `handleAllLLMStop` (`SalonView.tsx:1462-1465` @target) unchanged: close + un-awaited `chatControls.setPauseState(true)`. `useChatControls.ts` loses `handleAllLLMContinue` (empty no-op) and `handleAllLLMStop` (`setPauseState(true)`) and their two return keys — `git grep` at the target: no consumer remains (SalonView defines its own; `ChatModals.tsx:103-104,149,496-497` receive SalonView's).
  - **v5:** the Continue handler is NOT in `apps/web/src/app/chat/all-llm-pause*.ts` (those are the thresholds twin `all-llm-pause.ts` — `INITIAL_PAUSE_INTERVAL`, `getNextPauseThreshold` … — and the modal `all-llm-pause-modal.ts`). It is `apps/web/src/app/screens/salon/salon-conversation.ts:1755-1759` `onAllLLMContinue()`: `this.showAllLLMPause.set(false); await this.setPauseState(false); await this.onSidebarSkip();` — the same three steps in the same order (`onSidebarSkip` = v4 `handleContinue`, documented `:1741-1754`); Stop `:1762-1768` (close + `chatUpdate {isPaused:true}` + invalidate); pinned by `screens/salon/salon-turn-controls.spec.ts:1500` ("the all-LLM Continue RESUMES first, then asks for the next speaker (bug 139)": dispatch order `chatUpdate` → `chatTurnAction` → `chatSend`, `isPaused: false`). v5 has no `useChatControls` no-op twins (grep). → **RATIFY NO-PORT.** Optional (not required): tighten the v5 spec to v4's shape — hold the `chatUpdate` promise unresolved and assert no `chatTurnAction` before it resolves.
  - **The 19 new v4 tests → the v5 surface each is reference material for:**
    1. `__tests__/unit/app/api/v1/characters/[id]/avatar-rolls/[fileId]/route.test.ts` → `api/characters.rs` avatar-rolls fns + `crates/quilltap-web/src/characters_routes.rs`; families `crates/quilltap-web/tests/avatar_rolls_routes.rs`, `avatar_rolls_tier2_equivalence`.
    2. `…/avatar-rolls/route.test.ts` → same.
    3. `…/subprompts/[subpromptId]/route.test.ts` → `api/subprompts.rs`; `subprompts_routes_equivalence`, `subprompts_web_routes`.
    4. `…/subprompts/route.test.ts` → same.
    5. `__tests__/unit/app/aurora/groups/GroupScenariosCard.test.tsx` → SPA `screens/groups/group-scenarios-card.ts`.
    6. `__tests__/unit/app/salon/hooks/all-llm-pause-actions.test.ts` → `screens/salon/salon-conversation.ts:1755` + `salon-turn-controls.spec.ts:1500`.
    7. `…/salon/hooks/useConciergeRetry.test.ts` → SPA `chat/concierge-retry.state.ts` / `concierge-retry.ts`.
    8. `…/salon/hooks/useRegeneration.test.ts` → SPA `chat/regeneration.state.ts`.
    9. `…/salon/hooks/useSummaryActions.test.ts` → SPA Rebuild Summary in `screens/salon/salon-conversation.ts` (+ `chat/chat-admin.api.ts`, spec `salon-conversation.spec.ts:4293`); server `chat_rebuild_summary_equivalence`.
    10. `__tests__/unit/background-jobs/maintenance/fold-other-catchup.test.ts` → NEW in `d58548051` (`fold-other-catchup.ts`) — the memory lane's scheduled-maintenance unit.
    11. `__tests__/unit/config/next-config-pdf-externals.test.ts` → NO v5 surface (Next.js bundler config, bug 178 already NO-PORT-RATIFIED).
    12. `__tests__/unit/lib/background-jobs/handlers/memory-consolidation.test.ts` → NEW (`d58548051` handler) — memory lane.
    13. `__tests__/unit/lib/memory/consolidation-triggers.test.ts` → NEW (`d58548051`) — memory lane.
    14. `__tests__/unit/lib/mount-index/sync/apply-store.test.ts` → `services/mount_index/sync/apply_store.rs`; `sync_engine_equivalence`.
    15. `…/sync/orchestrator.test.ts` (imports `@/lib/mount-index/sync`) → `services/mount_index/sync/mod.rs`; `sync_engine_equivalence`.
    16. `…/sync/walk-disk.test.ts` → `sync/walk_disk.rs`.
    17. `…/sync/walk-store.test.ts` → `sync/walk_store.rs`.
    18. `__tests__/unit/lib/photos/save-attribution.test.ts` → `crates/quilltap-core/src/photos/save_attribution.rs` (`resolve_save_attribution` `:153`); `photo_tools_equivalence`, `photos_routes_equivalence`.
    19. `__tests__/unit/lib/startup/daily-db-optimize.test.ts` → `services/daily_db_optimize.rs`; `daily_db_optimize_equivalence`, `crates/quilltap-host/tests/host_boot_daily_optimize.rs`.
- **`783385873`** — `--stat`: ONE file `docs/releases/4.10.0.md` (+75/−9); the only span commit touching it. **RATIFY NO-PORT**; the `docs/v4/releases/4.10.0.md` mirror re-vendors (§2).
- **`5abcd01ea`** — `docs/developer/bugs.md` (+4/−2) + NEW `docs/developer/bugs/bug-183-…md` (+58), `bug-184-…md` (+71). **`3c56a41e7`** — `bugs.md` (+2/−1) + NEW `bug-185-…md` (+50). Both already NO-PORT (this port's filings); their files survive only as the `fixed/` copies `ed8b15b50` rewrote (§2). Ratify with the round.

---

## 7. The live-checkout guards — full list and prediction at `01a83539d`

The ONE locator is `crates/quilltap-harness/tests/common/mod.rs:406` `v4_root()` (`QT_V4_CHECKOUT` → `QT_V4_ROOT` → `$HOME/source/quilltap-server`). Code readers (grep of non-comment lines): five harness guards + Tier R. No SPA spec, script or other crate test reads the checkout at run time.

| guard | reads | prediction @ live `01a83539d` | why | flipped by |
|---|---|---|---|---|
| `qtap_schema_embed_guard::the_embedded_schema_equals_the_v4_checkouts` (`:89-108`) | `public/schemas/qtap-export.schema.json` | **RED** | 102,523 B vs vendored 101,092 (`d58548051`) | schema owner (§3) — re-vendor + `:48` + `qtap_schema.rs:184` |
| `qtap_schema_embed_guard::the_embedded_schema_is_self_consistent` | none (v5 only) | GREEN now; RED the moment the copy lands without the `:48` move | size pin | same |
| `public_schemas_vendor_guard` (`:137`) | `public/schemas/qtap-{custom-tool,progression}.schema.json` | GREEN | md5-identical (measured) | — |
| `builtin_prompt_templates_guard` (`:74`) | `plugins/dist/qtap-plugin-default-system-prompts/prompts/` | GREEN | `plugins/` untouched | — |
| `provider_sdk_version_guard` (`:162`) | root + `plugins/dist/*/node_modules` SDK versions | GREEN | all installed = recorded (measured) | — |
| `zod_version_guard` (`:114`) | `node_modules/zod/package.json` | GREEN | 4.6.5 | — |
| `crates/quilltap-cli/tests/cli_differential.rs` (Tier R, `QT_V4_CHECKOUT` `:1959`) | runs v4's `packages/quilltap/bin/quilltap.js` | **RED** at a live-HEAD run: `main help` (`:1992`; the `anchor-probe` line — measured), `completion bash`/`zsh`/`fish` (`:4252-4254`; templates DIFF — measured), `recall-replay help` (`:4473`; +25 help lines), `recall-replay table` (new `Harness`/`Tuning` lines); predicted GREEN: `completion help`/`no args`/`unknown shell`, `db`/`docs`/`instances`/`sync` help, the `recall-replay` validation arms whose messages did not move (`--turn`, `--limit`, `--port`, unknown option), `recall-replay json` (raw echo of the canned payload — measure). GREEN against a `f5e953a3f` pin | the CLI unit of the memory catch-up (§4) |

Regen-at-target families tied to the vendored trees (not live guards, but they read the PIN's tree): `help_tree_equivalence`, `help_section_size_equivalence` (RED until §1 lands), `qtap_schema_validate_equivalence` (RED on rows touching the new keys until §3 lands). The ledger §1's prediction list is CORRECT on every guard (schema RED, help RED on regen, Tier R RED, the four others GREEN).

---

## 8. Proposed lane: the vendoring lane (harness + vendored trees, ZERO predicted core hunks)

Precedent: P4.D260 (R-A — `help/**` re-vendored WHOLE at the target in ONE lane; a split vendor leaves `help_tree_equivalence` red on every lane). Run from `main`, pin `01a83539d` (v4 HEAD = target, clean — the live checkout IS the target unless v4 moves; re-probe §2 before the regen).

- **U1 — `help/**` WHOLE at `01a83539d` (131).** Copy the 9 paths byte-exact (`git show 01a83539d:help/<f>`); verify all 131 md5 = target, `find help -type f | wc -l` = 131. Move `help_tree_embed_guard.rs:84` → 131 (+ doc paragraph "131 at v4 `01a83539d` (130 + 1): `help/memory-consolidation.md` (`d58548051`); the eight re-vendored pages (`cli-memories`, `episodic-memory`, `memory-housekeeping`, `memory-recall-relevance`, `memory-regenerate`, `mount-points`, `system-backup-restore`, `wardrobe`) move no count"); `host_help_docs_boot.rs:108` → 131 + `:7/:14/:17` + `:106-107`; `help_tree_equivalence.rs:117-123` history line. Rebuild the host (build.rs embeds) before the guards; regen `help_tree_equivalence` (jest recipe) and `help_section_size_equivalence` (tsx; `js-tiktoken`) at the pin. Red-first: the embed guard at 131-on-disk vs 130.
  Files: `help/{cli-memories,episodic-memory,memory-consolidation,memory-housekeeping,memory-recall-relevance,memory-regenerate,mount-points,system-backup-restore,wardrobe}.md`; `crates/quilltap-harness/tests/{help_tree_embed_guard,help_tree_equivalence}.rs`; `crates/quilltap-host/tests/host_help_docs_boot.rs`.
- **U2 — `docs/v4/**` to the target, 19 paths byte-exact** (the §2 table; `cmp` each; the five `bugs/fixed/` adds; the four `features/` adds incl. `recall-probe-set-runbook.md`; `packages-quilltap-README.md`). Record the three unmirrored moved files. No test.
  Files: `docs/v4/CHANGELOG.md`, `docs/v4/developer/{API,BACKGROUND_JOBS_CHILD,CLI,DDL,bugs}.md`, `docs/v4/developer/bugs/fixed/bug-18{2,3,4,5,6}-*.md`, `docs/v4/developer/features/complete/wardrobe-{item-images,wear-ledger}.md`, `docs/v4/developer/features/{memory-consolidation-and-tiers,memory-recall-and-housekeeping-fixes,recall-multiplier-retuning,recall-probe-set-runbook}.md`, `docs/v4/releases/4.10.0.md`, `docs/v4/packages-quilltap-README.md`.
- **U3 — the export schema (RECOMMENDED here; alternative: the memory-carriers unit).** Byte-copy `public/schemas/qtap-export.schema.json` @target → `generators/qtap-export.schema.json`; `qtap_schema.rs:184` + `qtap_schema_embed_guard.rs:48` → 102,523 (+ history lines "102,523 at v4 `01a83539d` (`d58548051`: `ExportedChat.otherExtractionWatermarkMessageId`, `Memory.tier`/`supersededById`/`consolidatedFrom`/`consolidatedAt`, the `source` description)"); regen `qtap_schema_validate_equivalence` + the §3 mutation rows. Rationale for owning it here: in v5 the file is read ONLY by `validate_qtap_export` (AI import) — no carrier reads it; P4.D264 owned it only because that lane also touched the corpus. Alternative (a): the memory-carriers unit owns schema + pins + validate corpus together with `schema-key-order.json` (P4.D264's shape) — choose (a) if that unit is already regenerating `qtap_schema_validate` seeds for fixture-vintage reasons.
  Files: `crates/quilltap-core/src/generators/{qtap-export.schema.json,qtap_schema.rs}` (the literal + comment ONLY), `crates/quilltap-harness/tests/{qtap_schema_embed_guard,qtap_schema_validate_equivalence}.rs`, `harness/oracle/fixtures/qtap-schema-validate.json`, `harness/oracle/cases/qtap-schema-validate.test.ts` (only if a mutation needs a new op). Lane crate: core (a literal in a `#[cfg(test)]` block).
- **U4 — NO-PORT ratification record** for `1825bfd53`, `da98ca58b`, `7e9eaf42c` (with the §6 line-by-line + the 19-file map), `783385873`, `5abcd01ea`, `3c56a41e7` in the lane record / status-log (the ledger itself is `/unify`'s). Optional SPA hunk (tighten `salon-turn-controls.spec.ts:1500` to v4's unresolved-promise shape) — only if the human wants it; otherwise none.
- **NOT this lane (alternative only):** the CLI templates + `main_help.txt` + `SUBCOMMANDS` + `recall_replay_cmd.rs` help (§4 option a → the memory catch-up's CLI unit, which flips Tier R). If option (b) is chosen, U5 = byte-copy the three templates only (`crates/quilltap-cli/src/help/completion/*.template`; Tier R's three `completion` cases green; `completion_behavior` stays green — templates are a superset).

Forbidden for the vendoring lane: every `crates/quilltap-core/src/**` file except the U3 test literal; `services/qtap_export/schema-key-order.json`; `apps/web/**` (except the optional U4 spec); `crates/quilltap-cli/src/**` under option (a).

## 9. Meeting points

- **Memory SPA lane:** must give the Settings Memory tab a `section=memory-consolidation` anchor (the new page's `url` + `help_navigate`) and drop the "Also merge semantically similar memories…" checkbox (`memory-housekeeping-card.ts:132`) — the help page now documents "Merging similar memories (retired)".
- **Memory CLI unit:** owns Tier R's flip (`main help`, three `completion` cases, `recall-replay help`/`table`) — needs the three templates (from itself or U5), `main_help.txt` +1 line, `SUBCOMMANDS` + `anchor-probe` (dispatch or `not_yet_available`), the grown `recall_replay_cmd.rs` `HELP`; must land templates no later than the grown help (`completion_behavior.rs:428`).
- **Memory carriers unit:** owns `schema-key-order.json` re-dump (`MemorySchema` + `ChatMetadataSchema` key orders); consumes U3's schema if it regenerates `qtap_schema_validate` seeds; reports the seed-generation vintage measurement (§3 ⚠).
- **Wardrobe / restore / store-names lanes (`ed8b15b50`, `01a83539d`):** consume nothing from this lane; the help pages they describe (`wardrobe.md`, `mount-points.md`, `system-backup-restore.md`) ride U1 whole.
- **Unifier:** recount the help tree (131) on the union rather than taking any lane's number; re-run `help_tree_equivalence` after any lane that touched `help/` (none should).

## 10. Open questions / measurements before coding

1. Re-run the ledger §2 probe at lane start; if v4 moved past `01a83539d`, re-vendor at the ROUND's target pin, not HEAD.
2. `help_section_size_equivalence` regen needs `js-tiktoken` in the pin's `node_modules` (a pinned worktree's symlinked tree — the P4.D260 recipe).
3. The `qtap_schema_validate` seed vintage (§3 ⚠) — measure before choosing U3's owner.
4. Tier R `recall-replay json` and the validation arms — measure at the pin rather than trusting §7's GREEN predictions.
5. Human ruling only if wanted: tighten v5's bug-139 spec (U4 optional).
6. ⚠ **v4's tree went DIRTY during this survey:** an UNTRACKED `docs/developer/features/wardrobe-refactor.md` (22,632 B, mtime 2026-10-09 15:16 CDT) appeared in the v4 checkout — not written by this survey (HEAD still `01a83539d`). Docs-only dirt (not `lib/`/`app/`/`packages/`/`plugins/`), so it poisons no regen, but the §2 probe will now report a dirty tree, and if it is committed it becomes a new `docs/v4/developer/features/` path for U2 (and likely a planning input for a future wardrobe row).
