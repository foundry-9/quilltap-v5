# Dogfood walk — the `b220999d` round (dressing instructions ∥ archive-instead-of-delete ∥ the Documents chip) + the carried 💸 queue — 2026-08-26

**Instance:** a COPY of Friday at `~/qt-dogfood-friday` (never the live iCloud tree).
**Server:** `RUST_BACKTRACE=1 ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser`, log in the scratchpad.
**Findings log:** `docs/developer/porting/dogfood-findings.md` — next finding number is **#105**.
**Drift state:** ledger §2 freshness probe **PASSES** (v4 on `main`, tree clean, `b220999da..main` and `3a76b17df..bugfix` both empty). **No pending §3 drift rows** — every apparent divergence found on this walk is a v5 question, not v4 having moved.

## What this pass is for

Two rounds' 💸 items are owed: the `8f910137` round (unified 2026-08-25, after
that day's walk was already planned) and the `b220999d` round (unified
2026-08-26). The new round's three features are exactly the kind only a real
instance exercises interestingly — and the pre-walk measurement turned up a
**free cross-implementation proof** that no synthetic fixture could offer.

### The measured population (all counts from the rsynced copy, 2026-08-26)

| what | measured | why it matters |
|---|---|---|
| **`Wardrobe/instructions.md` written by v4** | **4 characters** — Abigail, Amy, Friday, Jackie (339 / 491 / 376 / 389 plain-text chars, written 04:04–04:12 today) | v4 ran the brand-new feature on this instance BEFORE the copy was taken. v5's reader, cascade and prompt thread must consume v4's own bytes — a free v4→v5 proof |
| **archived wardrobe items** | **17** across all four tiers: Abigail's vault 11, Voyages project 4, The Estate project 1, Quilltap General 1 | the archive feature already has a real population; the hide/show/badge/pool arms are testable on v4-written rows |
| archived scenarios | **0** | nothing to read; the walk archives one itself |
| Abigail's wardrobe | 34 files = 1 `instructions.md` + **33 garments, 11 archived** → 22 visible by default | exact expected counts for the reader-skip and hide arms |
| Quilltap General wardrobe | 13 files, 1 archived → 12 visible | the General tier's own arms |
| document stores | **18 enabled** (13 blob-backed, **5 externally rooted**) + 45 character vaults | the Documents chip over real breadth |
| doc file links / chunks | **4,924 links / 7,402 chunks** | the name scan and the content scan both have real corpora |
| stores named "Quilltap General" | **exactly 1** | the P4.D122 e2e-fixture duplicate does **not** reproduce on real data — measure and record |
| character photos | Friday **198**, Amy 98, Charlie 93 | the carried gallery-download 💸 item |
| chats / characters | 859 / 32 live | scenario-picker and new-chat material |

## ⚠ Hazards specific to THIS instance

- **Five document stores are rooted in the human's REAL directories** and are
  not part of the disposable copy:
  `Malory Wave` → `~/obsidian/Malory Wave`, `Quilltap Obsidian` →
  `~/obsidian/Quilltap`, `Sentinel` → `~/Local Obsidian/Charlie/Foundry-9/Sentinel`,
  `Church` → `~/Local Obsidian/Charlie/Church`, `Folio Source` →
  `~/source/quilltap-website/src/content/folio`.
  **Search/read them freely; NEVER create, edit, rename or delete a document in
  any of them.** All walk writes go to blob-backed stores (Quilltap General,
  the `Project Files: …` stores, character vaults, Quilltap Uploads).
- The registry's `Friday` instance points at **live iCloud Friday**. The CLI
  completion step registers a temporary `Dogfood` instance against the copy and
  removes it afterward rather than tab-completing against live data.
- The rsync left three 0-byte `*.db-journal` files behind (the recipe's `rm -f`
  leg); harmless, noted so it isn't mistaken for a crash artifact.

## What NOT to expect to work (do not file these)

