# Survey — the 4.11-dev vendor + NO-PORT lane: `help/` (9 pages), `public/schemas/` (2 files), the `docs/v4/` mirror (61 operations), the NO-PORT record, and the IMPORT-DEAD check

Surveyed 2026-10-16, READ-ONLY, for the round that moves the oracle baseline
from `83334b9a5` (B) to the PIN `25b8fab6aa58636c6885ba63684c7f8fbfca9c3b`
(P — v4 `main` HEAD, `4.11.0-dev.10`). v5 = `main` `56554d98c`. Every v4 fact
below was read with `git -C ~/source/quilltap-server show P:<path>`,
`git show <sha> -- <paths>`, `git diff [--stat|--name-status -M] B P -- <paths>`
or `git ls-tree`; every v5 fact by grep/read at `56554d98c`. Facts come from
SHIPPED HUNKS, never from commit messages. Shape follows the
`2026-10-13-setupphase-memory-spa-stream-help-docs.md` survey (§8.3 / §8.4) and
the P4.D303 order.

**Headline findings (the planner's list):**

1. **The IMPORT-DEAD check is POSITIVE and it is large.** `7417e9a1b` deleted
   `createMountWardrobeItem` (and `deleteMountWardrobeItem`), and v5's oracle
   **fixture builders import it at 12 sites in 11 files** (`§5`). Nine of
   those builders are invoked by recipe headers, so **18 families' regens die
   at the pin** with `createProjectWardrobeItem is not a function` — among them
   every wardrobe family the wardrobe-copy (`bbbcd2388`) and hair-slot
   (`7458ff94e`) lanes must regenerate. Three more oracle cases die at the pin
   on their own: `danger-trigger.test.ts:163` (`jest.doMock('@/lib/memory', …)`
   of a deleted barrel), `refusal-classify.test.ts:205/:223`
   (`isModerationRefusal`), and `memory-time.ts:39/:147`
   (`MEMORY_CALENDAR_DAY_MS` -> a `null` the Rust arm `unwrap()`s). **Ordering
   consequence: the repair is `harness/oracle`-only (zero `crates/*/src`
   hunks) and must land FIRST (stage A) or every sibling lane's pin-regen is
   red for a reason that is not theirs.** §5 gives the file:line list and the
   exact replacements.
2. `help/`: exactly **9 pages modified, 0 added, 0 deleted; 131 -> 131**
   (`+154 / -13`). One page changes its front-matter `url:` (`chat-settings-composer`
   -> `section=composer-slash-commands`). v5's tree is **byte-equal to `83334b9a5`
   for all 131 files** (measured), so the move is nine `git show P:help/<p>.md`
   copies. The two count literals (`VENDORED_FILE_COUNT = 131`,
   `assert_eq!(expected, 131, …)`) are UNMOVED.
3. `public/schemas/`: `qtap-custom-tool.schema.json` 39,471 -> **43,493 B**,
   `qtap-export.schema.json` 105,310 -> **105,774 B**. v5 vendors them at
   **two** places (the SPA's `public/schemas/` and the core's `generators/`) and pins
   them with **three** byte constants (§2). Recommendation:
   **the vendor lane carries the bytes AND the three constants**; the Pascal lanes
   only consume (no Rust code reads either schema's new keys).
4. `docs/v4/`: **61 operations** — 11 A, 4 D, 17 R (all into `features/complete/`;
   13 need a content overwrite after `git mv`, 4 are R100), 29 M (incl. the CLI
   README mirror). The mirror is a WHOLE-tree mirror (not "paths a round
   needs"): after the move `git ls-files docs/v4` must be **491** (484 - 4 + 11).
   The 4 deletions (`MCP.md`, `distant_future_separate_api.md`,
   `native-core-api-boundary.md`, `native-port-phase-0.md`) are all present in
   the mirror at pre-bytes; **ruling proposed: `git rm` them** (precedent: P4.81
   item 5, `5e2882ec9`) — §3.3.
5. NO-PORT: ten whole rows (nine subsections, §4.1–§4.9) ratify from hunks, plus
   eight HALF-subsections inside PORT rows (§4.10–§4.17). §9 lists the ledger statements that are
   wrong or loose (items 1–8, each corrected from a hunk).
6. v4's working tree is no longer clean (docs-only dirt: `ROADMAP.md`,
   `.claude/commands/update-documentation.md`, one untracked
   `pascal-wardrobe-effect-picker.md`). Vendor `git show P:`, never the tree —
   the dirty `ROADMAP.md` differs from the pin (§3.4).

---

## 0. The span, in one paragraph

26 commits `83334b9a5..25b8fab6a` (2026-10-13 21:36 -> 2026-10-16 07:07, all
the ledger's §3 rows). Paths in this slice that they move: `help/` — 9 files
across 6 commits (`bbbcd2388` wardrobe copy, `7458ff94e` hair slot,
`362426c8f` trigger phrases, `f0db8cbb8` wardrobe effects, `4877fc2da` slash
commands, `b2793400f` bug 251); `public/schemas/` — 2 files across
`362426c8f` + `f0db8cbb8` only; `docs/` — 60 paths + `packages/quilltap/README.md`
(`75d094e63`), across 17 commits. No `help/` page is added or removed, no
`lib/help*` / `lib/help-guide` / `lib/mount-index/chunker.ts` hunk (all three
`git diff --stat B P` EMPTY — the chunker, the front-matter parser, the Guide
categories and `HELP_SECTION_EMBEDDING_MAX_TOKENS = 1000` are unmoved, so the
v5 help machinery needs no code change and `apps/web/src/app/help/__fixtures__/`
needs no re-record). The release machinery (`2432b9057` `release: 4.10.0`,
`f573a7576`, `37246654d` `4.11.0-dev.0`) contributes five stamp files and an
empty merge; the docs gain `docs/FEATURES.md` and lose 3,600 CHANGELOG lines to
`CHANGELOG_V4.md`. The IMPORT-DEAD hazard is `7417e9a1b` alone.

| commit | date | what this slice sees |
|---|---|---|
| `7417e9a1b` | 10-13 | docs: `DEAD-CODE-REPORT.md`, `complete/memory-system-refactor.md` (one line), CHANGELOG; **code: 2 barrels deleted + 9 exports (§4.1, §5)** |
| `01f508d4e` | 10-13 | docs: CHANGELOG only |
| `a42c282b8` | 10-13 | docs: CHANGELOG only |
| `a8b2d2861` | 10-13 | docs: CHANGELOG only (+ `packages/plugin-utils/CHANGELOG.md`, not mirrored) |
| `49f8906c3` | 10-14 | docs: CHANGELOG only |
| `75d094e63` | 10-14 | docs: `packages/quilltap/README.md` (+14/-4) -> the CLI README mirror |
| `8489d08b9` | 10-14 | docs: 17 renames, 4 deletes, API.md, bugs.md, CHANGELOG moves, `releases/4.10.0.md` reorder; comment-path renames in code (§4.14) |
| `052659624`, `809e5104d` | 10-14 | docs: bug 247 filing (`bugs/bug-247-*` OPEN, `bugs.md`) — net effect after `25b8fab6a` is the `fixed/` copy only |
| `bbbcd2388` | 10-14 | help: `wardrobe.md`, `project-wardrobe.md`; docs: `API.md`, `complete/wardrobe-folders.md` |
| `b82037d75` | 10-14 | docs: bug 248 (`fixed/`), `bugs.md` |
| `3f64d3a6c` | 10-14 | docs: NEW `FEATURES.md` (+119), CHANGELOG |
| `2432b9057` | 10-14 | docs: `releases/4.10.0.md` 1-line stamp, CHANGELOG 1 line |
| `f573a7576` | 10-14 | merge, combined diff EMPTY |
| `37246654d` | 10-14 | docs: CHANGELOG +2 |
| `1e1712a12` | 10-14 | docs: CHANGELOG -> `CHANGELOG_V4.md` (4.9.2 section, +57/-57) |
| `7458ff94e` | 10-14 | help: `wardrobe.md`; docs: CHANGELOG -> `CHANGELOG_V4.md` (3,600 lines) |
| `936a34720` | 10-15 | docs: three NEW specs (top-level, later moved), CHANGELOG |
| `362426c8f` | 10-15 | help: 4 pages; **schemas: both**; docs: `API.md`, `DDL.md`, `CUSTOM_TOOL_SPEC_DICE.json`, `FEATURES.md`, specs; **prettify label (§4.11)** |
| `f0db8cbb8` | 10-15 | help: 3 pages; **schemas: both**; docs: `CUSTOM_TOOL_SPEC.json`, `DDL.md`, `FEATURES.md`, 4 specs |
| `6069a7acd` | 10-15 | docs: bug 249 (`fixed/`), `bugs.md`, CHANGELOG |
| `4877fc2da` | 10-15 | help: 4 pages (incl. the `url:` move); docs: `API.md`, `DDL.md`, `composer-slash-commands.md`, `composer-typeahead.md`; **prettify label (§4.11)** |
| `b93c2efda` | 10-15 | docs: two NEW specs (top-level; moved to `complete/` by `eb5d42338` / `b2793400f`) |
| `eb5d42338` | 10-15 | docs: `complete/dependency-upgrade-safe-batch.md` (move + edit), CHANGELOG |
| `b2793400f` | 10-16 | help: `chats.md`; docs: bug 251 (`fixed/`), `complete/lexical-0.52-upgrade.md`; code: Lexical-only (§4.12) |
| `25b8fab6a` | 10-16 | docs: bug 247 -> `fixed/`, bug 250 (`fixed/`), `bugs.md`; code: Lexical wrapper (§4.13) |

**Net-path rule (matters for the mirror):** `936a34720` / `b93c2efda` add specs at
`docs/developer/features/<x>.md` and `362426c8f` / `f0db8cbb8` / `eb5d42338` /
`b2793400f` move them to `complete/`; bug 247 / 250 are filed OPEN under
`bugs/` and moved to `bugs/fixed/`. At the pin only the FINAL paths exist, so
the mirror never sees an intermediate path — each is a plain **A** at its
final location (confirmed: `git diff -M --name-status B P` lists them as `A
…/complete/…` and `A …/bugs/fixed/…`).

---

## 1. `help/` — exact diff `83334b9a5..25b8fab6a` (9 pages; 131 -> 131)

`ls-tree -r --name-only` counts: **131 at B, 131 at P**, same name set (0 A, 0 D).
`git diff --stat B P -- help` = `9 files changed, 154 insertions(+), 13 deletions(-)`.

| page | +/- (B to P) | lines B to P | bytes B to P | md5 at pin | commits (oldest first) |
|---|---|---|---|---|---|
| `help/chat-settings-composer.md` | +44 / -4 | 296 -> 336 | 26411 -> 30540 | `e850a2a6c5f34d42f2ac6a2770575c0e` | `4877fc2da` |
| `help/chat-settings.md` | +2 / -0 | 229 -> 231 | 13637 -> 14262 | `43d548044d8fbd1b905f6db9ec9e02fd` | `362426c8f` |
| `help/chats.md` | +2 / -0 | 655 -> 657 | 45899 -> 46552 | `94d84aa97e2cc4d71224ed71f57f3bc5` | `b2793400f` |
| `help/custom-tools.md` | +85 / -4 | 562 -> 643 | 49790 -> 59153 | `1684c45d15135c627bca00c9c5feb144` | `362426c8f`, `f0db8cbb8`, `4877fc2da` |
| `help/pascals-workbench.md` | +3 / -0 | 83 -> 86 | 12998 -> 15063 | `4ffa8e9f7fa2a8ad9b99c6e1e0e4dce7` | `362426c8f`, `f0db8cbb8` |
| `help/project-wardrobe.md` | +1 / -1 | 73 -> 73 | 8583 -> 8666 | `816c8bac09a38d31d03ab440f00d2b5f` | `bbbcd2388` |
| `help/rng-tool.md` | +6 / -0 | 146 -> 152 | 4420 -> 5112 | `7c37e44b03dc33c081bc589486838b06` | `362426c8f`, `4877fc2da` |
| `help/run-tool.md` | +2 / -0 | 82 -> 84 | 4568 -> 4985 | `486982f58f88abacc951fde54febe0e9` | `4877fc2da` |
| `help/wardrobe.md` | +9 / -4 | 402 -> 407 | 64663 -> 66581 | `3125517a0a30645bf60db9f181ad8ff2` | `bbbcd2388`, `7458ff94e`, `f0db8cbb8` |

(`-` counts are replaced lines, not removals of content: e.g. `wardrobe.md`'s
`-5` are five edited bullets/sentences.) Front matter: **only
`help/chat-settings-composer.md` changes its `url:`** — the other eight keep
theirs (`/settings?tab=chat`, `/salon`, `/settings?tab=chat&section=custom-tools`,
`/custom-tools`, `/prospero/:id`, `/salon/:id` ×2, `/aurora`).

### 1.1 Per page — what the hunks say (all quotes are from `git diff B P`)

- **`chat-settings-composer.md`** (`4877fc2da`, +44/-4):
  `url: /settings?tab=chat&section=composer-spellcheck` ->
  `/settings?tab=chat&section=composer-slash-commands` **and** the in-body
  `> **[Open this page in Quilltap](/settings?tab=chat&section=composer-slash-commands)**`
  link, and the trailing `help_navigate(url: "/settings?tab=chat&section=composer-slash-commands")`
  line — three URL occurrences; the intro paragraph gains "what a slash at the
  start of a word may summon"; **NEW H2 `## Slash Commands`** (between the
  "In their own words" block and "## Auto-Scroll") — what is on the menu,
  run-at-once vs a form, a private roll needs the dialog, "What the slash leaves
  behind. Nothing", "Prose is safe", the **`Slash commands`** toggle ("a single
  toggle in the **Slash Commands** card, on by default"), When useful / When to
  turn it off. No other section moves. (v4's matching UI: `ChatTabContent.tsx`
  `<CollapsibleCard title="Slash Commands" … sectionId="composer-slash-commands">`
  — the deep-link target the new `url:` names; v5's `chat-tab.ts:110-111` has
  only the `composer-spellcheck` card id. **Rider for the slash-commands SPA
  lane**, not this lane: without a `sectionId="composer-slash-commands"` card
  the help page's "Open this page in Quilltap" link lands on the bare Chat tab.)
- **`chat-settings.md`** (`362426c8f`, +2/-0): one bullet in the Automation
  block, "**Auto-Run Custom Tools on Trigger Phrases** — Off by default …" with
  the link `custom-tools.md#words-that-pull-the-lever--triggers` (after the
  "dice mentions become real rolls" list, before "**When to disable:**").
- **`chats.md`** (`b2793400f`, +2/-0): one paragraph, "**A word on code
  blocks.** Three backticks and a space open a code block in the composer …
  strike **Shift+Enter** once more to leave a blank line at the foot of the
  block, then a plain **Enter**: the blank line is quietly tidied away … In
  composition mode the same blank-line **Enter** steps out of the block". This
  is bug 251's user-visible text; the PORT half (the exit trim) is the editor
  lane's (`apps/web/src/app/editor/editing-commands.ts`, ledger row
  `b2793400f`). The vendor lane only carries the bytes.
- **`custom-tools.md`** (`362426c8f` touches 30 lines, `f0db8cbb8` 57, `4877fc2da` 2
  per `--stat`; **net +85 / -4, 49,790 -> 59,153 B**): the big one.
  Headings added (the only three — verified by diffing the `^#` lists):
  **`## Words that pull the lever — \`triggers\``**, **`### Dressing and
  undressing`**, **`#### A tool that simply dresses someone`**. New anchors
  other pages link to: `#words-that-pull-the-lever--triggers`,
  `#dressing-and-undressing`. Edited prose: the "Recording consequences — side
  effects" opener ("…or else changing what that character **has on**"); the
  closing "Your characters are told …" paragraph (wardrobe effects described
  *wears Cloak of Night, Boots* / the bench's *would attempt*); the run-dialog
  section gains the `/pascal` slash paragraph; "## Limits" gains "eight trigger
  phrases per tool (three to eighty characters each) and four triggered tools
  per message"; "## Turning it off" gains the second switch. (The schema facts
  these sentences state — `triggers` `minItems 1 / maxItems 8`, 3–80 chars;
  `wardrobe` actions `wear` / `take_off` / `clear_slot`, `items` 1–8, `mode`,
  `slot` — are in §2 and agree.)
- **`pascals-workbench.md`** (`362426c8f` +1, `f0db8cbb8` +2; net +3/-0): two
  editor bullets — "**Trigger phrases.**" (Enter/Add chips, ≤ 8, the echo
  warning, the library's "✦ N phrases" badge) and "**Side effects.**" (Kind:
  *Write a value* / *Change the outfit*) — and one proving-bench bullet,
  "**Side effects, dry.**" (`→ wardrobe: wear "Cloak of Night" (would attempt)`).
  Surface for the Workbench SPA lane.
- **`project-wardrobe.md`** (`bbbcd2388`, +1/-1): the "**File** the collection
  into folders" bullet now names **Move to folder…** *or* **Copy to folder…**
  ("an outfit asks whether its components should come along").
- **`rng-tool.md`** (`362426c8f` +2, `4877fc2da` +4; net +6/-0): a sentence that
  Pascal's custom tools listen for phrases the same way (link to the new
  anchor), and **NEW `### From the Composer's Slash Commands`** (`/d6`, `/d20`,
  `/coin`, `/bottle`, `/roll`) — one added heading.
- **`run-tool.md`** (`4877fc2da`, +2/-0): the paragraph "Or, without lifting
  your hands from the keys: type `/tool` at the start of a word …".
- **`wardrobe.md`** (`bbbcd2388` +4/-2, `7458ff94e` +2/-1, `f0db8cbb8` +3/-1;
  net +9/-4): "Three consequences follow" -> "**Four** consequences follow" plus
  the new bullet "**A styled coiffure outranks the natural hair.**"
  (`7458ff94e`, the hair-slot PORT row's text); in Folders: NEW bullets
  "**Copy to folder…**" and "**Outfits ask after their components.**", the
  Select bullet's bar text now "*N selected · Move to folder… · Copy to folder…
  · Cancel*", and "moving an outfit moves only the outfit, never its pieces"
  -> "moving an outfit moves its pieces only if you say so when asked"; a
  paragraph after the `wardrobe_archive` bullet ("A custom tool may change an
  outfit too …"); the Aurora announcement sentence gains "the character
  themselves reaches for a `wardrobe_` tool, or a custom tool's wardrobe effect
  does it for them".

**Heading inventory (for the chunker):** `custom-tools.md` +3 (`##`, `###`,
`####`), `chat-settings-composer.md` +1 (`##`), `rng-tool.md` +1 (`###`); the
other six pages add no heading. The `chunks` count in `help_tree_equivalence`
therefore moves from 779 (the P4.D303 oracle) by an amount only the
regenerated oracle can state — MEASURE it, do not predict it.

### 1.2 Section size (the one check the diff cannot answer)

`HELP_SECTION_EMBEDDING_MAX_TOKENS = 1000` (cl100k) at P
(`lib/help/help-doc-chunking.ts:54`, unmoved). The largest section at B was 775
tokens (`startup-wizard.md`, P4.D303 record). The new `custom-tools.md` sections
are long — v4's own `__tests__/unit/lib/help/help-doc-size.test.ts`
gates every `help/` file at 1000 tokens and the commits shipped, so the pin is
within the ceiling by v4's gate; but `help_section_size_equivalence` also holds
**v5's composed text byte-equal to the oracle's**, and that is only knowable
by running the regen (§8). Expected, not asserted: GREEN, `files` 131,
`sections` > 779.

### 1.3 Where v5 vendors `help/` and what pins it

| piece | path | role |
|---|---|---|
| the tree | `help/` (repo root, 131 files, **byte-equal to `83334b9a5`** — `diff` of every v5 file against `git show 83334b9a5:help/<f>`: 0 differences, identical name set) | the vendored source |
| embed | `crates/quilltap-host/build.rs` -> `OUT_DIR` table -> `crates/quilltap-host/src/help_content.rs::EMBEDDED_HELP` | `include_str!` of every `help/**/*.md`, v4's `findMarkdownFiles` order (`strcmp` per directory) |
| walker/reader | `crates/quilltap-host/src/files_store.rs` (`embedded_help_source_files`, `load_help_source_files`) | the ONE source for the boot reconcile and `EMBEDDING_REINDEX_ALL` |
| embed guard | `crates/quilltap-harness/tests/help_tree_embed_guard.rs` — `const VENDORED_FILE_COUNT: usize = 131;` | path set + ORDER + BYTES of embedded vs on-disk; size = 131 |
| boot test | `crates/quilltap-host/tests/host_help_docs_boot.rs` — `assert_eq!(expected, 131, "the vendored tree at v4 01a83539d");` (:117; module doc says "the count stays 131" :14, :17) | shipped tree's 131 rows + chunks through the boot ensure |
| content oracle | `crates/quilltap-harness/tests/help_tree_equivalence.rs` ← `harness/oracle/cases/help-tree-sync.test.ts` (jest, v4's REAL `ensureHelpDocsSynced`) | docs rows (hash, title, `url`), section chunks, walk order |
| size oracle | `crates/quilltap-harness/tests/help_section_size_equivalence.rs` ← `harness/oracle/cases/help-section-size.ts` (tsx, v4's real chunker + `js-tiktoken`) | v5's composed section text == v4's, every count ≤ 1000 |
| spelling | `crates/quilltap-harness/tests/spelling_guard.rs` -> `harness/tools/check_spelling.py` | "Quilltap" in `help/` + `docs/v4/`; measured 0 misspellings in every P-side byte this lane writes |

Not pinned by any count/URL literal (grepped `crates/`, `apps/web/src`,
`harness/oracle`): `chat-settings-composer`'s old `section=composer-spellcheck`
`url:` appears nowhere in `crates/**` (the five `apps/web` hits are the SPA's
OWN card ids for the spellcheck card, which still exists in v4:
`ChatTabContent.tsx` keeps `composer-spellcheck`). The Guide tab's category
tables (`help-categories.ts`, `__fixtures__/help-guide-tables.json`) list
slugs only, and v4's `lib/help-guide/categories.ts` is unmoved -> no fixture
re-record.

### 1.4 The vendor move for `help/`

`for p in chat-settings-composer chat-settings chats custom-tools pascals-workbench project-wardrobe rng-tool run-tool wardrobe; do git -C ~/source/quilltap-server show 25b8fab6a:help/$p.md > help/$p.md; done`
— then the NUL-safe census over `git ls-tree -z -r --name-only P -- help/` (131
paths, 0 md5 mismatches, `git ls-files help/` the identical set). md5s are in
the table above. Comment-only edits (no literal moves): add a
"131 at v4 `25b8fab6a` (131, nine pages re-vendored, none added or retired)"
line to `help_tree_embed_guard.rs`'s doc block (**do not start the new line with
`+`** — clippy read a leading `+` as a list item last round, `03b1ea1e7`) and to
`host_help_docs_boot.rs`'s count comment (near :113-117), and a history line to
`help_tree_equivalence.rs`.

---

## 2. `public/schemas/` — two files move (`362426c8f`, `f0db8cbb8`); four others do not

`git ls-tree -r --name-only P -- public/schemas` = `plugin-manifest`,
`qtap-custom-tool`, `qtap-export-ndjson`, `qtap-export`, `qtap-progression`,
`qtap-theme` (`.schema.json`). `git diff --stat B P -- public` = exactly two:

| file | B bytes | P bytes | md5 at P | stat | commits |
|---|---|---|---|---|---|
| `public/schemas/qtap-custom-tool.schema.json` | 39,471 | **43,493** | `0f0f2f49e30f4c284538204048e0bec2` | +101 / -7 | `362426c8f` (+8/-0), `f0db8cbb8` (+94/-8) |
| `public/schemas/qtap-export.schema.json` | 105,310 | **105,774** | `910063dd069bd581cff3e22d531976cc` | +3 / -2 | `362426c8f` (+2/-1), `f0db8cbb8` (+1/-1) |
| `public/schemas/qtap-progression.schema.json` | 5,963 | 5,963 | `796dbe5209caed76196d2f01169efa34` | — | UNMOVED |

(Authority: `git diff --numstat B P -- public` = `101 7` and `3 2`; the byte
sizes and md5s above are the contract.)

### 2.1 What the hunks add

**`qtap-custom-tool.schema.json`** (`$id` unchanged, `https://quilltap.ai/schemas/qtap-custom-tool.schema.json`):

- `properties.triggers` (`362426c8f`): `type array`, `minItems 1`, `maxItems 8`,
  `uniqueItems true`, items `string` `minLength 3` `maxLength 80`; description
  "Phrases that run this tool automatically, with every parameter at its
  default … exact substrings, case-insensitive, no wildcards … Only honoured
  when the chat setting 'Auto-Run Custom Tools on Trigger Phrases' is on".
  Position: between `chipLabel` and `description`.
- `properties.effects` description (`f0db8cbb8`) widened: "…and wardrobe
  gestures that dress or undress the rolling character."
- `$defs.Effect` (`f0db8cbb8`): the `required: ["target","value"]` block is
  REMOVED; description rewritten to "EITHER a write (target + value) OR a
  wardrobe gesture (wardrobe), never both and never neither"; `target`'s
  description edited; NEW `properties.wardrobe` (`$ref #/$defs/WardrobeGesture`);
  NEW `oneOf` — branch 1 requires `target`+`value` and forbids `wardrobe`,
  branch 2 requires `wardrobe` and forbids `target` / `value`.
  **This LOOSENS acceptance for editors:** a `{wardrobe: …}` effect with no
  `target`/`value` was invalid before and is valid now.
- NEW `$defs.WardrobeGesture` (`f0db8cbb8`): `required ["action"]`,
  `additionalProperties false`; `action` enum `wear | take_off | clear_slot`;
  `items` array 1–8 of strings 1–200; `mode` enum `wear | replace | add_to_slot`;
  `slot` enum `top | bottom | footwear | accessories | hair`. The per-action
  rules are in the description ("checked by the loader, not by this schema").
- Everything else (`$defs` list: `Effect, WardrobeGesture, EffectWhen,
  ToolGate, GateComparator, NumberOperand, AnyOperand, StringOperand,
  NumericComparator, ParamComparator, MetadataComparator, LlmComparator,
  ParamRef, StateRef, NumberOrParamRef`) unmoved.

**`qtap-export.schema.json`**: inside the chat-message `pascalMeta` object (line 926 at P, under `$defs.ExportedChat`'s messages)
(`"required": ["tool","definitionTier","definitionMountId","params","rollForm","raw","value","state","outcomeIndex","invokedBy"]`):

- `effects` description gains the wardrobe targets ("A wardrobe change is
  targeted 'wardrobe.wear' / 'wardrobe.take_off' / 'w…'") — description only.
- **`invokedBy` enum `["llm","user"]` -> `["llm","user","trigger"]`** and its
  description extended — an ACCEPTANCE change for a present value
  `"trigger"` (the object has no `additionalProperties: false`, so no
  new-key refusal).
- **NEW `triggerPhrase`**: `{ "type": "string", "description": "The phrase that
  fired a triggered run, as written in the definition. Present only when
  invokedBy is 'trigger'." }` between `invokedBy` and `callerParticipantId`
  — documentation only (the object is open).

### 2.2 Where v5 vendors them, and every pin

| file | v5 location(s) | embedded/served how | pins (BYTE constants) |
|---|---|---|---|
| custom-tool | `apps/web/public/schemas/qtap-custom-tool.schema.json` (git-tracked; the Angular SPA serves it; `tool-draft.ts:57` `DEFAULT_SCHEMA_VALUE = '/schemas/qtap-custom-tool.schema.json'`, written into every tool draft's `$schema`) | static asset | `crates/quilltap-harness/tests/public_schemas_vendor_guard.rs` — table `SCHEMAS[0].bytes: 39_471` (doc comment says "at v4 `25f534c0b`") + `the_vendored_schemas_equal_the_v4_checkouts` (byte-equal to `$QT_V4_CHECKOUT/public/schemas/…`, SKIPs without a checkout) + `the_vendored_schemas_are_self_consistent` (parses, `$schema`, `$id`, every `$ref` local) |
| progression | `apps/web/public/schemas/qtap-progression.schema.json` | static asset | same guard, `bytes: 5_963` — UNMOVED, still byte-equal to P (cmp) |
| export | `crates/quilltap-core/src/generators/qtap-export.schema.json` (105,310 B == B, cmp) | `include_str!` -> `generators::qtap_schema::QTAP_EXPORT_SCHEMA_JSON`; `validate_qtap_export` compiles it with the `jsonschema` crate (Cargo.toml:148-162, human-approved 2026-09-07) | **TWO constants**: `crates/quilltap-harness/tests/qtap_schema_embed_guard.rs:72` `const VENDORED_BYTES: usize = 105_310;` AND `crates/quilltap-core/src/generators/qtap_schema.rs:195` `assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(), 105_310);` (the guard's own doc warns "A re-vendor moves both"); plus the v4-equality test in the guard and `qtap_schema_validate_equivalence.rs` (ajv-vs-crate graded diff, oracle `harness/oracle/cases/qtap-schema-validate.test.ts`) |
| `apps/web/dist/**` copies | untracked build output (`apps/web/dist/quilltap/browser/schemas/…`) | rebuilt by the SPA build | none — do not hand-edit |

`custom_tool_types.rs:9` calls the published schema "a mirror … for editor
completion": **no Rust code validates a custom tool against the JSON file**
(the loader is a hand port of v4's Zod, with Zod 4.4.3's message order). The
export schema's ONE production consumer is `generators/ai_import.rs:64`
(`validate_qtap_export`, the AI-import assembled-export validation); `.qtap`
export/backup writers do NOT validate against it. `docs/v4/developer/CUSTOM_TOOL_SPEC*.json`
(which gain the new keys) are referenced by no test.

### 2.3 Lane ownership — recommendation

**The vendor lane carries both byte files and all three byte constants; the Pascal
lanes consume.** Justification from how v5 uses the files:

1. The custom-tool schema is a *served asset with a byte-equality guard* and no
   Rust consumer. Its only semantic content for v5 is what the SPA editor
   offers in completion. Nothing the trigger/effects server lanes write reads
   it, so there is no producer-before-schema ordering. The guard fails the
   moment the bytes differ, which is the vendor lane's single, mechanical job.
2. The export schema's only production consumer is AI-import validation, and
   a model-assembled export carrying `pascalMeta.invokedBy: "trigger"` is an
   exotic input. v5's `PascalMeta.invoked_by` is already a free `String`
   (`db/chats_messages.rs:330`); the Pascal-trigger lane adds the Rust
   `trigger_phrase` field and its key order on its own authority — it does not
   need the schema moved first. Splitting the schema across two lanes would
   put the same two `105_310` constants in two diffs (a guaranteed union red);
   one owner moves all three numbers once (`43_493` in the vendor guard,
   `105_774` in both export constants).
3. Red-first for the export move is easy and honest: grow
   `qtap_schema_validate_equivalence` with two JSON-Pointer mutation rows over a
   seed that carries a chat message (the corpus has **no** `pascalMeta` rows
   today — `grep pascalMeta|invokedBy` in the test and the oracle case: 0 hits):
   `message_pascal_meta_invoked_by_trigger_valid` (v4 NEW schema: valid; the
   stale v5 schema: INVALID -> RED before the re-vendor, GREEN after) and
   `message_pascal_meta_invoked_by_bogus_invalid` (both invalid; pins that the
   enum is still closed). Count deltas ajv-vs-crate are RECORDED, as the
   family's header already does for every earlier re-vendor.

If the planner prefers the Pascal lanes to own them, the cost is a certain
`public_schemas_vendor_guard` / `qtap_schema_embed_guard` double edit; the
vendor lane then must NOT touch the files. Pick one — do not split.

### 2.4 The move

```
cp-equivalent (byte-exact, from the object store, NOT the dirty tree):
  git -C ~/source/quilltap-server show 25b8fab6a:public/schemas/qtap-custom-tool.schema.json > apps/web/public/schemas/qtap-custom-tool.schema.json
  git -C ~/source/quilltap-server show 25b8fab6a:public/schemas/qtap-export.schema.json      > crates/quilltap-core/src/generators/qtap-export.schema.json
constants: public_schemas_vendor_guard.rs SCHEMAS[0].bytes 39_471 -> 43_493 (+ the doc comment line);
           qtap_schema_embed_guard.rs:72 105_310 -> 105_774 (+ a "P4.D… : 105,774 at v4 25b8fab6a" history line);
           qtap_schema.rs:195 105_310 -> 105_774.
checks: wc -c == 43493 / 105774; md5 == the table; cmp against `git show P:…`.
```
The build embeds the export schema, so `cargo build` recompiles `quilltap-core`
(`include_str!` dependency) — the 12 MB amalgamation (pinned `quilltap-sqlite3mc-sys`)
is NOT rebuilt. `apps/web/dist` needs the SPA build for the new asset to be
served by the e2e; the guard reads the source file, not `dist/`.

---

## 3. `docs/v4/` — the mirror move list (61 operations)

### 3.1 The mirror, measured at `56554d98c`

- Layout: `docs/<p>` (v4) -> `docs/v4/<p>`; `packages/quilltap/README.md` ->
  `docs/v4/packages-quilltap-README.md`. Top level of `docs/v4/`:
  `BACKUP-RESTORE.md, CHANGELOG.md, CHANGELOG_V0.md … CHANGELOG_V4.md,
  DEPLOYMENT.md, TEMPLATES.md, developer/, packages-quilltap-README.md,
  releases/`. **The mirror DOES track `CHANGELOG*.md`** (all six) — answers the
  ledger `1e1712a12` row's "IF it carries".
- `git ls-files docs/v4` = **484** = the pin-`83334b9a5` `docs/**` (483) + the
  README mapping (the P4.D303 census). `find docs/v4 -type f` = 487: the three
  extra are untracked, git-ignored `.DS_Store` (`docs/v4/`, `developer/`,
  `developer/features/`) — **any `find`-based count in the lane must exclude
  `.DS_Store` or use `git ls-files`**; a fresh lane worktree has none (P4.D303
  recorded 484 on disk).
- **Pre-state verified byte-for-byte:** every M source and every R/D old path in
  the table below `cmp`-equals `git show 83334b9a5:<path>` (loop run over the
  29 M, 17 R-old, 4 D paths + the README: 0 mismatches). So the mirror is
  exactly at B and the move is a pure B -> P delta.
- The mirror is a WHOLE-tree mirror, not "the paths a round needs" (ledger
  `b93c2efda` row says the latter — see §9): P4.D303's `U2` census asserted the
  same-SET property over all 483 paths.

### 3.2 The table (generated from `git diff -M --name-status B P -- docs packages/quilltap/README.md` + `git show P:` md5s)

Status: A added at the final path, D deleted, M modified, R renamed (the `R0nn`
similarity is v4's; every R except the four R100 needs the pin's bytes written
over the moved file). "Span commits" = `git log --reverse B..P -- <new path>`
(default history simplification; `docs/CHANGELOG.md` is touched by essentially
every commit — the list shown is the simplified one; **`docs/releases/4.10.0.md`
is also touched by `8489d08b9`** (the +128/-99 reorder; `git log --full-history`)
before `2432b9057`'s 1-line stamp). Counts: **11 A, 4 D, 17 R, 29 M (the 29th is
the CLI README mirror)**.

| # | st | v4 path (`docs/...` at the pin) | v5 mirror action (`docs/v4/...`) | pin bytes | md5 at pin | span commits |
|---|---|---|---|---|---|---|
| 1 | M | `CHANGELOG.md` | overwrite `docs/v4/CHANGELOG.md` | 15772 | `feea3b8e0a8cdb5ef936635e5ee8a0ea` | `2432b9057`, `37246654d`, `1e1712a12`, `7458ff94e`, `936a34720`, `362426c8f`, `f0db8cbb8`, `6069a7acd`, `4877fc2da`, `b93c2efda`, `eb5d42338`, `b2793400f`, `25b8fab6a` |
| 2 | M | `CHANGELOG_V4.md` | overwrite `docs/v4/CHANGELOG_V4.md` | 2283822 | `59ba857686e0fa6bc05fb2a9fe236413` | `8489d08b9`, `1e1712a12`, `7458ff94e` |
| 3 | A | `FEATURES.md` | NEW `docs/v4/FEATURES.md` | 27889 | `330ae077603d81b00a2d5b3114f68d1c` | `3f64d3a6c`, `362426c8f`, `f0db8cbb8` |
| 4 | M | `developer/API.md` | overwrite `docs/v4/developer/API.md` | 270510 | `bc3aae59b7304bddb8067becb44eccc1` | `8489d08b9`, `bbbcd2388`, `362426c8f`, `4877fc2da` |
| 5 | M | `developer/CUSTOM_TOOL_SPEC.json` | overwrite `docs/v4/developer/CUSTOM_TOOL_SPEC.json` | 10446 | `80fe7301712e318d5798e278304e866e` | `f0db8cbb8` |
| 6 | M | `developer/CUSTOM_TOOL_SPEC_DICE.json` | overwrite `docs/v4/developer/CUSTOM_TOOL_SPEC_DICE.json` | 1951 | `56a7f0d45ca8883418bd6e961bb29775` | `362426c8f` |
| 7 | M | `developer/DDL.md` | overwrite `docs/v4/developer/DDL.md` | 144934 | `64da390691c75e95f87550b1fb869998` | `8489d08b9`, `362426c8f`, `f0db8cbb8`, `4877fc2da` |
| 8 | M | `developer/DEAD-CODE-REPORT.md` | overwrite `docs/v4/developer/DEAD-CODE-REPORT.md` | 83905 | `07589c4cf6af089b8993b54a6f38174d` | `7417e9a1b` |
| 9 | M | `developer/DEVELOPMENT.md` | overwrite `docs/v4/developer/DEVELOPMENT.md` | 29094 | `27bd140709d389c09879b1bade9b9145` | `8489d08b9` |
| 10 | M | `developer/PROMPT_ARCHITECTURE.md` | overwrite `docs/v4/developer/PROMPT_ARCHITECTURE.md` | 34982 | `fd1210421d49d0c22429e24a0e3865e2` | `8489d08b9` |
| 11 | M | `developer/SYSTEM_PROMPT_PLUGIN_DEVELOPMENT.md` | overwrite `docs/v4/developer/SYSTEM_PROMPT_PLUGIN_DEVELOPMENT.md` | 29647 | `68b9ca43fbd3c689a9bafcf5d60a35b1` | `8489d08b9` |
| 12 | M | `developer/bugs.md` | overwrite `docs/v4/developer/bugs.md` | 380107 | `5888355be70e30fba0b64dbc26c7b945` | `8489d08b9`, `052659624`, `809e5104d`, `b82037d75`, `6069a7acd`, `b2793400f`, `25b8fab6a` |
| 13 | M | `developer/bugs/fixed/bug-202-anchor-hits-floored.md` | overwrite `docs/v4/developer/bugs/fixed/bug-202-anchor-hits-floored.md` | 2778 | `927ad7b1e4e352159ab5910a3af126f5` | `8489d08b9` |
| 14 | A | `developer/bugs/fixed/bug-247-month-name-prototype-key.md` | NEW `docs/v4/developer/bugs/fixed/bug-247-month-name-prototype-key.md` | 5969 | `62a3953bdeb433885833c423e07bef11` | `25b8fab6a` |
| 15 | A | `developer/bugs/fixed/bug-248-render-embed-one-behind.md` | NEW `docs/v4/developer/bugs/fixed/bug-248-render-embed-one-behind.md` | 5248 | `2c88e1fd438e0b51bbff3ba20a0a3dfb` | `b82037d75` |
| 16 | A | `developer/bugs/fixed/bug-249-lone-character-chain-runaway.md` | NEW `docs/v4/developer/bugs/fixed/bug-249-lone-character-chain-runaway.md` | 4562 | `4780a029321cedb0728155194815a48d` | `6069a7acd` |
| 17 | A | `developer/bugs/fixed/bug-250-typeahead-menu-orphaned.md` | NEW `docs/v4/developer/bugs/fixed/bug-250-typeahead-menu-orphaned.md` | 4658 | `4edc358bf393de0cb654521b7bfd1535` | `25b8fab6a` |
| 18 | A | `developer/bugs/fixed/bug-251-code-block-enter-sends.md` | NEW `docs/v4/developer/bugs/fixed/bug-251-code-block-enter-sends.md` | 3415 | `d2bc3e99e91d6f8a60610d7baba740e3` | `b2793400f` |
| 19 | M | `developer/bugs/fixed/bug-62-dialogue-fallback-quotes.md` | overwrite `docs/v4/developer/bugs/fixed/bug-62-dialogue-fallback-quotes.md` | 11831 | `a3e725203da3ea72b34fafb44b51e3bd` | `8489d08b9` |
| 20 | D | `developer/features/MCP.md` | `git rm docs/v4/developer/features/MCP.md` | — | - | `8489d08b9` |
| 21 | M | `developer/features/ROADMAP.md` | overwrite `docs/v4/developer/features/ROADMAP.md` | 7114 | `e252133b9b6b456e883a28663f466804` | `8489d08b9` |
| 22 | R099 | `developer/features/avatar-configuration-cache.md` -> `developer/features/complete/avatar-configuration-cache.md` | `git mv docs/v4/developer/features/avatar-configuration-cache.md docs/v4/developer/features/complete/avatar-configuration-cache.md`, THEN overwrite with the pin bytes | 12077 | `81d72ee48767ea2aa35191cbd58a2297` | `8489d08b9` |
| 23 | R099 | `developer/features/builtin-tfidf-memory-tuning.md` -> `developer/features/complete/builtin-tfidf-memory-tuning.md` | `git mv docs/v4/developer/features/builtin-tfidf-memory-tuning.md docs/v4/developer/features/complete/builtin-tfidf-memory-tuning.md`, THEN overwrite with the pin bytes | 13603 | `e0d526a4b259b57dd57179c53c7218db` | `8489d08b9` |
| 24 | M | `developer/features/complete/composer-emoji.md` | overwrite `docs/v4/developer/features/complete/composer-emoji.md` | 50938 | `d051e2952455bdae0c1555eb1764d68d` | `8489d08b9` |
| 25 | A | `developer/features/complete/composer-slash-commands.md` | NEW `docs/v4/developer/features/complete/composer-slash-commands.md` | 28910 | `3fcc50eadc8f0cebb6539360387dfdb0` | `4877fc2da` |
| 26 | R095 | `developer/features/composer-smart-typography.md` -> `developer/features/complete/composer-smart-typography.md` | `git mv docs/v4/developer/features/composer-smart-typography.md docs/v4/developer/features/complete/composer-smart-typography.md`, THEN overwrite with the pin bytes | 52911 | `a5ff5ab4186a127913974a6a01475357` | `8489d08b9` |
| 27 | R099 | `developer/features/composer-spellcheck.md` -> `developer/features/complete/composer-spellcheck.md` | `git mv docs/v4/developer/features/composer-spellcheck.md docs/v4/developer/features/complete/composer-spellcheck.md`, THEN overwrite with the pin bytes | 20532 | `cf13c4e502e0fc0fab7ff99a431ddf6d` | `8489d08b9` |
| 28 | R098 | `developer/features/composer-text-replacement.md` -> `developer/features/complete/composer-text-replacement.md` | `git mv docs/v4/developer/features/composer-text-replacement.md docs/v4/developer/features/complete/composer-text-replacement.md`, THEN overwrite with the pin bytes | 27029 | `76b2b184b08e67a610a824c7a3240192` | `8489d08b9` |
| 29 | M | `developer/features/complete/concierge-default-at-creation.md` | overwrite `docs/v4/developer/features/complete/concierge-default-at-creation.md` | 21087 | `301586c801da0774e0bc08162a0115e5` | `8489d08b9` |
| 30 | M | `developer/features/complete/concierge-four-state.md` | overwrite `docs/v4/developer/features/complete/concierge-four-state.md` | 20511 | `f5c057814234cb77c4ae795cb24c7fec` | `8489d08b9` |
| 31 | M | `developer/features/complete/concierge-list-marks.md` | overwrite `docs/v4/developer/features/complete/concierge-list-marks.md` | 25174 | `ca1f5ad1721b811c335f499b066b0866` | `8489d08b9` |
| 32 | R100 | `developer/features/concierge-overhaul-phase-1-refusal-failover.md` -> `developer/features/complete/concierge-overhaul-phase-1-refusal-failover.md` | `git mv docs/v4/developer/features/concierge-overhaul-phase-1-refusal-failover.md docs/v4/developer/features/complete/concierge-overhaul-phase-1-refusal-failover.md`, THEN overwrite with the pin bytes | 34338 | `c5c1ac1b7c7e2c1b5d530210feb4dd73` | `8489d08b9` |
| 33 | R100 | `developer/features/concierge-overhaul-phase-2-refusal-ledger.md` -> `developer/features/complete/concierge-overhaul-phase-2-refusal-ledger.md` | `git mv docs/v4/developer/features/concierge-overhaul-phase-2-refusal-ledger.md docs/v4/developer/features/complete/concierge-overhaul-phase-2-refusal-ledger.md`, THEN overwrite with the pin bytes | 19938 | `f28b84b9e334fe7b5063e7b77487623c` | `8489d08b9` |
| 34 | R099 | `developer/features/concierge-overhaul-phase-3-three-states.md` -> `developer/features/complete/concierge-overhaul-phase-3-three-states.md` | `git mv docs/v4/developer/features/concierge-overhaul-phase-3-three-states.md docs/v4/developer/features/complete/concierge-overhaul-phase-3-three-states.md`, THEN overwrite with the pin bytes | 23814 | `e1c1a298ee17fd66c2734d946cb7cb9d` | `8489d08b9` |
| 35 | R100 | `developer/features/concierge-overhaul-phase-4-concierge-tab.md` -> `developer/features/complete/concierge-overhaul-phase-4-concierge-tab.md` | `git mv docs/v4/developer/features/concierge-overhaul-phase-4-concierge-tab.md docs/v4/developer/features/complete/concierge-overhaul-phase-4-concierge-tab.md`, THEN overwrite with the pin bytes | 27195 | `9fa3b3ffb85a7f771776c44881d4e136` | `8489d08b9` |
| 36 | R100 | `developer/features/concierge-overhaul-phase-5-salon-polish.md` -> `developer/features/complete/concierge-overhaul-phase-5-salon-polish.md` | `git mv docs/v4/developer/features/concierge-overhaul-phase-5-salon-polish.md docs/v4/developer/features/complete/concierge-overhaul-phase-5-salon-polish.md`, THEN overwrite with the pin bytes | 16792 | `47b6e4c6ea5f791e2c9e1a3e4ed29c5b` | `8489d08b9` |
| 37 | R095 | `developer/features/concierge-overhaul.md` -> `developer/features/complete/concierge-overhaul.md` | `git mv docs/v4/developer/features/concierge-overhaul.md docs/v4/developer/features/complete/concierge-overhaul.md`, THEN overwrite with the pin bytes | 8196 | `9d4657e14480f6f42d82d2348ed01444` | `8489d08b9` |
| 38 | M | `developer/features/complete/dangerous.md` | overwrite `docs/v4/developer/features/complete/dangerous.md` | 8559 | `ccf9516a73f11d6abfbc0b8d1e1bc203` | `8489d08b9` |
| 39 | A | `developer/features/complete/dependency-upgrade-safe-batch.md` | NEW `docs/v4/developer/features/complete/dependency-upgrade-safe-batch.md` | 11179 | `ba2d35ba041afe6e400d8cb89a32b4db` | `eb5d42338`, `b2793400f` |
| 40 | A | `developer/features/complete/lexical-0.52-upgrade.md` | NEW `docs/v4/developer/features/complete/lexical-0.52-upgrade.md` | 22213 | `52c4a1aa0393f1c47f1b719b0008e1ad` | `b2793400f` |
| 41 | R099 | `developer/features/llm_api_costs_breakdown.md` -> `developer/features/complete/llm_api_costs_breakdown.md` | `git mv docs/v4/developer/features/llm_api_costs_breakdown.md docs/v4/developer/features/complete/llm_api_costs_breakdown.md`, THEN overwrite with the pin bytes | 35618 | `9d74907bd822bf99c1730b95df31a39c` | `8489d08b9` |
| 42 | M | `developer/features/complete/memory-recall-and-housekeeping-fixes.md` | overwrite `docs/v4/developer/features/complete/memory-recall-and-housekeeping-fixes.md` | 17748 | `1aad2e3d89bbfcac5637c01612d715a5` | `8489d08b9` |
| 43 | M | `developer/features/complete/memory-system-refactor.md` | overwrite `docs/v4/developer/features/complete/memory-system-refactor.md` | 209682 | `67fb59805ee8bad94d8c44849e187166` | `7417e9a1b` |
| 44 | M | `developer/features/complete/message-route-trail.md` | overwrite `docs/v4/developer/features/complete/message-route-trail.md` | 37441 | `0658ea0fa07d2158fcdc7510bb992a82` | `8489d08b9` |
| 45 | M | `developer/features/complete/pascal-custom-tool-enhancements.md` | overwrite `docs/v4/developer/features/complete/pascal-custom-tool-enhancements.md` | 29233 | `d110feb13f77ef34d73253eb3e239ebd` | `f0db8cbb8` |
| 46 | M | `developer/features/complete/pascal-custom-tools.md` | overwrite `docs/v4/developer/features/complete/pascal-custom-tools.md` | 69356 | `eb6705bbea6f5739131236f3ddc6bd5b` | `362426c8f`, `f0db8cbb8` |
| 47 | A | `developer/features/complete/pascal-trigger-phrases.md` | NEW `docs/v4/developer/features/complete/pascal-trigger-phrases.md` | 35440 | `3789aaca575fe75f324106377c8f646f` | `362426c8f`, `f0db8cbb8` |
| 48 | A | `developer/features/complete/pascal-wardrobe-effects.md` | NEW `docs/v4/developer/features/complete/pascal-wardrobe-effects.md` | 30796 | `bf6b9b88a74b686682bdf3d7cb80a1f5` | `f0db8cbb8` |
| 49 | M | `developer/features/complete/per-character-prompt-caching.md` | overwrite `docs/v4/developer/features/complete/per-character-prompt-caching.md` | 18419 | `70f5610928e3092c3207849705922cd9` | `8489d08b9` |
| 50 | R099 | `developer/features/prompt-trust-and-anti-committee.md` -> `developer/features/complete/prompt-trust-and-anti-committee.md` | `git mv docs/v4/developer/features/prompt-trust-and-anti-committee.md docs/v4/developer/features/complete/prompt-trust-and-anti-committee.md`, THEN overwrite with the pin bytes | 72569 | `e7174106cf3c06a8128c0ed92151d4dd` | `8489d08b9` |
| 51 | R098 | `developer/features/recall-multiplier-retuning.md` -> `developer/features/complete/recall-multiplier-retuning.md` | `git mv docs/v4/developer/features/recall-multiplier-retuning.md docs/v4/developer/features/complete/recall-multiplier-retuning.md`, THEN overwrite with the pin bytes | 15753 | `54f22b58b27d8cb4a79eafce8939c74e` | `8489d08b9` |
| 52 | R099 | `developer/features/recall-probe-set-runbook.md` -> `developer/features/complete/recall-probe-set-runbook.md` | `git mv docs/v4/developer/features/recall-probe-set-runbook.md docs/v4/developer/features/complete/recall-probe-set-runbook.md`, THEN overwrite with the pin bytes | 79382 | `7a1680faaca4bed952b7d52b16a82dfc` | `8489d08b9` |
| 53 | R097 | `developer/features/salon-inform.md` -> `developer/features/complete/salon-inform.md` | `git mv docs/v4/developer/features/salon-inform.md docs/v4/developer/features/complete/salon-inform.md`, THEN overwrite with the pin bytes | 34398 | `3f63075454bc4573e06a18102ce07241` | `8489d08b9` |
| 54 | R097 | `developer/features/scenario-builder.md` -> `developer/features/complete/scenario-builder.md` | `git mv docs/v4/developer/features/scenario-builder.md docs/v4/developer/features/complete/scenario-builder.md`, THEN overwrite with the pin bytes | 42469 | `0324972e71d5c4baa8c45b102912ef48` | `8489d08b9` |
| 55 | M | `developer/features/complete/wardrobe-folders.md` | overwrite `docs/v4/developer/features/complete/wardrobe-folders.md` | 33481 | `1aeeb513af4e51a79843ef30d26d6640` | `bbbcd2388` |
| 56 | M | `developer/features/composer-typeahead.md` | overwrite `docs/v4/developer/features/composer-typeahead.md` | 36864 | `2afd6d2c9681dea3e7bf9e73315e37b1` | `4877fc2da` |
| 57 | D | `developer/features/distant_future_separate_api.md` | `git rm docs/v4/developer/features/distant_future_separate_api.md` | — | - | `8489d08b9` |
| 58 | D | `developer/features/native-core-api-boundary.md` | `git rm docs/v4/developer/features/native-core-api-boundary.md` | — | - | `8489d08b9` |
| 59 | D | `developer/features/native-port-phase-0.md` | `git rm docs/v4/developer/features/native-port-phase-0.md` | — | - | `8489d08b9` |
| 60 | M | `releases/4.10.0.md` | overwrite `docs/v4/releases/4.10.0.md` | 78591 | `e71a4042aa9980cf7f0339b30364ec9d` | `2432b9057` |
| 61 | M | `packages/quilltap/README.md` | overwrite `docs/v4/packages-quilltap-README.md` | 45054 | `218e714723b98cc999b4169222a51a25` | `75d094e63` |

### 3.3 The four deletions — evidence and the proposed ruling

All four are removed by `8489d08b9` (a pure `D`, no rename target; `git diff -M`
finds no similar file): `docs/developer/features/MCP.md` (265 lines; "Feature
Request: Local File CRUD via MCP Server — Status: Proposal / Not Implemented"),
`distant_future_separate_api.md` (654), `native-core-api-boundary.md` (322),
`native-port-phase-0.md` (298). The last two are v4's copies of THIS port's
early design notes. All four exist in the v5 mirror at pre-bytes (`cmp` vs
`git show B:`; `native-port-phase-0.md` is 14,811 B, last re-vendored by
P4.D303).

Live references into them, measured at `56554d98c`: from `docs/v4/**` only
`native-port-phase-0.md:5` (a relative link to `native-core-api-boundary.md` —
both die together) and a prose mention in `DEAD-CODE-REPORT.md` (a path inside a
table cell, not a link); from v5 code / tests / SPA / porting docs: **none**
(grep over `crates`, `apps/web/src`, `harness`, `scripts`, `CLAUDE.md`,
`docs/developer/porting/*.md` outside status-log / work-orders / ledger prose).
v5's authoritative versions live under `docs/developer/porting/` (`api-boundary.md`,
`phase-0.md`, `overview.md`; `d6a6ae18c` moved them there and then pruned the
mirror copies once — they came back by whole-tree re-vendor).

**Ruling proposed: `git rm` all four.** Evidence for, from how earlier lanes
treated deletions:

- P4.D303 U1 `git rm`'d two `help/` pages and its `U2` census asserted the mirror
  is the SAME SET as the pin (+ the README mapping) — a stale file fails that
  census.
- P4.81 item 5 (`5e2882ec9`, 2026-09-07) pruned `docs/v4/WINDOWS.md` and
  `docs/v4/help/database-protection.md` — "mirror paths v4 dropped … confirmed
  absent in a worktree pinned at that sha". The earlier "additive + modified
  habit, left in place and named" (P4.D158/P4.59 note, status-log ~109556) was
  superseded by that prune.
- The mirror's purpose is "v4's tree at the pin" (CLAUDE.md: "the `docs/v4/`
  tree is the v4 reference mirror"). Keeping `MCP.md` as a live-looking
  feature spec v4 itself discarded misleads a reader; keeping the two `native-*`
  copies creates two divergent "authoritative" boundary docs.

Alternative the human could rule instead (record it, do not default to it):
leave the four in place and name them "recorded, not acted on" as P4.D158 did.
The census count in §8 changes by 4 (495 vs 491 tracked) if so.

### 3.4 Rest of the list — what is non-obvious

- **`FEATURES.md` (A, 27,889 B)** — `3f64d3a6c` (119 lines, "A Tour of the
  House: Quilltap's Features in Full") then edited by `362426c8f` and
  `f0db8cbb8` (Pascal triggers / effects). NEW `docs/v4/FEATURES.md`.
  Check `docs/v4` has no `FEATURES.md` today (it does not).
- **`CHANGELOG.md` (M, 305,625 -> 15,772 B) and `CHANGELOG_V4.md` (M, 1,968,587
  -> 2,283,822 B)** — `1e1712a12` moves the `### 4.9.2` section; `7458ff94e`
  moves ~3,600 more lines; `8489d08b9` touches `CHANGELOG_V4.md` too. v5's
  `docs/v4/CHANGELOG.md` is the *mirror* (the repo's own changelog is
  `docs/CHANGELOG.md`, unrelated). Both are plain overwrites with the pin's
  bytes. `releases/4.10.0.md` (78,591 B) too.
- **API.md / DDL.md** — doc refreshes only (`DDL.md` +4/-2: the
  `composerSlashCommands` and `customToolTriggers` column lines + two comment
  lines; these are the *documentation* of the ELEVENTH/TENTH D23 moves, not the
  moves — `fresh_schema.json` is re-dumped from v4's live `generateDDL` by the
  schema lanes and is **not** this lane's). `API.md` (+85/-18 net) carries the
  `copy-items` row, the trigger/slash/effects documentation, etc.
- **`CUSTOM_TOOL_SPEC.json` / `_DICE.json`** — specimens gain a `gloves`
  parameter + a `wardrobe` effect, and `"triggers": ["make a saving throw",
  "roll to save"]`. Referenced by no v5 test (grep 0).
- **Bug files** — `bugs/fixed/bug-247 … 251` are the five NEW files; the
  `bug-247` copy is the *fixed* text (Status FIXED, 5,969 B, 81 lines) — the
  interim OPEN `bugs/bug-247-*` (71 lines) never reaches the pin. `bug-62` and
  `bug-202` are link-path edits only (`8489d08b9`). `bugs.md` (M) lists 247–251.
- **`DEAD-CODE-REPORT.md` (M, +58/-7)** — `7417e9a1b`; it is the keep-list for
  the dead-code sweep and the human-readable twin of §4.1/§5.
- **`composer-typeahead.md` (M)** — `4877fc2da` (+7/-5) opens with "Superseded by composer-slash-commands.md" and strikes
  the slash file-touch rows.
- **The two `complete/` specs of this span's code** — `pascal-trigger-phrases.md`
  (35,440 B, 330 lines) and `pascal-wardrobe-effects.md` (30,796 B) are the
  design-of-record the Pascal lanes' orders will cite; `composer-slash-commands.md`
  (28,910 B) likewise for the slash-commands SPA lane. Vendoring them is
  independent of those lanes, so they are available at lane start.

**v4 dirt (observed 2026-10-16, after the pin was fixed):** `git status` in
`~/source/quilltap-server`: ` M .claude/commands/update-documentation.md`,
` M docs/developer/features/ROADMAP.md` (+1 line), `?? docs/developer/features/pascal-wardrobe-effect-picker.md`.
HEAD is still `25b8fab6a`, branch `main`. Only `ROADMAP.md` is a path this lane
writes (table row `developer/features/ROADMAP.md`, md5 `e252133b…` at P):
**vendor `git show 25b8fab6a:docs/developer/features/ROADMAP.md`, never the
working tree.** The untracked picker spec is NOT at the pin — do not vendor it
(it is the next v4 round's input; `/driftcheck` will see it when committed).

### 3.5 Recipe (P4.D303 U2's shape)

1. `git mv` the 17 renames first (old -> `complete/`), then overwrite each new
   path from `git show P:` (13 change bytes; the four R100 are no-ops but the
   overwrite is harmless). 2. `git rm` the 4 deletions (per the ruling). 3. The 29
   M and 11 A and the README, each `git -C ~/source/quilltap-server show
   25b8fab6a:<v4 path> > docs/v4/<mirror path>` (`mkdir -p` the 5 new
   `bugs/fixed/…` parents already exist; `FEATURES.md` lands at `docs/v4/`).
   4. Stage by explicit path (never `git add -A`: the three `.DS_Store`).
   5. Census (§8.3). The mirror contains one spaced filename
   (`developer/features/complete/Wardrobe UX Overhaul.md`, untouched this span)
   — quote paths / use NUL-safe loops (memory note `v4-path-quoting-traps`).

---

## 4. The NO-PORT evidence — one subsection per row, then per half

Method (the P4.D303 shape): `git show --stat <sha> -- lib app packages plugins
components help public migrations` for the code-path census, then the decisive
hunk, then the v5 site. "Code-path stat EMPTY" below means that command printed
nothing. Proposed disposition words are for the unifier's ledger step.

### 4.1 `7417e9a1b` — "Release checklist 5: dead code sweep" (whole row; **NO-PORT-RATIFIED, with a harness obligation**)

`git show --stat`: **21 files, +80 / -338**, parent `83334b9a5`, `4.10.0-dev.177`.
Code-path census: `lib/memory/index.ts` (-189) and `lib/memory/recall/index.ts`
(-47) DELETED; `wardrobe-writes.ts` -10; `extraction/prompts/shared.ts` -17/+;
`time/calendar.ts` -4; `mount-index/sync/manifest.ts` -5, `sync/types.ts` -8/+;
`progressions/engine.ts` -6; `schemas/memory-extraction-cursor.types.ts` -2;
`dangerous-content/refusal.ts` -5; tests `clock-zone.test.ts` (28), `refusal.test.ts`
-6, `chat-danger-trigger.test.ts` -5; `knip.json` -2; docs/stamps.

Decisive hunks (every `lib/` hunk is a deletion of an export with no production
caller; no body of any surviving function changes):

```
-export function createMountWardrobeItem(mountPointId: string, item: WardrobeItem): Promise<WardrobeItem> {
-  return createInMount(mountWardrobeLocation(mountPointId), item);
-export function deleteMountWardrobeItem(mountPointId: string, id: string): Promise<boolean> {
-export async function withMemoryTimeZone<C extends ExtractionClock | undefined>(clock: C): Promise<C> {
-  logger.debug('[Memory] Extraction clock zone resolved', { timeZone, nowIso: clock.nowIso })
-const DAY_MS = 24 * 60 * 60 * 1000          ... -export { DAY_MS as MEMORY_CALENDAR_DAY_MS }
-export function emptyBase(): Map<string, ManifestEntry> { return new Map(); }
-export type SyncDirection …  -export type SyncPreference …   (the zod schema keeps the same enums, doc comments moved inline)
-export interface ProgressionIssue { id: string; issue: string; }
-export const EXTRACTION_CURSOR_PASSES: readonly ExtractionCursorPass[] = ExtractionCursorPassEnum.options
-export function isModerationRefusal(error: unknown): boolean { return classifyRefusal({ error }).refused }
```

Production-caller proof at P: `git grep -n -w <name> 25b8fab6a -- . ':!docs'` for
each of the eleven names (the nine exports + `SyncDirection` / `SyncPreference`
+ the DEBUG string) returns **0 hits** — including tests. The barrels have no
importer: `git grep -nE "['\"]@/lib/memory(/recall)?['\"]"` at P hits only two
prose lines in `docs/CHANGELOG_V4.md:66` and `DEAD-CODE-REPORT.md:36` (the latter
says "Barrel with no importers anywhere — every caller imports the submodules
directly. Its only mention was a stale `jest.mock('@/lib/memory'…)`").
`withMemoryTimeZone`'s only call site was `clock-zone.test.ts`, rewritten in the
same commit to call `refreshMemoryTimeZone()` directly
(`clock = { …, timeZone: await refreshMemoryTimeZone() }`). The `knip.json` hunk
drops `"plugins/**"` from `ignore` and `"ps"` from `ignoreBinaries`.

v5 sites: no v5 production behaviour moves. v5's `[Memory] Extraction clock zone
resolved` — **0 hits** (never ported; `memory_extraction/prompts/shared.rs:16`
says "`withMemoryTimeZone` … has no production caller (work order P4.D296 L-8 —
not ported)", now double-confirmed). v5's own same-named items survive as v5
code and need no change: Rust `MEMORY_CALENDAR_DAY_MS` (`memory_time/calendar.rs:35`,
used at :156/:243), `SyncDirection` / `SyncPreference` enums (`mount_index/sync/types.rs:187/:215`),
SPA `ProgressionIssue` (`apps/web/src/app/progressions/engine.ts:116`),
`is_moderation_refusal` (`dangerous_content/refusal.rs:381` — **test-only** in v5
since v4 deleted its twin; optional YAGNI follow-up, not an obligation). **The
harness obligation — IMPORT-DEAD — is §5.** Proposed disposition:
`NO-PORT-RATIFIED(<round>)` for production; the oracle repair rides this lane.

### 4.2 `052659624` + `809e5104d` — bug 247's filing (ratify WITH the fix row `25b8fab6a`)

`052659624`: **2 files, +74 / -2** (`bugs/bug-247-month-name-prototype-key.md` NEW,
71 lines; `bugs.md` +3/-2). `809e5104d`: **2 files, +4 / -3**. Code-path stat
EMPTY for both. Decisive text (from the file at `052659624`, Status OPEN): "**Found**
2026-10-13, the v5 port's differential harness (P4.D289, the `memory_time` family
at the `83334b9a5` pin)"; "**Provenance** Original to v4"; defect site
`lib/memory/time/clock.ts:94-97` (`MONTH_NAMES` a `Record<string, number>`
literal inheriting `Object.prototype`) and `:160-165`. `809e5104d`'s only change
corrects the throw site: "`iso(NaN)` — `new Date(NaN).toISOString()` — throws"
-> "`offsetMs` → `zonedParts` hands `new Date(NaN)` to
`Intl.DateTimeFormat#formatToParts`, which throws (`clock.ts:167` → `calendar.ts:152`
→ `:134` → `:120`, measured at `8489d08b9`)". At the pin the file is
`bugs/fixed/bug-247-…md` (Status FIXED, Fixed 2026-10-16, "Fix site: `lib/memory/time/clock.ts`
(`lookupWord`, own keys only…); `lib/memory/time/anchors.ts` (`resolveEpisodicAnchors`
never throws)"); the interim OPEN copy never reaches the pin, so the mirror
gets only the `fixed/` text (§3).

v5: `memory_time_equivalence.rs:317` `PROTOTYPE_KEY_MONTH_THROWS = &["constructor 5",
"5 constructor"]` — **the pin trips by design** at P (the CONVERGENCE row
`25b8fab6a`, owned by the memory-time lane; the bug file lists FIVE shapes —
`"constructor 5"`, `"5 constructor"`, `"on constructor 5"`, `"constructor 5th"`,
`"constructor 5, 2026"` — and v4's new test table adds `a constructor ago` /
`two constructor ago`). Two small findings for that lane: v5's doc comment at
`:309` ALREADY says `formatToParts` (so the ledger's "should name formatToParts,
not toISOString" is moot — v5 never quoted the wrong mechanism); and the same
comment cites the OPEN path `docs/developer/bugs/bug-247-month-name-prototype-key.md`
— update it to `…/bugs/fixed/…` (or drop the cite) when the table retires.
Disposition: both filing rows `NO-PORT-RATIFIED`, riding the convergence row.

### 4.3 `3f64d3a6c` — README rewrite + `docs/FEATURES.md` (**NO-PORT-RATIFIED**; mirror gains one file)

`--stat`: **5 files, +190 / -214** (`.claude/commands/update-documentation.md`
+2/-1, `CLAUDE.md` +2/-2, `README.md` +61/-211, `docs/CHANGELOG.md` +6, NEW
`docs/FEATURES.md` +119). Code-path stat EMPTY; no `package.json` / badge hunk
(the batch's `4.10.0-dev.185` stamp is `b82037d75`'s). Decisive hunks: `CLAUDE.md`
"**Databases are SQLCipher-encrypted** … Use `npx quilltap`" -> "**Databases are
encrypted (ChaCha20 via SQLite3 Multiple Ciphers — not SQLCipher, despite the
identifiers)**"; `README.md:207` at P "encrypted at rest with ChaCha20-Poly1305
(via SQLite3 Multiple Ciphers)". `docs/developer/DATABASE_ENCRYPTION.md` is
NOT touched (still **13** case-insensitive "sqlcipher" hits at P) — so the
mirror's copy keeps asserting what this repo's CLAUDE.md calls wrong; unchanged
standing fact. v5: only effect is `docs/v4/FEATURES.md` (A) and `docs/v4/CHANGELOG.md`
(§3). Optional one-line note in `phase-0.md` (not a code row): "v4's README and
CLAUDE.md say ChaCha20 as of `3f64d3a6c`; `DATABASE_ENCRYPTION.md` does not."

### 4.4 `2432b9057` — `release: 4.10.0` (**NO-PORT-RATIFIED**)

First parent `8fbf2afe0` = "release: 4.9.2" (2026-09-06): `git diff --stat
2432b9057^ 2432b9057 -- lib app packages plugins` = **1,013 files, +215,058 /
-75,729** (the whole 4.10 cycle, each already a ledger row or §6 entry — NOT new
content). Against main's pre-merge tip: `git diff --stat 3f64d3a6c 2432b9057` =
**5 files, +6 / -6** — `README.md` badge `4.10.0--dev.185 yellow` -> `4.10.0 green`,
`docs/CHANGELOG.md` `### 4.10-dev` -> `### 4.10.0`, `docs/releases/4.10.0.md`
`pubDate: 2026-10-13` -> `2026-10-14`, `package.json` + lock
`4.10.0-dev.185` -> `4.10.0`; code-path diff against `3f64d3a6c` EMPTY. Nothing
reads v4's `package.json` version in v5 (the oracle's `V4_APP_VERSION` stamp is
the sweep's per-baseline literal, a drift-round chore).

### 4.5 `f573a7576` — `merge: 4.10.0 back into main` (**NO-PORT-RATIFIED**)

Two parents (`3f64d3a6c`, `2432b9057`); `--stat` 5 files +6/-6 (the same five
stamp files vs the first parent); `git show --cc f573a7576` -> **0 lines** (the
combined diff is empty); `git diff --stat f573a7576 2432b9057` equals
`f573a7576 3f64d3a6c`'s five-file stat. Nothing.

### 4.6 `37246654d` — `dev: started 4.11.0 development` (**NO-PORT-RATIFIED**)

`--stat` **4 files, +6 / -4**: `README.md` badge `4.10.0` -> `4.11.0--dev.0`
(yellow), `docs/CHANGELOG.md` `+### 4.11-dev` + blank line, `package.json` +
lockfile `"version": "4.10.0"` -> `"4.11.0-dev.0"`. Hunks quoted in full from
`git show 37246654d -- docs/CHANGELOG.md README.md package.json`. From here
v4 stamps `4.11.0-dev.N` (the pin is `.10`).

### 4.7 `1e1712a12` — `docs(CHANGELOG): moved 4.9.x changes to v4 docs` (**NO-PORT-RATIFIED**; mirror overwrite)

`--stat` **2 files, +57 / -57**: the `### 4.9.2` section (bugs 124/125, both
absorbed long ago) removed from `docs/CHANGELOG.md` and inserted verbatim under
`## Historical Changes` in `docs/CHANGELOG_V4.md` (`1e1712a12:docs/CHANGELOG_V4.md`
opens "# Changelog Changelog v4.x / ## Historical Changes / ### 4.9.2"). The
mirror carries both files (§3.1) -> two plain overwrites (rows 1–2).

### 4.8 `936a34720` — three NEW proposal specs (**NO-PORT-RATIFIED as a row; the specs are PORT inputs**)

`git show --stat 936a34720 -- lib app packages plugins components help public
migrations` -> **EMPTY** (verified). `--stat`: **5 files, +843 / 0**:
`docs/developer/features/pascal-trigger-phrases.md` (305), `pascal-wardrobe-effects.md`
(266), `composer-slash-commands.md` (262), CHANGELOG +7, the `update-documentation`
command +3. All three become `complete/` specs of PORT-NEW rows the same
day (`362426c8f`, `f0db8cbb8`, `4877fc2da`) — the pin's copies are the
*implemented* revisions (330 / 283 / 278 lines), which is what the mirror gets.
Nothing to compare.

### 4.9 `b93c2efda` — specs for the dependency safe batch + Lexical 0.52 (**NO-PORT-RATIFIED**)

Code-path stat EMPTY. `--stat` **3 files, +319**: `features/dependency-upgrade-safe-batch.md`
(135), `features/lexical-0.52-upgrade.md` (180), CHANGELOG +4. Moved to
`complete/` by `eb5d42338` (the first) and `b2793400f` (the second, +19 edits) —
net A at `complete/` (§3). Ledger row says "v5 vendors only the `docs/v4/`
paths a round needs": **wrong** — the mirror is whole-tree (§9).

---

### NO-PORT HALVES inside PORT rows

### 4.10 `75d094e63` — `extractMigrationId` (the `migrations status` fix) is NO-PORT until v5 has a migration runner

`--stat` **11 files, +101 / -16**. The hunk (`packages/quilltap/lib/migrations-commands.js`
+22/-4): NEW `extractMigrationId(fileContent)` — `const MIGRATION_ID(: T)? = '…'`
paired with `^\s*id:\s*MIGRATION_ID\b` wins, else the old `^\s*id:\s*['"]…['"]`
literal, else `null`; exported for the new
`packages/quilltap/lib/__tests__/migrations-source-ids.test.js` (54 lines).
v5 confirmation: `grep -rn "extract_migration_id\|extractMigrationId\|migrations_cmd"
crates` -> **0 hits**; `crates/quilltap-cli/src/` has no `migrations_cmd.rs`
(files: `anchor_probe_cmd, completion_cmd, db_characters, db_cmd, dbopen,
docs_cmd, http, instances_cmd, main, nodefmt, out, recall_replay_cmd, resolve,
restore_key, sync_cmd, sync_report, vtable`); `main.rs` lists `migrations` in
`SUBCOMMANDS` but routes it to `not_yet_available`; the runner is deferred
(CLAUDE.md "The migration runner stays deferred"). **Carry by name:** when a
v5 `migrations status` arrives it must read BOTH id forms from day one. The PORT
halves of this row (main-help line, zsh / fish templates, the CLI README
mirror) belong to the CLI lane; the README bytes are in §3 row 61.

### 4.11 The two `prettify.ts` labels — `362426c8f` and `4877fc2da` (NO-PORT per the standing prettify ruling)

`git diff --stat B P -- lib/startup` = `lib/startup/prettify.ts | 2 ++`. Hunks:
```
362426c8f:  +  'add-custom-tool-triggers-field-v1': 'Teaching Pascal which words to listen for',
4877fc2da:  +  'add-composer-slash-commands-field-v1': 'Fitting the composer with a speaking-tube to the tool room',
```
into `PRETTY_LABELS` (`lib/startup/prettify.ts:22`, consumed by `prettify()` :324).
**The standing ruling** (find it): "the `prettify` label a third-time NO-PORT"
(`phase-4.md:13015`, P4.D179's round), `work-orders/p4.6ay-pascal-custom-tools-server.md:1434-1436,
:1566` ("v5 has no prettify subsystem at all … The whole subsystem is
unported; this is not a Pascal problem"), and the code-side record
`crates/quilltap-harness/tests/folders_chokepoint_wiring_guard.rs:123-126`
(`NO_V5_COUNTERPART`: "`lib/startup/prettify.ts PRETTY_LABELS` — v4's
migration-runner progress screen has no v5 analogue — the standing deliberate
non-port recorded at `db/chat_activity_recompute_heal.rs`"). Measured at
`56554d98c`: `grep -rn PRETTY_LABELS crates apps/web/src` -> **3 hits, all
comments** (`files_sha256_realign_heal.rs:77`, `p4d288_span.rs:17`, the guard
above); no label map exists. **Two** new lines, both NO-PORT by that ruling.

### 4.12 `b2793400f` — Lexical 0.52 internals (the NO-PORT half; bug 251 is the PORT half, not this lane's)

`--stat` **19 files, +981 / -293** (`package.json` 20, `package-lock.json` 633,
`jest.config.ts` 25, `KeyboardPlugin.tsx` 105, `table-transformer.ts` 13,
`CharTypeaheadPlugin.tsx` 3, `ImagePastePlugin.tsx` 4, `MentionTypeaheadPlugin.tsx`
6, two NEW test files 194 + 108, docs, `help/chats.md` +2). The Lexical-only hunks
(NO-PORT, v5 is ProseMirror, D17): the nine-package 0.43 -> 0.52 lockstep; type
fixes `(event: KeyboardEvent | null) => { if (event === null) return false` ->
`(event: KeyboardEvent) =>` (CharTypeahead), `(event: ClipboardEvent) =>
event.clipboardData?.items` -> `(event) => 'clipboardData' in event ? … : undefined`
(ImagePaste); `jest.config.ts`'s ESM exemption; `table-transformer.ts`'s bare
`regExpEnd`. Bug 251 (`KeyboardPlugin.tsx` `$codeBlockToEscape`: element-point
caret after Shift+Enter, and the exit trim "remove trailing whitespace-only text
nodes then ONE line break") is the PORT half for `exitCodeBlockOnBlankLine`;
the element-point caret is Lexical-only, the narrowed trim is not.
Installed confirmation (live checkout): `node_modules/lexical` and
`@lexical/react` are `0.52.0` — consistent with the lockfile at P.

### 4.13 `25b8fab6a` — bug 250 `GuardedTypeaheadMenuPlugin` (the NO-PORT half; bug 247 is the CONVERGENCE half)

`--stat` **17 files, +207 / -31**. Bug 250 hunks: NEW
`components/chat/lexical/typeahead/GuardedTypeaheadMenuPlugin.tsx` (+81) and
three one-line swaps (`LexicalTypeaheadMenuPlugin` -> `GuardedTypeaheadMenuPlugin`)
in `CharTypeaheadPlugin.tsx`, `MentionTypeaheadPlugin.tsx`, `SlashCommandPlugin.tsx`.
Decisive text (the file's own doc and `bugs/fixed/bug-250-…md`): "The plugin
opens its menu inside `startTransition` … `closeTypeahead` … in Lexical 0.52
returns without closing … the menu sits at the window's top-left until the next
keystroke"; provenance "Upstream, new in our build with the Lexical 0.43 -> 0.52
upgrade. Confirmed absent on 0.43"; **"v5 status: Not applicable (v5 does not
use Lexical)"**. The wrapper emits `console.debug('[Typeahead] Closing a menu
whose trigger vanished before it opened')` — v5: 0 hits, no analogue. v5's
typeaheads are ProseMirror plugins (`apps/web/src/app/editor/char-insert/`).
The bug-247 half (`lookupWord` + `resolveEpisodicAnchors` try/catch) is §4.2's
convergence.

### 4.14 `8489d08b9` — the comment-path renames in `lib/` (the NO-PORT remainder of a PORT+docs row)

`--stat` **72 files, +357 / -1,793**. Code-path census (`lib app migrations
eslint.config.mjs __tests__`): **24 files, +27 / -27**, of which `app/about/AboutView.tsx`
(+4/-4 — the four About blurbs, the PORT half owned by the About-page lane) and
**23 one-line comment edits** `docs/developer/features/<x>.md` ->
`features/complete/<x>.md` (verified: stripping every `+/-` line that mentions
`docs/developer/features` leaves exactly the eight AboutView lines). Files (eleven of the 23 are tests / READMEs):
`inform-block.ts`, `user-narration-anchor.ts`, `builtin-refit-policy.ts`,
`avatar-cache.ts`, `cache-key.ts`, `extraction/prompts/shared.ts`, `recall/tags.ts`,
`recall/tuning.ts`, `scenario-builder.service.ts`, `scenario-builder/route.ts`,
`migrations/scripts/collapse-duplicate-avatar-rolls-v1.ts` (comment only — no
migration logic, no DDL), `eslint.config.mjs`, and the tests / READMEs.
v5's carried why-comments that cite the OLD `docs/developer/features/<x>.md`
paths are v5 prose, no obligation (the ledger says 39 files; leave or tidy).

### 4.15 `01f508d4e` — the SPA `console.debug` (the one line v5 HAS)

`--stat` **9 files, +12 / -17**; the SPA hunk (`components/wardrobe/hooks/useWardrobeItemActions.ts`):
```
-      console.debug('[useWardrobeItemActions] Duplicating wardrobe item', {
-        sourceId: item.id, targetScope: home.scope, targetId: home.id,
-        newTitle: title, componentItemIds: item.componentItemIds,
-      })
```
**v5 has the line:** `apps/web/src/app/wardrobe/wardrobe-item-actions.ts:231` and the spec asserts
it (`wardrobe-item-actions.spec.ts:157` `expect(debug).toHaveBeenCalledWith('[useWardrobeItemActions]
Duplicating wardrobe item', {…})`, test title "logs v4's debug line"). It is a
browser-console line, in no log census; porting v4's removal is a two-edit SPA
change (delete the call, delete the spy + assertion + retitle the test). Not a
vendor-lane item — hand it to whichever lane owns the log-lines row (`01f508d4e`).
The three server-side log changes of that row (`[Transcript] Projecting chat
transcript`, `[Sync] Reading disk file`, the two Post Office labels) are that
lane's: v5 carries `[Sync] Reading disk file` at
`services/mount_index/sync/apply_disk.rs:96,104,398` and `discardLetter` /
`markMailAlertsDelivered` at `post_office/mailbox.rs:441,679`.

### 4.16 `49f8906c3` — `restore-field-fidelity.test.ts` is tests-only (and is NOT new)

`--stat` **8 files, +201 / -6**; the "audited every table and column" claim ships
no backup/restore code: `__tests__/unit/lib/backup/restore-field-fidelity.test.ts`
**+107** and `uuid-remapper.test.ts` +43 (tests), `lib/backup/restore/uuid-remap.ts`
+41/-1 (the one code hunk: `PARTICIPANT_ID_JSON_FIELDS` + `remapJsonIdList` — the
PORT row's, `backup/uuid_remap.rs`). `git cat-file -e 49f8906c3^:__tests__/unit/lib/backup/restore-field-fidelity.test.ts`
succeeds -> the file **pre-existed and is MODIFIED** (the ledger says "NEW, 107
lines" — wrong, §9). The CHANGELOG's deliberate NON-carry statements (FTS index,
`chats.transcriptVersion`, the Concierge refusal ledger) are statements of
existing behaviour (no hunk). NO-PORT half: the test file (v5's equivalence for
restore is the differential family, not a port of v4's mocks).

### 4.17 `a8b2d2861` — the "plugin self-containment" audit is prose

`--stat` **31 files, +257 / -95**. Enforcement hunk for "plugin self-containment":
**none** — the only source hunk is `packages/plugin-utils/src/providers/openai-compatible.ts`
(+58/-16, `normalizeUsage`) with its three plugin bundles (`openai-compatible`,
`deepseek`, `nanogpt` `index.js` +49/-15 each) and the dependency stamp move (§7).
The `plugins/dist/*/package.json` moves are `@quilltap/plugin-utils` `^2.6.2` ->
`^2.7.1` in 14 files plus version stamps. Behaviour change is the provider lane's PORT row.


---

## 5. The IMPORT-DEAD check (the ledger's `7417e9a1b` ask) — POSITIVE

**Verdict: the round's regens die at the pin, in four distinct ways, unless the
`harness/oracle` side is repaired first.** Method: for each deleted name and for
each barrel specifier, `grep -rn` (aliased to `ugrep` on this machine; plain
`-w` for names) over `harness/`, `crates/`, `apps/web/src`, `scripts/`
(excluding `node_modules`, `target`, `sweep-results`), plus the exact-specifier
regexes `['"\`]@/lib/memory['"\`]`, `['"\`]@/lib/memory/recall['"\`]`,
`lib/memory/index['"\`.]`, `lib/memory/recall/index['"\`.]`. A Python scan with a
multi-line-aware regex then listed every `import { … } from
'@/lib/database/repositories/vault-overlay/wardrobe-writes'` and its
`await import(…)` twin. Because the repo's oracle modules reach v4 through
dynamic `await import('@/…')` (CJS interop under `tsx` / jest), a name that is
no longer exported does **not** fail at import — it is `undefined`, and the case
dies at its first *use*.

### 5.1 Every hit, with file:line

**A. Barrel specifier** (`lib/memory/index.ts` deleted) — 1 hit:

| file:line | text | failure |
|---|---|---|
| `harness/oracle/cases/danger-trigger.test.ts:163-166` | `jest.doMock('@/lib/memory', () => ({ processMessageForMemoryAsync: () => {}, processInterCharacterMemoryAsync: () => {}, }));` | a non-virtual `jest.doMock` of a specifier that no longer resolves is expected to throw at the `doMock` call inside `runCase` (every case; not executed here — read-only survey) — v4's OWN commit deleted the byte-identical 4-line block from `__tests__/unit/services/chat-danger-trigger.test.ts` (`-jest.mock('@/lib/memory', () => ({ processMessageForMemoryAsync: jest.fn(), processInterCharacterMemoryAsync: jest.fn() }));`) for that reason. Consumer: `crates/quilltap-harness/tests/danger_trigger_equivalence.rs` (recipe header :32+). |

`@/lib/memory/recall` / `lib/memory/index` / `lib/memory/recall/index`: **0 hits**
in `harness/`, `crates/`, `apps/`. (Subpaths — `@/lib/memory/index-store`,
`/recall/assemble`, `/recall/deliver`, `/recall/render`, `/recall/query`,
`/recall/tags`, `/recall/tuning`, `/extraction`, `/admission/*` — are real modules
that still exist; ~50 oracle imports of them are unaffected. `@/lib/memory/extraction`
resolves to `extraction/index.ts`, which is NOT deleted.)

**B. `createMountWardrobeItem`** (`wardrobe-writes.ts`) — **12 import sites in 11
fixture builders**, every one `const { createMountWardrobeItem[: alias] } = await
import('@/lib/database/repositories/vault-overlay/wardrobe-writes')`:

| builder (`harness/oracle/fixtures/`) | import line | alias | call lines | recipe families that run it (Rust tests) |
|---|---|---|---|---|
| `build-chat-create-capstone.ts` | :426 | `createProjectWardrobeItem` | :430 | `chat_create_capstone_equivalence` |
| `build-chat-dialogs-fixture.ts` | :875 | `createProjectWardrobeItem` | :885 | `chat_export_equivalence` |
| `build-groups-projects-fixture.ts` | :574 | `createProjectWardrobeItem` | :580, :596 | **none** (committed fixture; no recipe invokes it) |
| `build-project-roster-access-fixture.ts` | :271 | `createProjectWardrobeItem` | :275 | `project_roster_access_equivalence` |
| `build-wardrobe-item-image-generation-fixture.ts` | :251 | `createProjectWardrobeItem` | :274 | `wardrobe_item_image_generation_tier3_equivalence`, `image_attempt_tier3_equivalence`, `generated_file_row_equivalence` |
| `build-wardrobe-item-images-fixture.ts` | :275 | `createProjectWardrobeItem` | :301 | `wardrobe_item_images_tier2_equivalence`, `photo_album_options_equivalence` |
| `build-wardrobe-keystone-fixture.ts` | :146 | (none) | :275, :276, :327, :328, :339, :340 | `wardrobe_location_tier2_equivalence`, `wardrobe_folder_writes_tier2_equivalence`, `vault_projection_recursive_tier2_equivalence`, `wardrobe_folders_routes_equivalence` (quilltap-web) |
| `build-wardrobe-routes-fixture.ts` | :273 | `createProjectWardrobeItem` | :277, :302 | **none** (committed fixture) |
| `build-wardrobe-tools-fixture.ts` | :298 | `createProjectWardrobeItem` | :330 | `wardrobe_tools_equivalence`, `wardrobe_tools_avatar_trigger_equivalence`, `wardrobe_wear_commit_tier2_equivalence` |
| `build-wardrobe-transfers-fixture.ts` | :343 and :353 | `createProjectWardrobeItem`, `createIntoMount` | :356, :374, :530 | `wardrobe_transfers_tier2_equivalence`, `group_wardrobe_routes_equivalence` |
| `build-wearable-pool-fixture.ts` | :295 | (none) | :18 (comment), :300 | `wearable_pool_tier2_equivalence` |

**18 families** (the union of the right column) have a *builder step* in their
recipe header, run by the sweep driver's regen stage; at the pin each builder
reaches its first `createProjectWardrobeItem(...)` / `createMountWardrobeItem(...)`
and dies with `TypeError: … is not a function`, so the family reports
`regen_failed`. The wardrobe copy-to-folder lane (`bbbcd2388`: families
`wardrobe_folders_routes_equivalence`, `wardrobe_location_tier2`, the transfers
pair) and the hair-slot lane (`7458ff94e`: `wearable_pool`, `wardrobe_tools`,
the Lantern/appearance families) **cannot regen at the pin without this repair**.
(`wardrobe-keystone-tier2.json:21` also contains the string "createMountWardrobeItem"
as prose inside a committed fixture description — harmless, do not edit.)

**C. `MEMORY_CALENDAR_DAY_MS`** (`time/calendar.ts`) — 1 import + 1 emit:

| file:line | text | failure |
|---|---|---|
| `harness/oracle/cases/memory-time.ts:39` | `MEMORY_CALENDAR_DAY_MS,` in the `import { … } from '@/lib/memory/time/calendar'` list | `undefined` |
| `harness/oracle/cases/memory-time.ts:147` | `emit({ kind: 'constants', MEMORY_CALENDAR_DAY_MS });` | `JSON.stringify` drops the `undefined` key -> row `{"leg":…,"kind":"constants"}` |
| `crates/quilltap-harness/tests/memory_time_equivalence.rs:178-182` (+ the row-count table :220 `("constants", 1)`) | `row["MEMORY_CALENDAR_DAY_MS"].as_i64().unwrap()` | **panics on `Null`** -> `memory_time_equivalence` RED (not skipped) |

**D. `isModerationRefusal`** (`refusal.ts`) — 1 import + 1 call + 1 emitted key:

| file:line | text | failure |
|---|---|---|
| `harness/oracle/cases/refusal-classify.test.ts:205` | `const { classifyRefusal, isModerationRefusal } = await import('@/lib/services/dangerous-content/refusal');` | `isModerationRefusal` is `undefined` |
| `…:223` | `const isRefusal = c.error === null ? null : isModerationRefusal(error);` | `TypeError: isModerationRefusal is not a function` on the first case with an error -> the whole jest test fails, no NDJSON |
| `…:236` | `isModerationRefusal: isRefusal,` (the emitted key) | consumed by `refusal_classify_equivalence.rs:207-216` (`row["isModerationRefusal"].as_bool()` vs v5 `is_moderation_refusal`) |

**E. Deleted names with NO oracle/harness hit** (measured `grep -w`): 
`deleteMountWardrobeItem` 0; `emptyBase` 0; `SyncDirection` / `SyncPreference`
0 in `harness/` (the `crates/**` hits are v5's OWN Rust enums); `ProgressionIssue`
0 in `harness/` (SPA's own interface); `EXTRACTION_CURSOR_PASSES` 0 (a doc
comment at `crates/quilltap-core/src/db/memory_extraction_cursors.rs:88`; the
oracle case `memory-extraction-cursors-tier2.ts:97` imports other names from
that module); `withMemoryTimeZone` 0 (comment at `memory_extraction/prompts/shared.rs:16`).
`harness/oracle/providers/record-text-errors.mjs:198` and `record-stream-fixtures.mjs:249`
load `refusal.ts` but use `classifyRefusal` only (`grep -w isModerationRefusal` 0).
Not deletions despite the `-export` lines in the diff: `CustomToolEffectSchema` and
`isApplicableEffect` are *re-declared* by `f0db8cbb8` (still exported at P:
`lib/pascal/custom-tool.types.ts:690`, `custom-tools.ts:1173`) — the Pascal
lane's, not IMPORT-DEAD.

### 5.2 The repairs (all `harness/oracle`, ZERO `crates/*/src` hunks)

1. **Builders (11 files, 12 sites):** replace each destructure with v4's own
   removed one-liner built from the two exports that still exist at P
   (`createInMount` `wardrobe-writes.ts:209`, `mountWardrobeLocation` :110):
   ```ts
   const { createInMount, mountWardrobeLocation } = await import(
     '@/lib/database/repositories/vault-overlay/wardrobe-writes'
   );
   const createMountWardrobeItem = (mountPointId: string, item: never) =>
     createInMount(mountWardrobeLocation(mountPointId), item);
   ```
   (keep the `: createProjectWardrobeItem` / `: createIntoMount` aliasing where the
   file used it — assign the arrow to that name). This is the removed function
   verbatim (`7417e9a1b^:wardrobe-writes.ts:647-650`), calling the real v4
   code — nothing reimplemented. The builders are self-contained (no relative
   imports), so inline per site; do NOT add a shared helper unless the lane
   wants one (`harness/oracle/lib/` exists but no builder imports from it).
2. **`danger-trigger.test.ts:163-166`:** delete the `jest.doMock('@/lib/memory', …)`
   block (mirrors v4's own hunk). No other line of the case references those two
   functions (`processMessageForMemoryAsync`/`processInterCharacterMemoryAsync`
   appear only inside the block).
3. **`refusal-classify.test.ts:223`:** `isModerationRefusal(error)` ->
   `classifyRefusal({ error }).refused` (v4's removed body, through the real
   classifier; keep the emitted `isModerationRefusal` key so the Rust arm is
   unchanged) and drop the name from the destructure at :205. The extra call's log
   lines are cleared by the next iteration's `logs.length = 0` exactly as the old
   call's were. Add a one-line comment citing `7417e9a1b`.
4. **`memory-time.ts:39,147`:** v4 no longer exports the constant. Preferred:
   derive it by MEASUREMENT through the real functions still exported —
   `zonedTimeToUtcMs(2026, 1, 2, 0, 'UTC') - zonedTimeToUtcMs(2026, 1, 1, 0, 'UTC')`
   (= 86400000) — and keep the row and the Rust arm; alternative (also fine,
   record it): drop the row and the `"constants"` arm. **File-ownership
   warning:** `memory-time.ts` and `memory_time_equivalence.rs` are ALSO the
   bug-247 convergence lane's files (`PROTOTYPE_KEY_MONTH_THROWS`, corpus growth).
   The edits are far apart (constants at :39/:147 vs the corpus), so a textual
   conflict is unlikely, but the order text should name both lanes on the file.

### 5.3 Ordering consequence for the round

These repairs unblock *other* lanes' pin-regen, so they cannot wait for the
vendor lane's unification. Two safe shapes: (a) a tiny stage-A order (this lane's
U1, landing first) that every other lane rebases on; or (b) fold the repair
into the first stage-A lane and name it in §R. The mandate's "decides whether the
round's regens die at the pin" is answered: **yes — 18 families + 3 cases**
(`danger_trigger_equivalence`, `refusal_classify_equivalence`,
`memory_time_equivalence`).

---

## 6. v5 today (measured at `56554d98c`)

- **`help/`**: 131 files at the repo root, all byte-equal to `git show 83334b9a5:help/<f>`; the name set equals B's. Embedded by `crates/quilltap-host/build.rs`
  into `help_content::EMBEDDED_HELP` (re-built whenever `help/**` changes —
  P4.D303 recorded the `Compiling quilltap-host` re-embed).
- **Count literals:** `help_tree_embed_guard.rs` `VENDORED_FILE_COUNT: usize = 131`
  (+ a doc block with one history line per round, newest "131 at v4 `83334b9a5`
  (131 - 2 + 2, P4.D303)"); `host_help_docs_boot.rs:117`
  `assert_eq!(expected, 131, "the vendored tree at v4 01a83539d")` (message left stale on
  purpose last round; leave it). `help_tree_equivalence` / `help_section_size_equivalence`
  carry no literal count — they compare to the regenerated oracle's `count` /
  `meta.files` / `meta.sections`.
- **Recipe headers that regenerate the two equivalence families from a pin**
  (verbatim in the test files' leading doc comments; the sweep driver extracts
  them): `help_tree_equivalence` = jest over a staged copy of
  `harness/oracle/cases/help-tree-sync.test.ts` in the v4 checkout/pin, writing
  `/tmp/oracle-help-tree.ndjson` (`QT_ORACLE_OUT`), run with
  `QT_ORACLE_HELP_TREE=…`; `help_section_size_equivalence` = `npx tsx
  harness/oracle/cases/help-section-size.ts > /tmp/oracle-help-section-size.ndjson`
  from the pin (needs `js-tiktoken` in the pin's `node_modules`), run with
  `QT_ORACLE_HELP_SECTION_SIZE=…`. Driver: `python3.13 harness/tools/recipe_sweep.py
  --v4 <pin> --v5w ~/source/quilltap-v5 --run <family>` (the system python is 3.9 and
  cannot parse it; `--v5w` must be the SPACELESS symlink path).
- **Schemas:** `apps/web/public/schemas/qtap-custom-tool.schema.json` 39,471 B == B's bytes; `…/qtap-progression.schema.json` 5,963 B == P's;
  `crates/quilltap-core/src/generators/qtap-export.schema.json` 105,310 B == B's.
  Constants to move: `public_schemas_vendor_guard.rs` `bytes: 39_471`;
  `qtap_schema_embed_guard.rs:72` and `qtap_schema.rs:195` `105_310` (§2.2).
- **`docs/v4/`**: 484 tracked files (+3 untracked `.DS_Store`); `git log -3 -- docs/v4`
  = `cc2a15b13` (P4.D303 U2, "mirror v4 `docs/` at `83334b9a5`"), `47a3e5f96`
  (P4.D287), `a96948e9d`. `docs/v4/CHANGELOG*.md` ×6 present.
- **Wire/CLI surfaces this slice does NOT own but touches by name:**
  `crates/quilltap-cli/src/help/main_help.txt:19`, `help/completion/zsh.template:46`,
  `…/fish.template:64` carry the pre-`75d094e63` "Memory Gate" bytes (captured
  from v4, owned by the CLI lane); `docs/v4/packages-quilltap-README.md` is the
  one CLI artifact THIS lane writes.
- **Dependency facts at the pin** (item 5; the provider survey owns the rest):
  `package.json:70` `"@quilltap/plugin-utils": "^2.7.1"` (B `^2.6.2`); lockfile
  `node_modules/@quilltap/plugin-utils` **2.7.1**
  (`https://registry.npmjs.org/@quilltap/plugin-utils/-/plugin-utils-2.7.1.tgz`;
  `packages/plugin-utils/package.json` `"version": "2.7.1"`; its CHANGELOG:
  "[2.7.1] Version-only republish of 2.7.0"); lockfile `node_modules/openai`
  **7.34.0** (B 7.30.0; `package.json:94` range `^7.23.0` unchanged). Of the **15**
  `plugins/dist/*` dirs, **14** declare `"@quilltap/plugin-utils": "^2.7.1"`
  (every dir except `qtap-plugin-builtin-embeddings`, which has only
  `@quilltap/plugin-types ^2.8.0` and was untouched in the span). Other pin
  facts measured because the sweep depends on them: the live v4 `node_modules`
  currently has `openai 7.34.0`, `@quilltap/plugin-utils 2.7.1`, `js-tiktoken 1.0.21`,
  `lexical 0.52.0`, `@lexical/react 0.52.0` — equal to the pin's lockfile, so
  the symlinked-`node_modules` pin worktree is valid *today*; re-verify at lane
  start (the P4.D303 trap: the live tree can move under the pin, and the plugin
  recorders resolve `plugin-utils` from each PLUGIN dir's own `node_modules`).

---

## 7. Tier split

**Tier 1 — must land**
1. U0: pin worktree + red-first measurement (§8.1): the four equivalence/guard
   families RED on unported `main` against the pin's oracles; the 18 builder
   families + 3 cases RED at the pin from IMPORT-DEAD.
2. U1: the IMPORT-DEAD repairs (§5.2) — stage A, first.
3. U2: `help/` — nine pages byte-exact from `git show P:`, the embed/boot comments.
4. U3: the two schema files + the THREE byte constants + (Tier 1 if the planner keeps
   the "vendor lane owns both" recommendation) the two `qtap_schema_validate_equivalence`
   mutation rows.
5. U4: `docs/v4/` — 61 operations, census `git ls-files docs/v4` = 491, every
   path `cmp`-equal to `git show P:`.
6. U5: the NO-PORT record (§4) written into the lane record for the unifier's
   `NO-PORT-RATIFIED(<round>)` step (the ledger itself is never written by a lane).

**Tier 2 — should land**: the `phase-0.md` one-liner (§4.3); the doc-comment
history lines on the three guards; updating `memory_time_equivalence.rs:309`'s
cite of the OPEN bug path only if that lane does not retire the table.

**Tier 3 — recorded deferrals (named, never silent)**: (a) the SPA
`composer-slash-commands` card id for the help page's new deep-link (slash-commands
SPA lane); (b) the About page's four blurbs (`8489d08b9`'s PORT hunk); (c) the SPA
`console.debug` removal (§4.15); (d) the CLI captured bytes (`75d094e63`);
(e) v5's test-only `is_moderation_refusal` (optional cleanup); (f) the
`DATABASE_ENCRYPTION.md` SQLCipher error stays in the mirror (v4's own doc).

---

## 8. The differential / guard plan

### 8.1 Pin and red-first (U0)

```
git -C ~/source/quilltap-server worktree add --detach /tmp/qt-v4-pin-<order>-25b8fab6a 25b8fab6a
rev-parse HEAD == 25b8fab6aa58636c6885ba63684c7f8fbfca9c3b ; git show 25b8fab6a:package.json -> "version": "4.11.0-dev.10"
symlinks: root node_modules, packages/quilltap/node_modules, every plugins/dist/*/node_modules
verify:   node_modules/openai/package.json 7.34.0 ; js-tiktoken 1.0.21 ; @quilltap/plugin-utils 2.7.1
          (if any differs: `npm ci --offline --no-audit --no-fund` IN the pin, as P4.D303 did)
Node: PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH ; QT_V4_CHECKOUT = QT_V4_ROOT = the pin
```
The pin's HEAD equals the live checkout's today (both `25b8fab6a`), but the live
tree is dirty (§3.4) — a worktree at the pin is still mandatory so no regen sees
the dirt. Red-first, on unported `main`, one invocation each, `rm -f` outputs first:
`help_tree_equivalence` (RED on `docs`/`chunks`: the nine pages' hashes and the
new sections' chunks; `count` stays 131), `help_section_size_equivalence` (RED on
the `(file, section)` set), `public_schemas_vendor_guard::the_vendored_schemas_equal_the_v4_checkouts`
(RED: custom-tool 39,471 vs 43,493), `qtap_schema_embed_guard`'s v4-equality test
(RED: 105,310 vs 105,774). GREEN before and after (count unmoved, measure it):
`help_tree_embed_guard`, `host_help_docs_boot` (-p quilltap-host), `spelling_guard`,
`public_schemas_vendor_guard::…_self_consistent` only after the `bytes:` edit.

### 8.2 Regen from the pin (after U2/U3)

`help_tree_equivalence`: recipe in its header (jest, staged case, `--roots
"$PWD" --roots $TMPO/cases -- help-tree-sync`, `QT_ORACLE_OUT=/tmp/oracle-help-tree.ndjson`),
expect `count 131`, `walkOrder 131`, `docs 131`, `chunks` = a new number (>779),
`jobs 0`. `help_section_size_equivalence`: `npx tsx
$V5W/harness/oracle/cases/help-section-size.ts > /tmp/oracle-help-section-size.ndjson`,
expect `meta {"maxTokens":1000,"files":131,"sections":N}`, N > 779, the three
touched pages' new section rows present, every count ≤ 1000, v5 composed text
byte-equal. Mutation proof (the headers name it): a one-byte edit to ONE
vendored page reddens `help_tree_equivalence`.

### 8.3 Byte-identity censuses (NUL-safe; zsh does not word-split — use a script)

```
python3 -I - <<'PY'     # run from the repo root; paths with spaces are fine
import subprocess,hashlib,os
V4=os.path.expanduser('~/source/quilltap-server'); P='25b8fab6a'
def show(p): return subprocess.run(['git','-C',V4,'show',f'{P}:{p}'],capture_output=True,check=True).stdout
names=subprocess.run(['git','-C',V4,'ls-tree','-r','-z','--name-only',P,'--','help/','docs/'],capture_output=True,check=True).stdout.split(b'\0')
bad=[n for n in names if n and open(('docs/v4/'+n.decode()[5:]) if n.startswith(b'docs/') else n.decode(),'rb').read()!=show(n.decode())]
print(len([n for n in names if n]),'pin paths;',len(bad),'mismatches',bad[:5])
for v5,v4 in [('docs/v4/packages-quilltap-README.md','packages/quilltap/README.md'),
              ('apps/web/public/schemas/qtap-custom-tool.schema.json','public/schemas/qtap-custom-tool.schema.json'),
              ('crates/quilltap-core/src/generators/qtap-export.schema.json','public/schemas/qtap-export.schema.json')]:
    assert open(v5,'rb').read()==show(v4), v5
PY
```
Expected: `help/` 131 paths + `docs/` 490 paths (= 621 names), 0 mismatches;
`git ls-files help/` = the identical 131-path set; `git ls-files docs/v4` = **491**
(490 + the README mapping) and the same SET as the pin's (the four old rename
paths and the four deleted paths ABSENT); `git ls-tree -r --name-only P -- docs |
wc -l` = 490 (B: 483). Wire the loop to skip `.DS_Store`.

### 8.4 Guards to run by name after each unit (zero `SKIP`)

U1: the 18 builder families + `danger_trigger_equivalence`,
`refusal_classify_equivalence`, `memory_time_equivalence`, each regenerated
from the pin (sweep `--families a,b,c`, comma-separated, ONE sweep at a time).
U2: `help_tree_equivalence`, `help_section_size_equivalence`, `help_tree_embed_guard`,
`-p quilltap-host --test host_help_docs_boot`, `spelling_guard`,
`quilltap-web`'s `help_web_routes`. U3: `public_schemas_vendor_guard` (3 tests),
`qtap_schema_embed_guard`, `-p quilltap-core generators::qtap_schema`,
`qtap_schema_validate_equivalence` (regenerated; the two new rows red-first),
`generation_key_travels_as_is_guard`, `moderation_refusal_ledger_isolation_guard`
(both read the export schema by name — re-run, expect no move). U4: the census
above + `spelling_guard`. Final: fmt/clippy (both feature sets; a `///` doc line
beginning with `+` is a list item to clippy), `cargo test --workspace` is the
unifier's.
Quote heredocs; never `pgrep`-poll a long job; one background command per job.

### 8.5 What the gate cannot prove

The size ceiling for the new `custom-tools.md` sections (§1.2) and the final
`chunks` count are known only after the regen. The `qtap_schema_validate_equivalence`
count deltas ajv-vs-crate for the new rows are RECORDED, not asserted equal.

---

## 9. What the ledger rows got wrong (or left loose) — corrected from hunks

1. **`a8b2d2861`**: "ALL 15 `plugins/dist/*/package.json`" for the plugin-utils
   `^2.6.2 -> ^2.7.1` move — **14**. `plugins/dist/qtap-plugin-builtin-embeddings/package.json`
   has no plugin-utils dependency (only `@quilltap/plugin-types ^2.8.0`) and
   `git diff --stat B P -- plugins/dist/qtap-plugin-builtin-embeddings` is empty.
   (The mandate's "the fifteen plugin dirs" repeats it; there are 15 dirs, 14
   dependents.)
2. **`eb5d42338`**: "RE-BUNDLED into all ten `plugins/dist/*/index.js` (each
   `VERSION2 = "7.34.0"`)" — `git show --stat eb5d42338 -- plugins/dist | grep
   index.js` lists **8** bundles (anthropic, curl, default-system-prompts, google,
   mcp, ollama, openrouter, search-serper; 973 lines each); the other three
   moved bundles (`deepseek`, `nanogpt`, `openai-compatible`) are `a8b2d2861`'s
   `normalizeUsage` re-bundle, and `openai`, `grok`, `z-ai`,
   `builtin-embeddings` are untouched. The literal `VERSION2 = "7.34.0"` is
   present in only two of the eight (anthropic, google); the string `7.34.0` in
   all eight. (Provider survey owns the rest — flagged here because the
   recorder-pin trap depends on which plugin dirs resolve the SDK.)
3. **`49f8906c3`**: "`restore-field-fidelity.test.ts` (NEW, 107 lines)" — the
   file **pre-existed** (`git cat-file -e 49f8906c3^:…` succeeds); the commit
   MODIFIES it (+107, two additions to `buildRepoMocks` + the "4.10 data-model
   additions" `describe`). Still tests-only; the NO-PORT conclusion stands.
4. **`b93c2efda`**: "v5 vendors only the `docs/v4/` paths a round needs;
   re-vendor … if the mirror tracks `features/complete/`" — the mirror is a
   **whole-tree** mirror and demonstrably tracks `features/complete/` (110 files
   at B); P4.D303's census asserts the same SET as the pin. Not optional.
5. **`8489d08b9`**: "`app/about/AboutView.tsx` (+8 / -4)" — `--stat` is `8
   ++++----`, i.e. **+4 / -4** (four lines replaced). Also "docs/releases/4.10.0.md
   reordered" is right but is invisible to a default `git log` (the commit is
   reachable only through the merge; use `--full-history`), so a per-path
   commit list built without it omits `8489d08b9` there.
6. **`7417e9a1b`**: "Oracle effect at a later pin: … measure at the next pin" —
   measured here: far larger than "any case importing the barrel or a deleted
   name" suggests. Not one barrel importer but **12 builder import sites in 11
   files / 18 recipe families**, plus three cases (§5). The ledger names
   `createMountWardrobeItem` but not its fixture-builder reach.
7. **`052659624`/`809e5104d`**: the §3 text "v5's own code comment at the pin
   should name `formatToParts`, not `toISOString`" — v5's comment
   (`memory_time_equivalence.rs:309`) already says `formatToParts`; only its
   cite of the OPEN bug path needs the `fixed/` move.
8. **`1e1712a12`**: "IF [the mirror] carries `CHANGELOG*.md`" — it does, all six
   (`CHANGELOG.md`, `CHANGELOG_V0.md` … `CHANGELOG_V4.md`).
9. **State drift not in the ledger**: v4's tree is dirty with docs-only changes
   (§3.4), so the §2 freshness probe "clean" no longer holds even though HEAD is
   the pin.
10. **Loose, correct as written**: `3f64d3a6c`'s "NOT `8489d08b9`" parent note,
    `2432b9057`'s 1,013-file / five-stamp claims (re-measured, exact), `f573a7576`'s
    empty combined diff (re-measured, 0 lines), `37246654d`'s four files,
    `936a34720`'s empty code-path stat (re-measured), `8489d08b9`'s 17 renames +
    4 deletions with their line counts (265 / 654 / 322 / 298), `75d094e63`'s
    `extractMigrationId` (hunk read), `01f508d4e`'s SPA `console.debug`.
