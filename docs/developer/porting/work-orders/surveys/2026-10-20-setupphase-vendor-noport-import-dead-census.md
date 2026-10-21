# Setupphase survey — the vendor lane, the NO-PORT ratifications, and the IMPORT-DEAD / runtime-break census for `e8e17b2d9..322a5488e`

**Date:** 2026-10-20. **Read-only survey** (nothing built, run or written
except this file).

**Pin:** v4 `322a5488e7bd93f0234a25b42f87d5418dbf4c2c` (`main`, 2026-10-20,
"Fix: custom-tool LLM consult can narrate a verdict the table already reached
(bug 257)", `package.json` `4.11.0-dev.53`). **Baseline:** `e8e17b2d9`
(`4.11.0-dev.26`). **Span:** 22 commits.

**v4 facts** come only from `git -C ~/source/quilltap-server show
322a5488e:<path>` and `git diff e8e17b2d9 322a5488e`. **v5 line numbers** are
at v5 `main` `5a94a9d1e`. Port facts come from the shipped hunks, never from
commit messages. Template: the previous vendor survey
(`2026-10-19-setupphase-riders-vendor-import-dead-census.md`).

---

## Headline findings

1. **IMPORT-DEAD: ZERO. No stage-0 `REAIM` lane.** All 37 `-export` lines in
   the span are re-declared, aliased or re-exported from the SAME module at
   the pin (§E.1); no file was deleted, renamed or copied; zero dead import
   specifiers across `harness/oracle/**`, `crates/**/tests/**`, `apps/web/**`.
2. **ONE stacking constraint instead:** v4's chat create
   (`app/api/v1/chats/route.ts:1444`), participant add
   (`actions/participants.ts:242`) and merge (`apply-chat-merge.ts:267`) now
   call `applyInventoryDefaults`, which WRITES `chats.equippedInventory =
   {"<charId>":[]}` per character participant on every REBUILT fixture. The
   `chat_create_capstone` `chats` dump reds at the pin until the Inventory
   keystone (the 13th D23 column + the arrival write) lands — every lane that
   regenerates a chat-create / participant-add / merge family must stack on
   that keystone (§E.5). On COMMITTED fixtures the write is caught by
   `safeQuery` (no column) and only costs two log lines.
3. **`help/`: 131 → 135** (+`interrupting.md`, `inventory.md`,
   `project-inventory.md`, `the-log.md`; 18 pages modified, +621 / −51).
   `custom-tools.md` carries SEVEN commits' hunks (+205 / −10) — per-lane
   patches are refused by construction; the vendor lane re-vendors WHOLE at
   the pin (P4.D323 R-1), four guards move 131 → 135 (§A).
4. **`docs/v4/`: 20 operations** — 12 overwrites + 8 adds (the two deferred
   Inventory specs land at their FINAL `complete/` paths, `pascal-gate-
   broadening.md` likewise, the five bug files 253–257); 494 → **502** tracked
   (§B).
5. **Both public schemas move and BOTH are now RUNTIME inputs on the engine
   side:** `qtap-custom-tool.schema.json` 44,021 → **63,595** bytes (four
   commits) is imported by `edit_custom_tool`'s `action: 'schema'` together
   with `docs/developer/CUSTOM_TOOL_SPEC.json` (+20 / −1 in the span) and
   `CUSTOM_TOOL_SPEC_DICE.json` — v5 has NO Rust copy of any of the three
   (the custom-tool schema is served by the SPA only). A ruling is owed on
   where the engine embeds them (§C, §H).
6. **`qtap-export.schema.json` 108,678 → 109,559** (`canManageInventory`,
   `equippedInventory`, the Inventory-effects description); three guard
   constants move (§C).
7. **NO-PORT ratifications ready with evidence:** `cad017599` (a test + the
   CHANGELOG), `cc57e6cbc` (docs only), `packages/theme-storybook/**` (by
   precedent), `.claude/commands/update-documentation.md`, v4's `CLAUDE.md`
   and README prose, `themes/tools/madmans-box-icon-preview.html` (§D).
8. **Dependencies: exactly ONE new runtime dependency**, `@xterm/headless
   ^6.0.0` (`a6292f169`), present in the npm cache
   (`headless-6.0.0.tgz`) and the live tree, so the pin's `npm ci --offline`
   works; SDK / zod / plugin-utils guards unchanged (§F).
9. **`V4_APP_VERSION`** lives at ONE site —
   `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:126` —
   `"4.11.0-dev.26"` → `"4.11.0-dev.53"` (§G).
10. **The theme / icon vendor set is split across two SPA lanes AND one
    shared file:** `themes/bundled/madmans-box/theme.json` gains three lines
    from two commits (`2b5e8ee24`, `e27fb6b34`); give the byte copy of
    `theme.json` + its three new icon SVGs to the vendor lane and the SPA
    icons (`public/images/icons/*.svg`, the `icon.ts` union) to the two SPA
    lanes (§D.6).

---

## §0 — The span for this slice, in one paragraph

Across `e8e17b2d9..322a5488e` v4 shipped the Inventory programme
(`4fbdad186`, `e1872c94f`, `102a23648`, `7263105c6`, `e27fb6b34`,
`d88c1ce39`), the Log (`e4a60ad9e`, `a495faa26`), `edit_custom_tool`
(`ea2cb60ab`, `c56fe278c`), the custom-tool gate broadening (`a53024d3c`)
and bug 257 (`322a5488e`), composer interrupts (`2b5e8ee24`, `b8362beb5`),
bug 255's terminal emulator (`a6292f169`, the one new dependency), the
/photos album selector (`680b71044`), vault-only custom tools
(`cfa0cba29`), the moderated-image re-ruling (`406dbdd5a`), the scenario
sort (`1d0a31132`), a toast z-index (`5a8338b30`), one test-only commit
(`cad017599`) and one docs-only commit (`cc57e6cbc`). For THIS slice that
means: `help/` grew by four pages and moved eighteen; `docs/` grew by eight
files (two of them the previously deferred Inventory specs moving to
`complete/`) and moved twelve; both public schemas moved; three additive
migrations, one lazily-created llm-logs table, one new npm dependency; and
a refactor that moved the wardrobe folder helpers into a shared
`item-folders` module while keeping every old export alive under its old
name.

---

## §A — `help/`

### A.1 The counts and the diff

- `git ls-tree -r --name-only e8e17b2d9 -- help | wc -l` = **131**;
  at `322a5488e` = **135**. v5 `help/` = 131 tracked (`git ls-files help`).
- `git diff --stat e8e17b2d9 322a5488e -- help/`: **22 files, +621 / −51**;
  `--name-status`: 4 `A`, 18 `M`, 0 `D`/`R`.
- No `url:` / `title:` front-matter line moves on an existing page.

| Page | +/− | Commits (newest first) | New headings (`##`/`###`) | Content lane |
|---|---|---|---|---|
| `help/interrupting.md` (A) | +55 | `b8362beb5`, `2b5e8ee24` | `# Cutting In — Raising a Hand and Stopping`; `## Raising a Hand`; `## Stopping`; `## In-Chat Navigation`; `## Related Pages`; `url: /salon/:id` | interrupts |
| `help/inventory.md` (A) | +195 | `d88c1ce39`, `e27fb6b34`, `7263105c6`, `102a23648`, `e1872c94f`, `4fbdad186` | `# The Inventory`; `## Where to find it`; `## Where items are kept`; `## Quilltap General`; `## What an item is`; `### A title may not borrow a name`; `## Archetypes and instances`; `## Kits`; `## Carrying things in a chat`; `### Equipped by default`; `## What the characters are told`; `## Carried and equipped`; `## Pictures`; `## Folders`; `## Moving and copying between shelves`; `## Archiving`; `## What the characters may do for themselves`; `### Inventory management`; `## Pascal's custom tools and the Inventory`; `## In-Chat Navigation`; `## Related Pages`; `url: /salon` | Inventory (six commits) |
| `help/project-inventory.md` (A) | +39 | `102a23648`, `4fbdad186` | `# Project Inventory`; `## Where to find it`; `## What a project shelf is good for`; `## Things to know`; `## In-Chat Navigation`; `## Related Pages`; `url: /prospero/:id` | Inventory |
| `help/the-log.md` (A) | +45 | `a495faa26`, `e4a60ad9e` | `# The Log`; `## What a line is`; `## The three tools`; `## Switching it off`; `## Custom tools can write to it`; `## How long it is kept`; `## In-Chat Navigation`; `url: /salon/:id` | the Log |
| `help/ariel.md` | +4 / −0 | `a6292f169` | — | terminal (bug 255) |
| `help/chat-participants.md` | +0 / −1 | `2b5e8ee24` | (drops `### Interrupt/Stop Button`) | interrupts |
| `help/chat-turn-manager.md` | +3 / −3 | `2b5e8ee24` | `### Raising a Hand and Stopping` (replaces `### Interrupt/Stop Button`) | interrupts |
| `help/chats.md` | +2 / −1 | `2b5e8ee24` | — | interrupts |
| `help/custom-tools.md` | **+205 / −10** | `322a5488e`, `a53024d3c`, `c56fe278c`, `e4a60ad9e`, `cfa0cba29`, `ea2cb60ab`, `e1872c94f` | `### Equipping, setting down and picking up`; `### Writing to the log`; `## Letting a character write one — \`edit_custom_tool\`` | SEVEN lanes' hunks in one page |
| `help/document-editing-tools.md` | +3 / −1 | `7263105c6` | — | bug 256 |
| `help/general-scenarios.md` | +2 / −0 | `1d0a31132` | — | scenario sort |
| `help/image-generation-profiles.md` | +1 / −1 | `406dbdd5a` | — | moderated-image rule |
| `help/pascals-workbench.md` | +14 / −11 | `322a5488e`, `a53024d3c`, `e4a60ad9e`, `cfa0cba29`, `e1872c94f`, `4fbdad186` | — | six lanes' hunks |
| `help/photo-gallery.md` | +10 / −2 | `680b71044` | `## Whose cabinet is whose`; `## Choosing an album` | /photos |
| `help/project-characters.md` | +16 / −8 | `102a23648` | `### A character cannot find an object on the project's shelf` | Inventory (roster shelf gate) |
| `help/project-wardrobe.md` | +1 / −0 | `4fbdad186` | — | Inventory |
| `help/projects.md` | +5 / −4 | `102a23648`, `4fbdad186` | `### A character can't reach the project files, wardrobe or inventory` (replaces `… files or wardrobe`) | Inventory |
| `help/scenario-browser.md` | +1 / −0 | `1d0a31132` | — | scenario sort |
| `help/system-llm-logs.md` | +1 / −1 | `e4a60ad9e` | — | the Log |
| `help/the-concierge.md` | +16 / −8 | `406dbdd5a` | `## What Gets Cleaned Up` | moderated-image rule |
| `help/tools-settings.md` | +2 / −0 | `e4a60ad9e` | — | the Log |
| `help/wardrobe.md` | +1 / −0 | `4fbdad186` | — | Inventory |

### A.2 `url:` front matter — the context resolver is unaffected

Three of the four new pages share an existing `url:` with pages that sort
BEFORE them in `readdirSync` order, and v5's resolver is "first match wins"
in that order (`crates/quilltap-host/build.rs` header; `files_store.rs`
`find_markdown_files`): `/salon/:id` is first matched by `agent-mode.md`
(then … `interrupting.md`, …, `the-log.md`); `/salon` by `ariel.md` (then
… `inventory.md`); `/prospero/:id` by `project-characters.md` (then …
`project-inventory.md`). The Guide LIST order and `help_docs` rowid order DO
change (four new rows interleaved alphabetically) — that is what
`help_tree_equivalence` re-measures at the pin.

### A.3 Where v5's copy lives and how it is guarded

- **The tree:** top-level `help/` (131 files), byte-equal to
  `git show e8e17b2d9:help/<p>` (P4.D323 vendored it whole).
- **The embed:** `crates/quilltap-host/build.rs` walks `<repo>/help/**/*.md`
  in `readdirSync` order (`strcmp` on name bytes, subdirectories inlined) and
  writes `help_embedded.rs` into `OUT_DIR`; `crates/quilltap-host/src/
  help_content.rs:16` `include!`s it as `EMBEDDED_HELP`, and
  `:23 embedded_help_count()` DERIVES the count ("the only literal lives in
  `help_tree_embed_guard`").
- **Guards (the four that move 131 → 135):**
  1. `crates/quilltap-harness/tests/help_tree_embed_guard.rs` — the doc
     comment is the running history (124 → … → 131); the literal
     `const VENDORED_FILE_COUNT: usize = 131;` (the last line of that
     comment block) is asserted against BOTH the embedded table and the
     on-disk walk (`embedded_table_equals_the_on_disk_help_tree`), then path
     set + ORDER + bytes. There is no per-file md5 table; pin equality is
     established at vendoring by `cmp` against `git show`.
  2. `crates/quilltap-host/tests/host_help_docs_boot.rs:126`
     `assert_eq!(expected, 131, "the vendored tree at v4 01a83539d");` — the
     assert MESSAGE is stale (P4.D323's recorded OPEN smalls item); fix the
     text when moving the number.
  3. `crates/quilltap-harness/tests/help_tree_equivalence.rs` — regenerated
     from the pin by `npx jest … harness/oracle/cases/help-tree-sync.test.ts`
     (recipe `:24-32`, `QT_ORACLE_HELP_TREE`).
  4. `crates/quilltap-harness/tests/help_section_size_equivalence.rs` —
     regenerated by `npx tsx harness/oracle/cases/help-section-size.ts`
     (`:24-28`, `QT_ORACLE_HELP_SECTION_SIZE`); the four new pages add
     ~24 H2/H3 sections.
- **Readers that DERIVE the count (no edit):**
  `crates/quilltap-web/tests/help_web_routes.rs:97` ("The count is read from
  the embedded table rather than transcribed"),
  `crates/quilltap-harness/tests/help_doc_ensure_equivalence.rs` (counts
  `help_docs` / `help_doc_chunks` against the embedded table).
- The SPA hard-codes no help slug list (`ggrep -rn "'chat-gallery'"
  apps/web/src` is empty outside specs).

### A.4 The rule, confirmed from P4.D323

`p4.d323-*.md:217-224` **R-1 — `help/` WHOLE, from the object store, nothing
shared:** every byte from `git show <pin>:help/<p>.md`, never from the
working tree; "no sibling lane patches `help/`". The eight sibling orders
carried `HANDOFF:` sentences naming their help prose for the unifier's
byte-compare only (`p4.d318-*.md:934` "No help / `docs/v4/` sentence —
`help/wardrobe.md`'s History-tab prose is P4.D323's"). This round's
`custom-tools.md` (seven commits) and `pascals-workbench.md` (six) make the
rule load-bearing: **the vendor lane copies all 135 files at `322a5488e`
in ONE commit** (`for p in $(git -C ~/source/quilltap-server ls-tree -r
--name-only 322a5488e -- help); do git … show 322a5488e:$p > $p; done`),
moves the two literals to 135, regenerates the two equivalence families
from the pin, and records per-page which content lane's prose each hunk
is (the table in A.1) so each content lane's `HANDOFF:` can cite it.
**The live v4 tree is AT the pin** (clean at `322a5488e`), so this round the
working tree and the object store agree — still use `git show`, the rule
exists for the next round.

---

## §B — `docs/` → v5 `docs/v4/`

### B.1 Counts

- v4 `docs/`: **495 at `e8e17b2d9`, 501 at `322a5488e`** (`git ls-tree -r`).
- v5 `docs/v4/`: **494 tracked** = 495 − the two deferred Inventory specs +
  the one v5-only file `docs/v4/packages-quilltap-README.md` (v4's
  `packages/quilltap/README.md`, UNCHANGED in the span — `git diff --stat
  e8e17b2d9 322a5488e -- packages/quilltap/README.md` is empty).
- After this round: 494 + 8 = **502** (re-measure with
  `git ls-files docs/v4 | wc -l`). With the two deferrals landing at their
  final paths, the mirror becomes a FULL copy of the pin's `docs/` again
  (501) plus the CLI README.
- **Spaced filename (memory `v4-path-quoting-traps`):** v4 `docs/` has
  exactly one — `docs/developer/features/complete/Wardrobe UX Overhaul.md`
  — unchanged in the span; the vendor lane's loop must quote paths
  (`while IFS= read -r p`), or the `for` word-split shows it as three
  "EXTRA" files, as the survey's own first pass did.
- Every current v5 `docs/v4/` `M`-path is byte-equal to `git show
  e8e17b2d9:docs/<p>` today (P4.D323's census; not re-run here).

### B.2 The operations (`git diff --name-status -M --numstat`)

| v4 path (status) | +/− (lines at pin) | Commits | v5 twin today | Op |
|---|---|---|---|---|
| `docs/CHANGELOG.md` (M) | +245 / −0 | all 22 | present | overwrite |
| `docs/FEATURES.md` (M) | +2 / −0 | `4fbdad186` | present | overwrite |
| `docs/developer/API.md` (M) | +86 / −5 | `2b5e8ee24`, `e4a60ad9e`, `680b71044`, `e1872c94f`, `4fbdad186` | present | overwrite |
| `docs/developer/BACKGROUND_JOBS_CHILD.md` (M) | +8 / −2 | `e4a60ad9e`, `e1872c94f` (the `logEntries` buffered-write override + the Inventory lock prose; the code halves are the Log lane's `child-repositories-proxy.ts` + the Inventory's) | present | overwrite |
| `docs/developer/CUSTOM_TOOL_SPEC.json` (M) | +20 / −1 | `a53024d3c`, `e4a60ad9e`, `e1872c94f` | present | overwrite — **also a RUNTIME artifact at the pin** (§C.3) |
| `docs/developer/DDL.md` (M) | +33 / −2 | `e4a60ad9e`, `a6292f169`, `e1872c94f`, `4fbdad186` (`characters.canManageInventory`, `chats.equippedInventory`, `terminal_sessions.cols/rows`, the `pascalMeta` Inventory-effect sentence, NEW `### log_entries` + the LLM-Logs table row) | present | overwrite |
| `docs/developer/ICON_INVENTORY.md` (M) | +3 / −0 | `2b5e8ee24`, `e27fb6b34` | present | overwrite |
| `docs/developer/PROMPT_ARCHITECTURE.md` (M) | +1 / −1 | `102a23648` | present | overwrite |
| `docs/developer/bugs.md` (M) | +7 / −2 | `322a5488e`, `7263105c6`, `a6292f169`, `102a23648` (rows 253–257) | present | overwrite |
| `docs/developer/bugs/fixed/bug-253-inventory-section-skipped-chained.md` (A) | 63 | `102a23648` | absent | add |
| `docs/developer/bugs/fixed/bug-254-progressions-skipped-chained.md` (A) | 65 | `102a23648` | absent | add |
| `docs/developer/bugs/fixed/bug-255-terminal-read-tui-garbled.md` (A) | 69 | `a6292f169` | absent | add |
| `docs/developer/bugs/fixed/bug-256-self-address-wrong-vault.md` (A) | 88 | `7263105c6` | absent | add |
| `docs/developer/bugs/fixed/bug-257-llm-consult-blind-to-verdict.md` (A) | 256 | `322a5488e` | absent | add |
| `docs/developer/features/ROADMAP.md` (M) | +3 / −2 | `a53024d3c`, `cc57e6cbc`, `e1872c94f`, `4fbdad186` | present | overwrite |
| `docs/developer/features/complete/character-progressions.md` (M) | +2 / −2 | `102a23648` | present | overwrite |
| `docs/developer/features/inventory-tools-and-pascal.md` → `complete/inventory-tools-and-pascal.md` (R074; +62 / −5 vs the baseline's pre-move file) | 253 at the pin | `a53024d3c`, `7263105c6`, `102a23648`, `e1872c94f` | absent (DEFERRED by P4.D323 R-Q) | **add at the final path** — the deferral closes |
| `docs/developer/features/inventory.md` → `complete/inventory.md` (R088; +22 / −3) | 299 at the pin | `102a23648`, `e1872c94f`, `4fbdad186` | absent (DEFERRED) | **add at the final path** — the deferral closes |
| `docs/developer/features/complete/pascal-custom-tools.md` (M) | +21 / −6 | `a53024d3c`, `cfa0cba29` | present | overwrite |
| `docs/developer/features/complete/pascal-gate-broadening.md` (A; born `features/pascal-gate-broadening.md` in `cc57e6cbc` +209, moved + edited +41 / −… in `a53024d3c`) | 226 at the pin | `a53024d3c`, `cc57e6cbc` | absent | add at the FINAL path |

**12 overwrites + 8 adds = 20 operations.** Not in `docs/` and NOT
vendored (NO-PORT infra — §D): `README.md`, `CLAUDE.md`,
`.claude/commands/update-documentation.md`, `packages/theme-storybook/**`,
`themes/tools/**`. `.github/`, `docker/`, `scripts/`, `plugins/` do not
move in the span (`git diff --stat` empty).

---

## §C — The public schemas

### C.1 Sizes

| File | Baseline bytes, md5 | Pin bytes, md5 | Commits |
|---|---|---|---|
| `public/schemas/qtap-custom-tool.schema.json` | 44,021 `cc205f92e1a7cbf9a485b49e31e6d413` | **63,595 `670ade1d0accb312d58f6c6c034285ae`** | `e1872c94f` (+313: the `inventory` / `inventoryCount` gate subjects, `{{inventory.*}}`, the `$inventory` roll form, the `inventory` effect), `e4a60ad9e` (+52: the `log` effect), `a53024d3c` (+274 / −100: any-of arrays on gates / `when` / effect conditions, `inventoryAggregate`, the reworded describe texts), `322a5488e` (+8 / −2: `brief`, the `llm.prompt` guidance) |
| `public/schemas/qtap-export.schema.json` | 108,678 `1d8e6546fbdb6ad3a48fee15d2932911` | **109,559 `10b72fd1160722b77a49fe6f7abb5b79`** | `4fbdad186` (+8: `equippedInventory` on the chat def), `e1872c94f` (+6 / −2: `canManageInventory` on the character def; the `effects` description gains the Inventory sentence) |

### C.2 The export schema hunks (verbatim, the whole diff is 14 lines)

- Character def, after `canCreateOutfits`:
  `"canManageInventory": { "type": ["boolean", "null"], "description": "Whether this character may see and change what they carry with the inventory_* tools (null = enabled by default)" }`
- Chat def, before `equippedOutfit`:
  `"equippedInventory": { "type": ["object", "null"], "description": "Inventory objects each character carries in this chat, keyed by characterId: an array of Inventory object UUIDs (the \`id\` inside each \`Inventory/**/*.json\` file). Ids that no longer resolve are dropped on read.", "additionalProperties": { "type": "array", "items": { "$ref": "#/$defs/UUID" } } }`
- `pascalMeta.effects.description` gains: "An Inventory change is targeted
  'inventory.equip' / 'inventory.unequip' / 'inventory.pick_up', with
  `previous` the character's equipped object ids before it and `next` =
  { item: { id, title }, created?, equipped }."
- **Acceptance consequence to MEASURE at port time:** whether the chat and
  character `$defs` are `additionalProperties: true` (P4.D294 / P4.D313
  recorded `ExportedChat` and `Memory` as open; P4.D317 recorded the chats
  DATA object as closed). If the character def is open, both moves tighten a
  PRESENT key only (a non-boolean `canManageInventory`, a non-UUID
  `equippedInventory` id); if closed, a `.qtap` bundle written by v4 at the
  pin is REFUSED by v5's validator today — the live-data class P4.D313
  closed for `invokedBy: "trigger"`. Either way the re-vendor belongs to the
  Inventory carriers lane (the `.qtap` reader must accept what the schema
  accepts).

### C.3 Where v5 vendors them and the guards

| v4 file | v5 copy | Guards / constants that move |
|---|---|---|
| `qtap-custom-tool.schema.json` | `apps/web/public/schemas/qtap-custom-tool.schema.json` (md5 = the baseline's; SERVED by the SPA — `apps/web/src/app/pascal/tool-draft.ts` writes its `$schema` URL into every draft). **No Rust copy**: `crates/quilltap-core/src/pascal/custom_tool_types.rs:9` names it in a comment only. | `crates/quilltap-harness/tests/public_schemas_vendor_guard.rs:53` `bytes: 44_021` → `63_595` (the doc comment at `:44-48` is the running history; `the_vendored_schemas_equal_the_v4_checkouts` diffs against the checkout). |
| `qtap-export.schema.json` | `crates/quilltap-core/src/generators/qtap-export.schema.json` (`include_str!` at `qtap_schema.rs:59`; md5 = the baseline's) | `crates/quilltap-core/src/generators/qtap_schema.rs:200` `assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(), 108_678)` → `109_559`; `crates/quilltap-harness/tests/qtap_schema_embed_guard.rs:76` `const VENDORED_BYTES: usize = 108_678;` → `109_559` (+ a P4.D3xx history line after the P4.D317 one); `qtap_schema_validate_equivalence.rs` rows for `equippedInventory` / `canManageInventory` (red-first). |
| `qtap-progression.schema.json` | unchanged (5,963) | — |

**NEW at the pin — the custom-tool schema is an ENGINE input, not only an
SPA asset.** `lib/tools/handlers/edit-custom-tool-handler.ts:44-46`:

```ts
import customToolJsonSchema from '@/public/schemas/qtap-custom-tool.schema.json';
import rangeSpecimen from '@/docs/developer/CUSTOM_TOOL_SPEC.json';
import diceSpecimen from '@/docs/developer/CUSTOM_TOOL_SPEC_DICE.json';
```

and `:113` answers `action: 'schema'` with
`{ success: true, action: 'schema', schema: customToolJsonSchema, examples: [rangeSpecimen, diceSpecimen] }`
(`c56fe278c`). v5 has no embed of any of the three: `ggrep -rn
CUSTOM_TOOL_SPEC crates apps/web/src` is empty outside `docs/v4/`. The
`edit_custom_tool` lane therefore needs a Rust `include_str!` of all three
at the PIN bytes. **Ruling owed (§H.1):** embed from `docs/v4/developer/`
+ `apps/web/public/schemas/` directly (making two mirror paths build
inputs — the `<repo>/help` precedent) or keep crate-local copies under
`crates/quilltap-core/src/pascal/` with a byte guard against both mirrors
(the `qtap-export.schema.json` precedent). `CUSTOM_TOOL_SPEC_DICE.json` is
UNCHANGED in the span (not in the diff) and already in `docs/v4/`.

### C.4 Ownership recommendation

The previous round (P4.D323 R-E) gave BOTH public schemas to the content
lanes that port their acceptance (P4.D317 / P4.D319) and had the vendor lane
re-run the two guards on the union as a CHECK. Keep that shape:

- `qtap-export.schema.json` + `qtap_schema.rs:200` + `VENDORED_BYTES` → the
  **Inventory carriers** lane (it owns the `.qtap` reader for
  `equippedInventory` / `canManageInventory`).
- `qtap-custom-tool.schema.json` + `public_schemas_vendor_guard.rs:53` → ONE
  owner by the final-writer rule: the **gates + bug-257** lane
  (`a53024d3c` + `322a5488e` are the last two writers; the Inventory-tools
  and Log lanes' hunks ride the same file and cite the vendored bytes in
  their `HANDOFF:`), with the `edit_custom_tool` lane CONSUMING the embed
  through a Shared contract (`pub const QTAP_CUSTOM_TOOL_SCHEMA_JSON: &str`,
  `CUSTOM_TOOL_SPEC_JSON`, `CUSTOM_TOOL_SPEC_DICE_JSON`).
- The vendor lane copies the three `docs/v4/` bytes (B.2) and re-runs the
  guards on the union only.

---

## §D — NO-PORT candidates with evidence

### D.1 `cad017599` — "Test: inventory-gated custom tool end to end (Measure Twice)"

`git show --stat --format= cad017599`: **2 files, +232 / −0**:
`__tests__/unit/lib/pascal/custom-tools-measure-twice.test.ts` (+228) and
`docs/CHANGELOG.md` (+4). No `lib/ app/ components/ help/ public/
migrations/` file; no version stamp. The test mocks eleven modules and
drives the REAL `QtapCustomToolSchema.parse`, `loadInventoryContext`,
`evaluateToolGate`, `executeCustomTool` over `__tests__/helpers/
inventory-fake-store` (seven cases, `:108-199`: a title-keyed `eq:
"equipped"` gate opening for the carrier and staying closed for a different
instance of the same archetype; `inventoryCount lt: 1` with no context; the
apostrophed title `"Friday's Yellow Tape Measure"` and a metadata value
rendering through `{{inventory.<ref>.<field>}}`; kind-name matching with the
name fallback; two devices / two surfaces). **Production-invisible.**
Ratify NO-PORT; record it (as the ledger row already does) as a candidate
tier-3 MIRROR corpus for the Inventory-grammar lane's
`pascal_run_custom_equivalence` rows — that lane's Tier 2, not the vendor
lane's.

### D.2 `cc57e6cbc` — "Docs: spec for broader custom-tool gates"

**3 files, +214 / −0**: `docs/CHANGELOG.md` (+4), `docs/developer/
features/ROADMAP.md` (+1), NEW `docs/developer/features/
pascal-gate-broadening.md` (+209). Zero production files, no stamp. The
spec is moved to `complete/` and edited (+41 / −…) by `a53024d3c`; it
reaches v5 at its FINAL path through B.2. Ratify NO-PORT.

### D.3 `packages/theme-storybook/**` (by precedent — `drift-ledger.md:619,1566`)

`git diff --stat e8e17b2d9 322a5488e -- packages/theme-storybook/`: 6 files
+133 / −5 — `CHANGELOG.md` (+11: entries 1.0.75 `inventory` icon, 1.0.76
`stop-sign` / `hand-raised` + the composer action-button family),
`package.json` 1.0.74 → **1.0.77**, `package-lock.json` 1.0.74 → **1.0.76**
(the lock LAGS the manifest by one), `src/css/qt-components.css` (+83: the
storybook copy of `_chat.css`'s `.qt-chat-composer-actions`,
`.qt-chat-composer-send`, `.qt-chat-continue-button`, `.qt-chat-stop-button`,
`.qt-chat-interrupt-button`, `.qt-chat-interrupt-button-raised`),
`src/stories/components/Chat.tsx` (+34), `src/stories/components/Icons.tsx`
(+2 / −2: `'stop-sign', 'hand-raised'` in Media, `'inventory'` in People).
The storybook is a theme-author preview of the app's CSS; the CSS of record
is `app/styles/qt-components/_chat.css` (the interrupts lane's). NO-PORT.

### D.4 `.claude/commands/update-documentation.md` (+13 / −… in 5 commits)

v4's own slash-command checklist: five NEW bullets (`help/the-log.md`,
`help/inventory.md`, `features/complete/inventory-tools-and-pascal.md`,
`help/project-inventory.md`, `features/complete/inventory.md`) and four
rewritten descriptions (`the-concierge.md`, `custom-tools.md`,
`pascals-workbench.md`, `CUSTOM_TOOL_SPEC.json`). NO-PORT (v5 has its own
`/` commands).

### D.5 `CLAUDE.md` (+7 / −…) and `README.md` (+4 / −4)

- v4 `CLAUDE.md`: the Staff table's Pascal row gains "; the **Inventory**
  (objects characters carry, per-tier `Inventory/` JSON files, per-chat
  equipped set; the nine `inventory_*` tools gated by `canManageInventory`;
  custom tools' `inventory` / `inventoryCount` subjects, `{{inventory.*}}`,
  `$inventory` and `inventory` effects) — code in `lib/inventory/`". v4's
  own agent guidance; its substance is the Inventory lanes' code.
- `README.md:11` the badge `version-4.11.0--dev.26` → `--dev.53` (the net of
  twenty stamped commits), and `:164` the Pascal paragraph gains the sentence
  "He also keeps the **Inventory**: the things characters carry, on four
  shelves, with archetypes, kits, pictures, and a per-chat record of what
  each character has in hand — told to the model while carried and drawn
  into their pictures." v5's README is its own. NO-PORT.

### D.6 `themes/tools/madmans-box-icon-preview.html` (+6 / −2) — and the theme files that are NOT NO-PORT

The preview tool's canonical list moves 79 → 86 names (`'stop-sign',
'hand-raised'`, `'mail'`, `'inventory'`, `'thinking'`, `'brahma-console'` —
three of them were already missing from the tool). A theme-author
dev tool; NO-PORT.

**But the bundled theme IS vendored by v5:**
`apps/web/public/themes/madmans-box/theme.json` is a byte copy of v4's
`themes/bundled/madmans-box/theme.json` at the baseline (md5
`8c196de10329fc400af97141bf941f89` both sides), and the span moves it by
THREE lines from TWO commits — `"stop-sign": "icons/stop-sign.svg"`,
`"hand-raised": "icons/hand-raised.svg"` (`2b5e8ee24`) and `"inventory":
"icons/inventory.svg"` (`e27fb6b34`) — plus three NEW
`themes/bundled/madmans-box/icons/{hand-raised,stop-sign,inventory}.svg`.
v5's theme directory carries `fonts/ icons/ styles.css textures/ theme.json
tokens.json` with `icons/` holding ONLY `brand.svg` (v4's theme ships 122
files), so how v5 resolves the other icon overrides is a question for the
SPA lanes (`apps/web/src/app/theme/theme.service.ts:221,248` loads
`styles.css` and fonts; no icon-override loader was found by grep).
Separately, `public/images/icons/` is 84 at the baseline = v5's
`apps/web/public/images/icons/` (84), and 87 at the pin (+`inventory.svg`,
`hand-raised.svg`, `stop-sign.svg`); the SPA's icon union
`apps/web/src/app/ui/icon.ts:14-30` lacks all three names.

**Ownership proposal (disjoint):** the SPA icons (`apps/web/public/images/
icons/<n>.svg` + the `icon.ts` union entry + the registry) go to the lane
that uses them — `hand-raised` / `stop-sign` to the interrupts SPA lane,
`inventory` to the Inventory SPA lane; the ONE shared file
`apps/web/public/themes/madmans-box/theme.json` (+3 lines) and the three
theme-override SVGs go to the **vendor lane** as byte copies at the pin, so
no two lanes touch `theme.json`.

### D.7 The docs / infra halves of code commits (ratify with the rows)

`docs/CHANGELOG.md` (all 22), the README badge (20 stamped commits),
`ROADMAP.md` (4), `ICON_INVENTORY.md` (2), `FEATURES.md` (1), every
`__tests__/**` file and `__snapshots__/*.snap` (v4's own tests — the
ledger's standing rule), the `test:unit` script's `NODE_ENV=test`
(`package.json:51`; no v5 recipe runs `npm run test:unit`), and
`jest.setup.ts`'s `delete process.env.QUILLTAP_TIMEZONE` (§E.6 — harmless to
every recipe).

---

## §E — The IMPORT-DEAD + RUNTIME-BREAK census

### E.0 Verdict

- **IMPORT-DEAD names: 0.** Every one of the 37 removed `export` lines is
  alive at the pin under its old name in its old module (E.1).
- **Dead import specifiers: 0.** `git diff --name-status -M --diff-filter=DRC
  e8e17b2d9 322a5488e -- lib app packages plugins migrations components` is
  EMPTY (no deleted, renamed or copied file).
- **Positional-signature breaks on oracle-called exports: 0** (E.3).
- **Behaviour-drift reds** (a family goes red because v4 changed what it
  does, not because the oracle stopped running): 11 families, each owned by
  a content lane (E.4).
- **Runtime consequences no single lane owns:** ONE stacking constraint
  (E.5.1, the arrival write), two D23 tripwires (E.5.3), one lazily-created
  table (E.5.2), one dependency (E.5.4). **No stage-0 `REAIM` lane** — the
  P4.D314 precedent ("make the oracle side LOAD at the pin") has nothing to
  re-aim.

### E.1 Every removed `export` line, with what stands at the pin

`git diff e8e17b2d9 322a5488e -- lib app packages plugins migrations
components | grep '^-\s*export'` (file-attributed with `gawk`):

| Removed (baseline) | At the pin | Effect |
|---|---|---|
| `app/api/v1/photos/route.ts` `export const GET = createContextHandler(async …` | `:86 export const GET = createContextHandler(` — now `withCollectionActionDispatch({ albums: handleListAlbums }, handleListGallery)` (`680b71044`) | Same name; BEHAVIOUR drift (E.4 #7) |
| `lib/background-jobs/host/write-partition.ts` `LLM_LOGS_REPO_KEYS = new Set(['llmLogs'])` | `:70 … new Set(['llmLogs', 'logEntries'])` | Widened; `cases/write-partition.ts` imports other names (zero hits for `LLM_LOGS_REPO_KEYS` / `logEntries` in the case) — survives |
| `vault-overlay/wardrobe-writes.ts` `WardrobeMountScope`, `WardrobeMount` (type / interface) | `:57 export type WardrobeMountScope = VaultFolderMountScope;` `:60 export type WardrobeMount = VaultFolderMount;` | Types; the Rust `WardrobeMount` in `wardrobe_folder_writes_tier2_equivalence.rs:49` is v5's own |
| `wardrobe-writes.ts` `async function resolveWardrobeMount(` | `:73 export function resolveWardrobeMount(characterId): Promise<WardrobeMount \| null> { return resolveVaultFolderMount(characterId); }` | `async` dropped, still returns a Promise; `await` at `wardrobe-folder-writes-tier2.ts:130` and in 7 builders (`build-chat-create-capstone.ts:429`, `build-wardrobe-item-images-fixture.ts:278`, `build-wardrobe-transfers-fixture.ts:346,359`, `build-project-roster-access-fixture.ts:274`, `build-groups-projects-fixture.ts:577`, `build-wearable-pool-fixture.ts:298`) survives |
| `wardrobe-writes.ts` `listFoldersInMount`, `listFolderPathsInMount`, `createFolderInMount`, `renameFolderInMount`, `deleteFolderInMount` | `:325-357` — thin delegates over `item-folder-ops.ts` (`listFoldersIn`, `createFolderIn`, …) with `WARDROBE_FOLDER_RULES` | Same names and arities; `wardrobe-folder-writes-tier2.ts:199-264` survives (the refusal MESSAGES are byte-equal — next row) |
| `lib/wardrobe/folders.ts` `WardrobeFolderPathError`, `WardrobeFolderNotFoundError`, `WardrobeFolderConflictError`, `WardrobeItemNotInLocationError` (classes) | `:66-101` re-declared as subclasses of `ItemFolder*Error` (`lib/mount-index/item-folders.ts:35-68`) passing `noun = 'wardrobe'`: `` `${capitalise(noun)} folder not found: ${folder}` `` = `Wardrobe folder not found: …`; `` `A ${noun} folder named "${folder}" already exists` ``; `` `Not in this ${noun} (shared items are filed where they live): …` `` — **byte-equal to the baseline's `:41,52,63`**; `.name` re-set to the Wardrobe names | `cases/wardrobe-folders.ts:48-50,89-95` and `wardrobe_folders_equivalence.rs:42 PATH_ERROR_NAME` survive |
| `folders.ts` `interface WardrobeFolder` | `:104 export type WardrobeFolder = ItemFolder;` | Type |
| `folders.ts` `folderSegments`, `parentFolder`, `folderName`, `joinFolder`, `isSameFolder`, `isUnderFolder`, `rebaseFolder`, `folderCrumb` | Moved to `lib/mount-index/item-folders.ts:89-306` AND **re-exported from `lib/wardrobe/folders.ts:37-46`** | The STATIC import at `cases/wardrobe-folders.ts:31-52` resolves every name — survives |
| `lib/doc-edit/uri-producers.ts` `buildDocStoreUriResolver(characterId?)` | `:99 export async function buildDocStoreUriResolver(): Promise<{…}>` — the parameter is GONE (bug 256) | `cases/doc-edit-path-resolver.ts:229 buildDocStoreUriResolver(A)` passes an ignored argument; the `res-self` row's output flips (E.4 #9) |
| `lib/file-storage/wardrobe-image-bridge.ts` `wardrobeItemImageFolder(itemId)`, `wardrobeItemImagePath(itemId, leaf)` | `:59,64` gain a trailing `imagesFolder = WARDROBE_IMAGES_FOLDER` default | Additive; no oracle importer |
| `lib/pascal/custom-tool.types.ts` `RollSchema`, `ToolGateSchema`, `WhenSchema` | `:318` `z.union([RollDiceSchema, RollRangeSchema, InventoryRefSchema])`; `:647` the group-or-array union; `:766` `true \| object \| array` | Same names; the eight `custom-tool.types` importers (`pascal-bench-wardrobe.ts:32`, `pascal-custom-tool-definition.ts:76`, `pascal-llm-consult.ts:33`, `pascal-tool-vocabulary.ts:29`, `pascal-custom-tools-discovery.test.ts:66`, `pascal-run-custom.ts:19`, `pascal-custom-tools-execution.test.ts:61`, `pascal-simulate.ts:18`) load; the definition / vocabulary families drift (E.4 #3) |
| `lib/pascal/tool-draft.ts` `conditionsFromWhen(when: WhenObject)`, `whenFromConditions(…): WhenObject \| undefined` | `:528` accepts `WhenObject \| WhenObject[]`; `:1028` returns `WhenObject \| WhenObject[] \| undefined` | No oracle importer; the SPA twins `apps/web/src/app/pascal/tool-draft.ts:477,849` are the gates SPA lane's |
| `lib/schemas/llm-log.types.ts` `IMAGE_SPEND_LOG_TYPES` | `:52` + `'INVENTORY_ITEM_IMAGE'` | `cases/task-type-log-mapping.ts:77` builds `admitted` from `LLMLogTypeEnum.options` and emits `admitted: boolean` per probe — the set only widens, every existing probe's answer holds; the Rust enum twin must gain the member (the Inventory images lane) |
| `lib/wardrobe/outfit-change-effects.ts` `async function refreshAvatarForOutfit(repos, context, sourceContext)` | `:104 export const refreshAvatarForOutfit = refreshAvatarForAppearance;` — `:90` the SAME three-parameter signature | `cases/outfit-change-effects-tier3.ts:261` survives |
| The five interfaces `TextBlockEnabledToolOptions`, `TextBlockPromptOptions`, `BuildToolsOptions`, `SimpleJsonPromptOptions`, `WardrobeToolContext`-class | Re-declared with new fields (`inventory` / `log` flags) | Types, erased by tsx |

**Barrel / specifier census:** every `@/…` specifier imported anywhere under
`harness/oracle/**` and `crates/**/tests/**` (664 distinct) was intersected
with the 283 production modules the span touched: **77 modules** are both
imported by the oracle side and changed in the span (the list is in
`/private/tmp/claude-503/-Volumes-External-1-source-overflow-quilltap-v5/a23364e7-8003-4790-ae8f-69a136985640/scratchpad/oracle-specifiers.txt`
∩ `span-modules.txt`; the behaviour-relevant ones are E.4). None is missing
at the pin.

### E.2 Type fields deleted from exported interfaces

| Deleted field | Oracle hits | Runtime effect |
|---|---|---|
| `RosterContext.characterIds` (`lib/pascal/custom-tools.ts`, `cfa0cba29`) | `cases/pascal-custom-tools-discovery.test.ts` builds a `RosterContext` (`:105` "Passed on the RosterContext — v4's truthy short-circuit") — it passes `metadata` / `characterId`, not `characterIds` | None at load; `resolveCustomToolRoster` no longer deals co-participants' vault tools (E.4 #4) |
| `RunCustomToolContext.characterIds` (`run-custom-handler.ts`) | `cases/pascal-run-custom-handler.test.ts:62,349` and the 30 corpus rows at `:464-550` each pass `characterIds: ALL` | An extra property on a plain object — IGNORED at runtime. Rows whose tool lives in another participant's vault (the `gated-*` / `vault-*` groups) flip (E.4 #4) |
| `RunTriggeredCustomToolsParams.characterIds` (`trigger-runner.ts`) | `cases/pascal-trigger-runner.test.ts:240` passes no `characterIds` (zero grep hits) | None |
| `buildDocStoreUriResolver`'s `characterId`, `docStoreUriFor`'s `{ characterId }` context (`uri-producers.ts`, bug 256) | `cases/doc-edit-path-resolver.ts:225-233` | Ignored at runtime; output flips (E.4 #9) |
| `OutfitChangeContext` / `WardrobeTurnContainers` gain `pendingInventoryAnnouncements` (optional) | the four `newWardrobeTurnContainers()` callers (`wardrobe-tools.ts:190`, `wardrobe-tools-avatar-trigger.ts:93`, `pascal-wardrobe-effects.ts:132`, `outfit-change-effects-tier3.ts:168`) | `newWardrobeTurnContainers()` at the pin mints the new container itself — survives |

### E.3 Exported-function signatures whose PARAMETER lines changed (`-U0` scan)

Every `export (async )?function` whose multi-line header carries a `-`/`+`
line in the span: `handleStopTurn`, `handleRaiseHand`,
`handleInventoryEquip`, `handleInventoryResetDefaults`,
`planInventoryImagePointerFixes`, `stopActiveTurns`, `detectYieldSentinel`,
`compileSqlRegexp`, `parseInventoryItemFile`, `updateInventoryManyInMount`,
`deleteInventoryInMount`, `renameInventoryFolderInMount`,
`deleteInventoryFolderInMount`, `resolveVaultFolderMount`,
`parseScenarioFilesSorted`, `wardrobeItemImagePath` (a trailing default),
`sceneDescriptionParts`, `applyInventoryDefaults`, `resetInventoryToDefaults`,
`inventoryActionUrl`, `loadInventoryContext`, `isReachableFromKitTier`,
`findKitsStrandedByMove`, `notifyInventoryChanged`,
`recordPendingInventoryChange`, `strikeFromPendingInventoryChange`,
`flushPendingInventoryAnnouncements`, `dropCarriedEverywhere`,
`equipItemsHoldingLock`, and `resolveWardrobeMount` / `renameFolderInMount` /
`deleteFolderInMount` (`async` dropped only). **All but the last three are
NEW functions**; `wardrobeItemImagePath` is additive. **No existing export
the oracle calls changed its positional contract** — unlike the previous
round's `recordPendingWardrobeAnnouncement(repos, …)`.

Also checked: `sanitizeAppearancesIfNeeded` (`406dbdd5a`, recorded in the
ledger as a SIGNATURE CHANGE) — a module-internal call chain reached from
`image-gen/appearance-resolution` / `prompt-expansion`, which the oracle
imports; the families drift (E.4 #8) rather than break, because the oracle
calls the outer resolvers.

### E.4 Behaviour-drift reds at the pin (owned by content lanes)

| # | Family (`crates/quilltap-harness/tests/…`) | Oracle case | Why it flips at the pin | Owner |
|---|---|---|---|---|
| 1 | `tool_definitions*`, `tool_build`, pseudo-tool prompt / parser families, the four `streaming.service` importers | `tool-definitions{,-canonical}.ts`, `tool-build.test.ts`, … | `lib/tools/index.ts` + `tool-executor.ts` BUILT_IN_TOOLS gain `edit_custom_tool`, `log_create_entry`, `log_grep`, `log_read` and the nine `inventory_*` (59 → 63 → 72 on v4); `run_custom`'s category `utility` → `pascal` and its description grows (Inventory / log effects, `brief`) | the tools-catalog owners (Inventory tools, the Log, `edit_custom_tool`) |
| 2 | `write_partition_equivalence` | `write-partition.ts` | unchanged output today; extend with a `logEntries` op | the Log |
| 3 | `pascal_custom_tool_definition`, `pascal_tool_vocabulary`, `pascal_run_custom*`, `pascal_llm_consult`, `pascal_simulate`, `pascal_bench_wardrobe` | the eight `custom-tool.types` importers | any-of arrays, `inventoryAggregate`, `brief`, `log` / `inventory` effects, bug 257's consult order (`consultFirst`), the reworded `.describe` texts | gates + bug 257; the Log; Inventory grammar |
| 4 | `pascal_run_custom_handler_equivalence`, `pascal_build_tools_roster`, `pascal_custom_tools_discovery` | `pascal-run-custom-handler.test.ts`, `pascal-custom-tools-discovery.test.ts` | `TIER_ORDER` drops `participant`; `resolveCustomToolRoster` → `resolveTieredMountPool({userId, characterId, characterMountPointId, projectId})` with no `characterIds` | vault-only custom tools (`cfa0cba29`) |
| 5 | `terminal_tools_equivalence` | `terminal-tools.ts:90-91` (`handleTerminalRead` / `validateTerminalReadInput`) | `terminal_read` renders the transcript through `TerminalScreen` (`@xterm/headless`) at `session.cols ?? 80` × `rows ?? 24`; the input schema moves | bug 255 |
| 6 | `llm_log_cleanup_equivalence` | `llm-log-cleanup-jobs.test.ts:232` | see E.5.2 — SURVIVES today (table diff only); the Log lane extends it | the Log |
| 7 | `photos_routes_equivalence` | `photos-routes.test.ts:123,210` | `GET /photos` excludes character vaults (`isUserGalleryMount`), `?action=albums`, `album=` | /photos (`680b71044`) |
| 8 | `appearance_resolution*`, `prompt_expansion*`, `image_generation*`, the understudy / provider-routing families | `image-gen/*`, `dangerous-content/*` importers | `isUncensoredImageProfile`; the pre-screen arm removed | moderated-image rule (`406dbdd5a`) |
| 9 | `doc_edit_path_resolver_equivalence`, the Librarian / doc-edit announcement families | `doc-edit-path-resolver.ts:229` (`res-self`), `librarian-notifications/writer` importers | `qtap://self/…` → the vault's name / UUID; `scopeLabel` `self` → `a character's vault` | bug 256 |
| 10 | scenario list families (`build-scenario-location-fixture.ts:157`, `build-groups-projects-fixture.ts:368`, `build-chat-scenario-fixture.ts:323` use `buildScenarioFileContent` — UNCHANGED, 0 diff hits; the READ side `vault-readers.ts` / `read-overlay.ts` sorts) | scenario reads | `parseScenarioFilesSorted` + `sortScenarios` | scenario sort (`1d0a31132`) |
| 11 | `build_context_tier3`, the chat spine families (`orchestrator`, `context-builder`, `turn-manager/skip-signal`, `stream-watchdog`, `host-notifications/writer`) | `build-context-tier3.test.ts`, the tier-3 spine cases | the trailing `inventory` section (empty = absent — byte-identical with no Inventory; B1 of the previous survey proved it), `offerYield` / the `[YIELD]` sentinel, `matchesSentinelLine`, the Host's yield announcement | interrupts; Inventory keystone |

Plus `character_archive_tier2` (the `KEPT_FOLDERS` keep — ALREADY ported
by P4.D322 from its second pin; `archive-service.ts`'s only span commit is
`4fbdad186`, +19 / −13 — nothing new) and the restore families
(`uuid-remap.ts` `planInventoryImagePointerFixes`, the Inventory carriers
lane's).

### E.5 Runtime consequences no single lane owns

**E.5.1 — The arrival write (the ONE stacking constraint).** At the pin
`app/api/v1/chats/route.ts:1444` (`handleCreate`), `app/api/v1/chats/[id]/
actions/participants.ts:242` and `lib/chat/apply-chat-merge.ts:267` call
`applyInventoryDefaults(repos, chatId, characterIds, { source, userId,
sourceChatId })`. Per CHARACTER participant, `arriveUnderLock`
(`lib/inventory/apply-defaults.ts:115-150`) reads the chat, finds no entry,
runs `equipDefaults`, and **when nothing is equipped STILL writes**
`repos.chats.setEquippedInventory(chatId, characterId, [])` (`:146-148`,
"Record the arrival, so a later re-seat does not apply defaults again").
`setEquippedInventory` (`chats.repository.ts:802-831`) is a `safeQuery`
with fallback `false` and the error text `Failed to write equipped
inventory`, followed by DEBUG `Equipped inventory write` `{chatId,
characterId, count, cleared, written}`.

- **On a REBUILT fixture** (any builder that runs v4's `initializeDatabase`
  at the pin — `build-chat-create-capstone.ts` and every other
  `harness/oracle/fixtures/build-*.ts` → `generateDDL` renders the column):
  the write SUCCEEDS and the `chats` row gains
  `equippedInventory = '{"<charId>":[]}'` (one key per character
  participant, sorted). `cases/chat-create-capstone.test.ts` dumps the MAIN
  tables after each case (`:141` "The MAIN-db tables to dump", `:424`),
  so `chat_create_capstone_equivalence` REDS on every created chat until
  v5 (a) carries the column (the 13th D23 move) and (b) performs the arrival
  write. Same for any family that dumps `chats` after a participant add or a
  merge (no oracle case imports `actions/participants` or
  `apply-chat-merge` directly — zero grep hits — but the route-level cases
  that reach them, e.g. `chat-participants` / `chat-admin`, must be checked
  at regen).
- **On a COMMITTED fixture** (`salon-reads.test.ts` on
  `QT_FIXTURE_SALON_MAIN` = `crates/quilltap-web/tests/fixtures/
  salon-main.db`, `query-param-semantics.test.ts`): `UPDATE chats SET
  "equippedInventory" = ?` fails with `no such column`, `safeQuery` catches
  it and logs `Failed to write equipped inventory`, then the DEBUG line with
  `written: false`. Neither case captures logs (0 `transports` /
  `CAPTURED` refs in `salon-reads`, `query-param-semantics`;
  `chat-create-capstone` has one capture ref — verify at regen what it
  captures). The chat is created; the dump is unchanged. SURVIVES.
- **Consequence for the lane cut:** the Inventory keystone (the column +
  `ChatsRepository.setEquippedInventory` / `getEquippedInventory` /
  `normalizeEquippedInventory` + `applyInventoryDefaults` at the three
  sites) must be a **stage-0 / stage-1 lane that every chat-create,
  participant-add and merge regen stacks on**; lanes that touch only other
  families cut from `main`.

**E.5.2 — `log_entries` is created LAZILY by the first repository use.**
`LogEntriesRepository` (`lib/database/repositories/log-entries.repository.ts:39-40`)
is an `AbstractDedicatedDbRepository` with `dbTarget: 'llmLogs'`; its
`ensureTable` runs `generateDDL('log_entries', …)` on first use and
`onTableEnsured` adds `idx_log_entries_chatId` + `idx_log_entries_characterId`
(generateDDL emits `idx_log_entries_createdAt`). `handleLLMLogCleanup`
(`llm-log-cleanup.ts:53`) now calls `repos.logEntries.cleanupOldEntries(retentionDays)`
UNGUARDED before the `llmLogs` cleanup → on the oracle's per-run copy of
`llm-log-cleanup-llmlogs.db` the table is CREATED, 0 rows deleted, INFO
`Cleaned up old log entries` `{retentionDays, deletedCount: 0, cutoffDate}`
+ DEBUG `[LLMLogCleanup] Log entries pruned`. `llm_log_cleanup_equivalence.rs`
diffs `background_jobs` + `llm_logs` BY TABLE (`:15`) and captures no logs
→ **SURVIVES**; a `sqlite_master` dump would red (the Log lane's own
tripwire). `createBackup` / export / import / `delete-service` /
`cascade-delete` read NO `log_entries` (zero hits at the pin) → the whole
`createBackup` class that the previous round planted around is UNAFFECTED
this time.

**E.5.3 — Three new migrations, no new index.** `migrations/scripts/index.ts`
registers `addChatEquippedInventoryMigration`
(`add-chat-equipped-inventory-v1`, `dependsOn add-equipped-outfit-field-v1`,
`addColumnIfMissing('chats','equippedInventory','TEXT DEFAULT NULL')`, INFO
`Added equippedInventory column to chats` / ERROR `Failed to add
equippedInventory column to chats`), `addCharacterInventoryFlagMigration`
(`add-character-inventory-flag-v1`, `dependsOn
add-character-wardrobe-flags-v1`, `('characters','canManageInventory',
'INTEGER DEFAULT NULL')`, INFO `Added canManageInventory column to
characters`), `addTerminalSessionGeometryMigration`
(`add-terminal-session-geometry-v1`, `dependsOn
add-terminal-sessions-table-v1`, `cols` + `rows` `INTEGER DEFAULT NULL`,
INFO `Added geometry columns to terminal_sessions` `{added}` / ERROR
`Failed to add geometry columns to terminal_sessions`), each
`introducedInVersion '4.11.0'`. None creates an index →
`services/provisioning/migration_indexes.json` is unmoved by them. The
full-registry runners (`cases/migration-index-backfill.ts:265-266` items
9/10 "v4's runner on a backfilled v5 copy is a no-op",
`provision/migrations-first.ts`, `provision/dump-migration-indexes.ts`)
now ALTER three columns on a v5-provisioned copy → `provisioning_equivalence`
(1d, `QT_FRESH_SCHEMA_LIVE`) and `migration_index_backfill_equivalence`
go RED at the pin — **the D23 tripwire firing as designed (moves 13–15),
owned by the Inventory keystone's ONE `fresh_schema.json` re-dump + three
boot ensures**. `zero-rerun.ts:137` runs a NARROWED list and is unaffected
unless widened. **`log_entries` (move 16)** lives in the llm-logs partition
and is created lazily, so `dump-fresh-schema.ts` / `dump-migration-indexes.ts`
(which read `getRawLLMLogsDatabase`) will NOT see it unless the dump
touches `getRepositories().logEntries` first — the provisioning-shape
ruling the ledger already records as owed.

**E.5.4 — The dependency.** `terminal_read` at the pin constructs
`new Terminal({ cols, rows, scrollback: 5000, allowProposedApi: true })`
from `@xterm/headless` (`lib/terminal/screen.ts`); without the package in
the pin's `node_modules` the `terminal-tools.ts` regen dies at import. §F
measures it present in the cache and the live tree.

**E.5.5 — Committed fixture DBs and the three new columns.** All 109
tracked `.db` files (`git ls-files | grep '\.db$'` — every pair under
`crates/quilltap-web/tests/fixtures/`, incl. `migration-vintage/` and the
`*-llmlogs.db` partitions) predate `4fbdad186`, so EVERY committed `chats`
lacks `equippedInventory`, every `characters` lacks `canManageInventory`,
every `terminal_sessions` lacks `cols` / `rows`. **v4 does NOT auto-ALTER
at init:** `generateAlterStatements` is only re-exported
(`lib/database/index.ts:113`; `git grep "generateAlterStatements("` finds
no caller outside `schema-translator.ts`); `initializeDatabase` →
`ensureCollection` creates MISSING TABLES only; the oracle's
`harness/oracle/lib/v4-migrations.ts` runs narrowed lists on request (51
importers). So at regen the committed fixtures stay as they are, and:
- reads are `SELECT *` (`backend.ts:358-430`), so the key is simply absent:
  `normalizeEquippedInventory(undefined)` → `{}`
  (`chats.repository.ts:785,811`), `character.canManageInventory !== false`
  → `true` (`tools/route.ts:635`), `session.cols ?? 80`;
- INSERTs are built from `Object.keys(row)` (`backend.ts:121-125`), so a
  create carries a new column only when the create body names it: the
  characters PUT (`handlers/put.ts:77`) patches `canManageInventory` only
  when the body sends it; terminal sessions are created by the PTY manager
  with `cols` / `rows` (the handler's `:88-89`), which no oracle drives
  (`terminal-tools.ts` reads a pre-baked session: "there is no live PTY in
  this process");
- the ONE unconditional write is E.5.1's, caught by `safeQuery`.
**Does v4's migrations-first boot ALTER them? Only in the families that run
it** (`migrations-first.ts`, `migration-index-backfill.ts`,
`restore_vintage_state`'s mirror-boot list — memory `vintage-restore-
mirror-boot-list`: every new boot ensure must be added there), and there
the ALTER is the expected tripwire of E.5.3.

**E.5.6 — `jest.setup.ts` (`7263105c6`).** The hunk adds
`delete process.env.QUILLTAP_TIMEZONE` (+6 lines) under
`setupFilesAfterEnv: ['<rootDir>/jest.setup.ts']` (`jest.config.ts:46`;
`process.env.TZ = 'UTC'` at `:11`). Every jest-driven oracle case already
deletes the variable itself (`update-memory-tier2.test.ts:102`,
`fold-other-pass-tier3.test.ts:135`, `extraction-enqueue-tier2.test.ts:89`,
`memory-processor-tier3.test.ts:118`, `admission-tier3.test.ts:102`,
`memory-regenerate-jobs-tier3.test.ts:89`, `fold-episode-tier3.test.ts:112`,
`markdown-transcript.ts:48`), and the only recipes that PASS
`QUILLTAP_TIMEZONE=` are tsx runs (`memory_rewrite_heals_ensure_equivalence.rs`,
`zero_migrations_rerun_equivalence.rs`; `memory-time-zone-refresh-tier2.ts`
sets it in-process) — memory `oracle-second-zone-needs-tsx` already forbids
jest for second-zone oracles. No v5 file copies `jest.setup.ts` (the ten
"jest.setup" grep hits are comments naming its global mocks). **Harmless.**

### E.6 Conclusion

- **REAIM lane: NO.** Nothing fails to load; nothing garbles silently.
- **Stacking:** the Inventory keystone (column 13 + `setEquippedInventory`
  + `applyInventoryDefaults` at the three sites + the `fresh_schema.json`
  re-dump for moves 13–15 + the three boot ensures + the
  `restore_vintage_state` boot-align) is the stage the chat-create /
  participants / merge / provisioning regens must branch from; the Log
  lane owns move 16 and its provisioning ruling; every other lane cuts from
  `main` and absorbs E.4's drift in its own Tier 1.

---

## §F — Dependencies

`git diff e8e17b2d9 322a5488e -- package.json package-lock.json
packages/quilltap/package.json packages/theme-storybook/package*.json
'plugins/*/package.json'`:

- `package.json`: `"version"` `4.11.0-dev.26` → **`4.11.0-dev.53`**;
  `"test:unit"` gains `NODE_ENV=test`; **`"@xterm/headless": "^6.0.0"`**
  added (between `@xterm/addon-web-links` and `@xterm/xterm`).
- `package-lock.json`: root + `packages[""]` `4.11.0-dev.21` →
  **`4.11.0-dev.53`** (the lock is now IN STEP with the manifest — the
  previous survey's "lock lags" note no longer holds at this pin); ONE new
  package node `node_modules/@xterm/headless` `6.0.0`, `resolved
  https://registry.npmjs.org/@xterm/headless/-/headless-6.0.0.tgz`,
  `integrity sha512-5Yj1QINYCyzrZtf8OFIHi47iQtI+0qYFPHmouEfG8dHNxbZ9Tb9YGSuLcsEwj9Z+OL75GJqPyJbyoFer80a2Hw==`,
  `workspaces: ["addons/*"]`. **No other dependency, devDependency or
  resolved line moves** — `openai` 7.34.0, `zod` 4.6.7, plugin-utils 2.7.1
  stand; `provider_sdk_version_guard.rs` / `zod_version_guard.rs` need no
  edit.
- `packages/quilltap/package.json`: `dev.26` → `dev.53`.
- `packages/theme-storybook/package.json` 1.0.74 → 1.0.77; its lock 1.0.74
  → 1.0.76 (lags by one — NO-PORT).
- `plugins/*/package.json`: no change.
- **Can the pin's `npm ci --offline` be built from the cache?** Yes:
  `npm cache ls | grep xterm/headless` →
  `make-fetch-happen:request-cache:https://registry.npmjs.org/@xterm/headless/-/headless-6.0.0.tgz`
  (the index grep over `~/.npm/_cacache/index-v5` by package name finds
  nothing because entries are keyed by hash — use `npm cache ls`). The live
  `node_modules/@xterm/headless/package.json` is `6.0.0` and
  `node_modules/.package-lock.json` is stamped `4.11.0-dev.53`, so the
  symlink classes (ledger §5.1) also resolve it. Keep
  `npm ci --offline --no-audit --no-fund` in the pin as the default (the
  2026-10-07 rule); memory `tier-r-vacuous-equal-rows` reminds that a
  `--ignore-scripts` install needs the sqlite build copied.

---

## §G — The `V4_APP_VERSION` stamp

ONE site: `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:126`

```rust
const V4_APP_VERSION: &str = "4.11.0-dev.26"; // v4 `e8e17b2d9` (the oracle baseline at the `e8e17b2d9` round's unification — moves with every baseline move)
```

→ `"4.11.0-dev.53"` with the comment re-pointed at `322a5488e`. The doc
comment at `:122-125` says why ("v4 bumps it on EVERY commit, so this
constant moves with every regen"). No other `4.11.0-dev` literal exists
under `crates/`, `apps/web/src`, `harness/oracle` (the only other hits are
`harness/tools/sweep-results/*.json`, records). Owner: whichever lane
regenerates `ai_import_tier3` — by precedent the vendor lane carries the
constant when no content lane touches the family; this round the Inventory
carriers lane touches the AI-import validator (the export schema), so hand
it to that lane and have the vendor lane check the union.

---

## §H — Rulings owed and disagreements with the ledger

1. **Where the engine embeds the custom-tool schema + the two specimens**
   (C.3): `docs/v4/developer/CUSTOM_TOOL_SPEC{,_DICE}.json` +
   `apps/web/public/schemas/qtap-custom-tool.schema.json` as direct
   `include_str!` build inputs (two mirror paths become load-bearing), or
   crate-local copies under `crates/quilltap-core/src/pascal/` with a byte
   guard against the mirrors. Recommend the crate-local copy + guard (the
   export-schema precedent) — it keeps `docs/v4/` a pure mirror.
2. **`help/` guard literal placement:** move both literals to 135 AND fix
   `host_help_docs_boot.rs:126`'s stale message (`"the vendored tree at v4
   01a83539d"`) in the same commit — P4.D323's recorded OPEN item.
3. **The theme file split** (D.6): confirm the vendor lane owns
   `apps/web/public/themes/madmans-box/theme.json` + the three
   `icons/*.svg` byte copies and the SPA lanes own `public/images/icons/`
   + `icon.ts`; and ask the SPA lanes how v5 resolves theme icon overrides
   when `icons/` holds only `brand.svg`.
4. **Ledger agreement:** every §3 row's hunk-level facts I touched agree
   with the diffs. Two refinements: (a) the ledger's "Help / docs" note
   lists `custom-tools.md +33 / −3 by three commits` from the 10-19 probe —
   at the pin it is **+205 / −10 by SEVEN**; (b) the ledger's lockfile note
   "the lockfile stamp LAGS" is true at `e8e17b2d9` (`dev.21`) but not at
   `322a5488e` (`dev.53` both).
5. **`cad017599` / `cc57e6cbc`:** ratify NO-PORT at this round's
   unification with D.1 / D.2 as the evidence; the ledger rows already say
   "VERIFIED".
6. **The arrival-write stacking rule** (E.5.1) is a planning constraint, not
   a ruling — but the planner should decide whether the keystone lane also
   carries `setEquippedInventory`'s caught-failure lines
   (`Failed to write equipped inventory` + `Equipped inventory write`) as
   captured pins, since a v5 instance opened on a pre-column v4 DB (the
   migration runner is deferred) would hit exactly that arm.