- **Help-doc content is unported everywhere** (`p4.9i2`). This round banked ten
  more files (the three instructions files, the seven archive files,
  `help/search.md`). Their absence from the Guide is not a gap.
- **Group- and project-wardrobe REST URLs have no `quilltap-web` edge** — the
  dispatch-only precedent. Only three wardrobe edges are registered (General
  GET/POST + characters GET); a curl at `/api/v1/groups/{id}/wardrobe` 404s
  by design. The SPA calls dispatch verbs.
- **`componentsTransferred` / `unresolvedComponentIds` are still rendered
  nowhere** (v4's own client never reads them).
- **An archived character's instructions GET answers 200, not a tombstone
  404** — v4's deliberate-looking asymmetry, reproduced on purpose. The POST
  is the one that 409s.
- **A wardrobe `instructions.md` that trims to empty CONTINUES to the next
  cascade tier** — blank does not mean "unset here, stop". Not a bug.
- **CREATE does not carry `includeArchived`** — v4's body-not-param quirk means
  a create refresh drops the show-archived view. Reproduced client-side too.
- **The character seed in New Chat is UNGUARDED against archived** (only the
  project and general seeds are guarded) — v4's B7 quirk, reproduced.
- **`qtap-export.schema.json`** has never shipped in v5 (a named standalone flag).
- **No native save dialog** for downloads — the browser path.

---

## Part A — the Dressing Instructions vertical (P4.D119 server ∥ P4.D121 client)

| # | Owner | What | Gesture | Expect / verify | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | **v4's own bytes read through v5's GET verb** | `wardrobeInstructionsGet` for Abigail / Amy / Friday / Jackie via the dispatch API | Each returns v4's stored text **verbatim and trimmed**; compare against `doc_mount_documents.content` for the four `Wardrobe/instructions.md` rows. A free v4→v5 cross-implementation proof | **PASS** |
| A2 | CLAUDE | The section in the **wardrobe control dialog** | Open a character wardrobe → the collapsed `Dressing Instructions` header | Collapsed by DEFAULT; status reads `On file` for Abigail (U+2026 in `Consulting…`); chevron `-rotate-90` when collapsed | **PASS** |
| A3 | CLAUDE | The section in the **Aurora character wardrobe tab** | Character detail → Wardrobe tab, after the "Open wardrobe for …" button | Same component, same content — the second mount point | **PASS** |
| A4 | CLAUDE | **Save** a change | Edit the draft, `Save Instructions` | Toast `Dressing instructions saved`; the echoed value is the **trimmed** string; `doc_mount_documents.content` updated; save button disabled again (`!dirty`) | **PASS** |
| A5 | CLAUDE | **Clear** (blank → delete) | On a throwaway container (a `Project Files: …` store), write then blank the field | Toast `Dressing instructions cleared`; the `Wardrobe/instructions.md` link **removed** from that store (not blanked) | **PASS** |
| A6 | CLAUDE | 💸 **The cascade on a real "Let character choose" turn** | New chat with **Abigail** (character-tier instructions on file), outfit mode = let character choose; send one message | The `llm_choose` request in `llm_logs` carries (a) the 4th bullet `- The character's own dressing instructions, when provided …` and (b) the block `Dressing Instructions (addressed to Abigail in the second person — "you" is Abigail):` followed by v4's exact stored text, positioned LAST in the note chain immediately before `Available Wardrobe Items:` | PENDING |
| A7 | CLAUDE | The cascade **falls through** to a lower tier | Write project-tier instructions on a project store; start a chat in that project with a character that has **no** vault instructions, mode = let character choose | The same block appears carrying the PROJECT text — the character tier missed and the project tier won | PENDING |
| A8 | CLAUDE | The reader **skips** `instructions.md` | Abigail's wardrobe list | **22 garments** visible (33 non-instructions minus 11 archived); `instructions.md` never appears as a garment | **PASS** |
| A9 | CLAUDE | The refusal arms | curl: POST instructions with `{}`; POST for a missing character id; GET for a missing character | `{}` → flat `Validation error` 400; **missing character + invalid body → 404 `Character not found`** (the §3 review's guard-order fix); missing GET → 404 | **PASS** |
| A10 | CLAUDE | The **unprovisioned/no-vault** arms | curl General instructions POST with content while provisioned; character POST with content on a vault-less character | General provisioned → 200; vault-less + content → 500 `Character has no vault to hold dressing instructions`; vault-less + cleared → 200 no-op | **PASS** |

## Part B — archive instead of delete (P4.D120 server ∥ P4.D121 client)

| # | Owner | What | Gesture | Expect / verify | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | Archived garments **hidden by default** | Abigail's wardrobe dialog | 22 of 33 shown; none of the 11 v4-archived names (the Ottoman set, `Naked`, `Original Console`, `Roman Stola — Practical Linen`, `Walnut Gloves — Concealment`) | **PASS** |
| B2 | CLAUDE | **Show archived** reveals them with the badge | Tick `Show archived` | 33 shown; the 11 carry the lowercase `archived` badge next to `· default` | **PASS** |
| B3 | CLAUDE | **Archive** a live garment | Kebab → `Archive` on a Quilltap General item | Row leaves the default list; `archivedAt:` appears in that file's frontmatter | **PASS** |
| B4 | CLAUDE | **Restore** it | Kebab → `Restore from archive` | Row returns; `archivedAt` key **dropped**, not set false | **PASS** |
| B5 | CLAUDE | 💸 An archived garment is **absent from the Green Room pool** | The A6 `llm_choose` request | `Available Wardrobe Items:` contains none of Abigail's 11 archived names — but a General **composite** may still bundle an archived archetype (B7's deliberate asymmetry: the container loader fetches General WITH archived) | PENDING |
| B6 | CLAUDE | 💸 **Archived scenario vanishes from the Salon picker, returns suffixed** | Archive a General scenario; open the in-chat scenario control | Gone from the dropdown by default; with `Show archived` it returns as `<name> (archived)` (suffix AFTER the default marker, BEFORE ` — description`) | **PASS** |
| B7 | CLAUDE | The **default radio** is disabled on an archived scenario | Scenarios manager | Radio `disabled`, title `Archived scenarios cannot be the default`; `Archived` badge (`qt-badge qt-badge-secondary`) | **PASS** |
| B8 | CLAUDE | 💸 **Character-edit Archive/Restore on a real vault file** | A character with a `Scenarios/*.md` (e.g. Trina, Ariadne, Manjit Kaur) → edit form → scenario editor | Archive writes `archived`; **Restore DROPS the key**; `updatedAt` bumps; and the **`description` survives the round trip** (P4.D120's red-first fix — v5 used to drop it) | PENDING |
| B9 | CLAUDE | New Chat archived behavior + the **group optgroup** | New Chat with a grouped character | `Show archived scenarios` refetches all tiers; the group optgroup is now populated (v4 only just wired it); the project/general default seeds skip archived, the **character seed does not** | PENDING |
| B10 | CLAUDE | The **project wardrobe card** inline controls | Prospero → a project with wardrobe (Wardrobe Design 31, Voyages 20) | Inline `Archive`/`Restore` buttons (not a kebab); checkbox in the `justify-between` row with `+ New wardrobe item` | PENDING |
| B11 | CLAUDE | `archived: null` **refuses** | curl a scenario update with `"archived": null` | 400 — Zod 4's `Invalid input: expected boolean, received null` on the file-backed bag; flat `Validation error` on the character-scenario verbs; **nothing written** | **PASS** |
| B12 | CLAUDE | Unknown `?action=` cannot CREATE | `POST /api/v1/wardrobe?action=bogus` with a valid body | 400 `Unknown action:` envelope; **no archetype created** (count before/after) | **PASS** |

## Part C — the Documents search chip (P4.D122)

| # | Owner | What | Gesture | Expect / verify | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | Chip order + copy | Open the search dialog | Chips in the new order **chats, characters, messages, documents, tags, memories**; placeholder `Search chats, characters, messages, documents, tags, memories...`; the empty-state second line names documents | PENDING |
| C2 | CLAUDE | 💸 **Name-match over the real stores** | Search a filename fragment that exists across stores | Document rows; subtitle `<store> · <relativePath>` (U+00B7, spaced); `Document` badge | PENDING |
| C3 | CLAUDE | **Content-match** (the chunk scan) | Search a phrase present only in document BODY text | Rows whose `matchedField` is content and whose snippet shows the phrase (`line-clamp-2`) | PENDING |
| C4 | CLAUDE | The **`Vault` badge** | Search a term that hits a character vault file | `Vault` badge in addition to `Document` when `storeType === 'character'` | PENDING |
| C5 | CLAUDE | **Fail-closed archived-vault exclusion** | Archive a character that has vault documents, then re-search a term that hit it | Its vault rows disappear from results; unarchive restores them | PENDING |
| C6 | CLAUDE | **Standalone open** (plain click) | Click a document result with no Salon active | A `document-standalone` workspace tab opens on the right file; the search UI closes | PENDING |
| C7 | CLAUDE | **In-chat split open** | With a Salon tab focused, click a document result | Split-opens into the chat (`mode:'split'`, `mountPoint` = the result's ref) — and, thanks to the unification wire, an already-split Salon reconciles WITHOUT a reload | PENDING |
| C8 | CLAUDE | **Modified click passes through** | ⌘-click / middle-click a document result | The dialog stays OPEN and the browser follows the standalone href — no `preventDefault` | PENDING |
| C9 | CLAUDE | Cross-type ranking | A query matching several types | Ties place documents **after memories, before tags** (source fan-out order under the stable sort); `countsByType` covers the whole pre-pagination set | PENDING |
| C10 | CLAUDE | The duplicate-General question | Count enabled stores named `Quilltap General` | **Exactly 1 on real data** — the P4.D122 e2e-fixture collision does not reproduce here; record it as evidence for candidate 2 | PENDING |

## Part D — the carried `8f910137` 💸 queue

| # | Owner | What | Gesture | Expect / verify | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | 💸 **The in-chat scenario picker on real data** | A real chat: seed → pick a preset → the Host revision bubble → re-pick the same (no-op) → clear | The revision announcement is byte-exact; a no-op writes nothing; `chats.scenarioText` / the transcript carry follows | **PASS** |
| D2 | CLAUDE | 💸 **Gallery download + detail-modal controls** | Friday's 198-photo gallery → the gallery-tab download button and the detail modal | The download control exists on the gallery tab (v4 bug 99's port) and the modal is reparented to the body — clickable, not trapped under the workspace stacking context | PENDING |
| D3 | CLAUDE | 💸 The restyled **qt-*** surfaces | A pass across a few screens | The 364 formerly-inert `qt-*` sites now style (bugs 100/102) — no unstyled text where a token was inert | PENDING |
| D4 | CLAUDE | 💸 A real CLI **completion** | Register a temporary `Dogfood` instance at the copy; `quilltap docs --instance Dogfood <TAB>` behavior via the completion script; remove the instance | Completions list real values, bash and zsh arms alike (bug 101's byte-copied templates) | PENDING |

## Part E — the standing 💸 remainder

| # | Owner | What | Why here | Status |
|---|---|---|---|---|
| E1 | CLAUDE | 💸 **Pascal side effects — the chat tier** | The 2026-08-25 recipe: a custom tool with `effects: [{target: "state.dogfoodCounter", …}]` run in a **project-less** chat lands at the chat tier (the key exists nowhere → default). Verify from the effect record `{"target","previous","next","tier"}` plus the tier store | PENDING |
| E2 | CLAUDE | 💸 Pascal side effects — the **project tier** | Pre-seed the same key at the project tier, run the tool in a chat inside that project → the project tier wins | PENDING |
| E3 | HUMAN | 💸 The raised **Brahma budget** on a deep query | Genuinely exhausting 50 agent turns is expensive and non-deterministic; the wire half is already proven | PENDING |
| E4 | HUMAN | 💸 **Memory dedup + conversation-summaries** first run | Real batch spend across the whole Friday corpus | PENDING |
| E5 | HUMAN | 💸 The **NanoGPT caching** smoke + the #101 cache-read cost question | A cost judgment, not a correctness one | PENDING |

---

## Results log

_(appended as steps run)_

### A1 — v4's own instruction bytes, read back through v5 · **PASS**

`characterWardrobeInstructionsGet` for all four characters v4 wrote this morning,
compared character-for-character against `doc_mount_documents.content`:

```
Abigail: IDENTICAL  339 chars      Amy:    IDENTICAL  491 chars
Friday:  IDENTICAL  376 chars      Jackie: IDENTICAL  389 chars
```

A **free cross-implementation proof**: v4 wrote these files with its own
`writeWardrobeInstructionsFile` hours before the copy was taken, and v5's reader
returns them byte-for-byte — trimmed body, no frontmatter, no trailing newline.

### A2 — the section in the wardrobe control dialog · **PASS**

Collapsed by default; status `On file`. Expanded, the field hint is
**byte-identical to v4's `field-hints.ts` at `b86bb1a5`**, curly quotes and em
dash included:

> Standing guidance for a character choosing their own opening outfit, addressed
> to the character in the second person. Consulted only when a chat begins with
> “Let character choose” — the nearest copy wins (a character’s own over their
> group’s, a group’s over the project’s, the project’s over Quilltap General)
> and the search stops there.

…and the example line `Written as: You prefer practical tweeds for fieldwork, and
reserve the brass-buttoned frock coat for occasions with an audience.`
Switching the container to **Quilltap General** (no file) shows the third status
state, `None on file`.

### A3 — the section on the character detail's Wardrobe tab · **PASS**

Second mount point present, in v4's slot: **after** the `Open wardrobe for
Abigail` button, collapsed, `On file`.

### A4 — save · **PASS**

Typed into the General container's editor, `Save Instructions` enabled on dirty,
clicked → toast **`Dressing instructions saved`**, status flipped to `On file`,
button disabled again. On disk the store gained
`Wardrobe/instructions.md` holding exactly
`Dress for the Estate's evening hours: dark wool, no jewellery.` — **62 chars,
no frontmatter, no trailing newline**, and the GET verb echoes the same bytes.

### A8 — the reader skips `instructions.md` · **PASS**

`characterWardrobeList` for Abigail, over a vault where v4 really did write the
file: **22 items** with `includeArchived=false`, **33** with `true`, and **no
item titled anything like "instructions"** in either. The character detail's stat
line agrees (`22 wardrobe items`).

### B1 / B2 — archived hidden by default, revealed with the badge · **PASS**

The 11 archived names the server withholds are exactly v4's 11 (the seven-piece
Ottoman set + its composite, `Naked`, `Original Console`, `Roman Stola —
Practical Linen`, `Walnut Gloves — Concealment`).

In the dialog, ticking **Show archived** took the merged Items list **33 → 44
rows (+11)** — Abigail's 10 non-composite archived plus **General's own
`Hotel Staff Uniform`**, which is the flag reaching *both* loaders as v4 threads
it. Each carries the lowercase badge `<span class="qt-badge qt-badge-secondary">archived</span>`.

⚠ **Trap banked:** the badges are invisible to a `textContent` word-boundary
regex — adjacent badges render as `archivedtopbottom`, so `/\barchived\b/`
matches nothing and reads as "the badge is missing". Count the badge *elements*.

### A5 — clear deletes the file · **PASS**

Blanking the General editor and saving → toast **`Dressing instructions cleared`**,
status `None on file`, and the `Wardrobe/instructions.md` **link row is gone from
the store** (not blanked). `wardrobeInstructionsGet` answers `{"instructions":null}`.

### A9 / A10 — every refusal arm, byte-exact · **PASS**

| gesture | answer |
|---|---|
| character SET, key absent | 400 `Validation error` |
| **missing character + invalid body** | **404 `Character not found`** ← the §3 unification fix, live |
| missing character GET | 404 `Character not found` |
| missing character + valid body | 404 `Character not found` |
| **archived** character GET | **200 `{"instructions":null}`** — v4's asymmetry, reproduced |
| archived character SET (content) | 409 `Character is archived; dressing instructions cannot be edited` |
| archived character SET (`null`) | 409 — the same, the archive gate beats the clear no-op |
| General SET explicit `null` | 200 (harmless clear no-op) |
| General SET key absent | 400 `Validation error` |

The `double_option` tri-state decodes correctly through the wire: *absent* and
*explicit null* land on different arms.

### B3 / B4 — archive and restore through the kebab · **PASS**, and a free `preserve_file_names` proof

Archiving General's `Apple Watch` took the list 12 → 11 and wrote

```yaml
archived: true
archivedAt: 2026-08-26T15:14:42.011Z
updatedAt: 2026-08-26T15:14:42.011Z   # bumped
```

Restoring **dropped both keys** — never `archived: false` — and bumped `updatedAt`
again. Exactly v4's contract.

**The unplanned proof:** an archive write projects the whole General wardrobe
array back into the vault folder, and that sweep deletes every file not in the
projection. `Wardrobe/instructions.md`, written minutes earlier in A4,
**survived** — which is P4.D119's `preserve_file_names` option doing its one job,
on real data, with a real sweep.

### B6 / B7 — an archived scenario leaves the pickers and comes back suffixed · **PASS**

Archived `The Attic` from the General Scenarios manager (which now shows v4's four
inline actions: Edit / Rename / **Archive** / Delete, and the `Show archived`
checkbox in the `justify-between` row with `+ New scenario`).

- The manager: the row vanishes; with **Show archived** it returns carrying the
  `Archived` badge, its action reads **`Restore`**, and its default radio is the
  **only disabled radio of the 20**, titled `Archived scenarios cannot be the
  default`.
- The in-chat picker: **33 options → 34**, the returning one reading
  **`The Attic (archived) — The Lantern's Projection Space`** — suffix after the
  name, before the ` — description`, exactly v4's order (the sibling
  `The Bridge (project default) — …` shows the marker it has to follow).

### B11 / B12 — the two §3-review refusals, live · **PASS**

- `scenarioUpdate` with `"archived": null` → **400** `Invalid request body:
  Invalid input: expected boolean, received null; Required`; with `"archived":
  "yes"` → the same shape with `received string`. Zod 4's measured sentences, and
  the target scenario is untouched afterwards (name, 4,251-char body, `archived:
  false` all unchanged).
- `POST /api/v1/wardrobe?action=bogus` with a valid create body → **400**
  `{"error":"Unknown action: bogus","availableActions":["instructions"]}` and the
  General archetype count is **13 before, 13 after** — nothing created. A
  present-but-empty `?action=` falls through to the collection verb (v4's
  truthiness gate) and refuses the bogus body with `Validation error`.

### D1 — 💸 the in-chat scenario picker on real data · **PASS**

On the real 71-message chat *The Nice Rack Incident* (project **Voyages of the
Covenant**), through the sidebar's Chat → Scenario control:

1. **Preset change** `The Deck` → `The Galley` → a Host message written with
   `systemSender=host`, `systemKind=scenario-change`, opening
   `The Host revises the scene for the proceedings:` and `chats.scenarioText`
   replaced (4,597 chars of the Galley scene).
2. **No-op** — re-picking the same preset and pressing Change scenario: message
   count **72 → 72**, nothing written.
3. **Clear** — `Custom…` with an empty box: `scenarioText` → **NULL** and a
   *different* Host sentence, `The Host draws the previous scene aside; the
   company carries on without a set scene.`

Both sentences are byte-identical to v4's
`lib/services/host-notifications/writer.ts:438` and `:453`.
