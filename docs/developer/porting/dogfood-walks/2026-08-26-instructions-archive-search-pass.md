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
| A6 | CLAUDE | 💸 **The cascade on a real "Let character choose" turn** | New chat with **Abigail** (character-tier instructions on file), outfit mode = let character choose; send one message | The `llm_choose` request in `llm_logs` carries (a) the 4th bullet `- The character's own dressing instructions, when provided …` and (b) the block `Dressing Instructions (addressed to Abigail in the second person — "you" is Abigail):` followed by v4's exact stored text, positioned LAST in the note chain immediately before `Available Wardrobe Items:` | **PASS** |
| A7 | CLAUDE | The cascade **falls through** to a lower tier | Write project-tier instructions on a project store; start a chat in that project with a character that has **no** vault instructions, mode = let character choose | The same block appears carrying the PROJECT text — the character tier missed and the project tier won | **PASS** |
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
| B5 | CLAUDE | 💸 An archived garment is **absent from the Green Room pool** | The A6 `llm_choose` request | `Available Wardrobe Items:` contains none of Abigail's 11 archived names — but a General **composite** may still bundle an archived archetype (B7's deliberate asymmetry: the container loader fetches General WITH archived) | **PASS** |
| B6 | CLAUDE | 💸 **Archived scenario vanishes from the Salon picker, returns suffixed** | Archive a General scenario; open the in-chat scenario control | Gone from the dropdown by default; with `Show archived` it returns as `<name> (archived)` (suffix AFTER the default marker, BEFORE ` — description`) | **PASS** |
| B7 | CLAUDE | The **default radio** is disabled on an archived scenario | Scenarios manager | Radio `disabled`, title `Archived scenarios cannot be the default`; `Archived` badge (`qt-badge qt-badge-secondary`) | **PASS** |
| B8 | CLAUDE | 💸 **Character-edit Archive/Restore on a real vault file** | A character with a `Scenarios/*.md` (e.g. Trina, Ariadne, Manjit Kaur) → edit form → scenario editor | Archive writes `archived`; **Restore DROPS the key**; `updatedAt` bumps; and the **`description` survives the round trip** (P4.D120's red-first fix — v5 used to drop it) | **PASS** |
| B9 | CLAUDE | New Chat archived behavior + the **group optgroup** | New Chat with a grouped character | `Show archived scenarios` refetches all tiers; the group optgroup is now populated (v4 only just wired it); the project/general default seeds skip archived, the **character seed does not** | **PASS** |
| B10 | CLAUDE | The **project wardrobe card** inline controls | Prospero → a project with wardrobe (Wardrobe Design 31, Voyages 20) | Inline `Archive`/`Restore` buttons (not a kebab); checkbox in the `justify-between` row with `+ New wardrobe item` | **PASS** |
| B11 | CLAUDE | `archived: null` **refuses** | curl a scenario update with `"archived": null` | 400 — Zod 4's `Invalid input: expected boolean, received null` on the file-backed bag; flat `Validation error` on the character-scenario verbs; **nothing written** | **PASS** |
| B12 | CLAUDE | Unknown `?action=` cannot CREATE | `POST /api/v1/wardrobe?action=bogus` with a valid body | 400 `Unknown action:` envelope; **no archetype created** (count before/after) | **PASS** |

## Part C — the Documents search chip (P4.D122)

| # | Owner | What | Gesture | Expect / verify | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | Chip order + copy | Open the search dialog | Chips in the new order **chats, characters, messages, documents, tags, memories**; placeholder `Search chats, characters, messages, documents, tags, memories...`; the empty-state second line names documents | **PASS** |
| C2 | CLAUDE | 💸 **Name-match over the real stores** | Search a filename fragment that exists across stores | Document rows; subtitle `<store> · <relativePath>` (U+00B7, spaced); `Document` badge | **PASS** |
| C3 | CLAUDE | **Content-match** (the chunk scan) | Search a phrase present only in document BODY text | Rows whose `matchedField` is content and whose snippet shows the phrase (`line-clamp-2`) | **PASS** |
| C4 | CLAUDE | The **`Vault` badge** | Search a term that hits a character vault file | `Vault` badge in addition to `Document` when `storeType === 'character'` | **PASS** |
| C5 | CLAUDE | **Fail-closed archived-vault exclusion** | Archive a character that has vault documents, then re-search a term that hit it | Its vault rows disappear from results; unarchive restores them | **PASS** |
| C6 | CLAUDE | **Standalone open** (plain click) | Click a document result with no Salon active | A `document-standalone` workspace tab opens on the right file; the search UI closes | **PASS** |
| C7 | CLAUDE | **In-chat split open** | With a Salon tab focused, click a document result | Split-opens into the chat (`mode:'split'`, `mountPoint` = the result's ref) — and, thanks to the unification wire, an already-split Salon reconciles WITHOUT a reload | **FAIL(#105) → FIXED, re-verified PASS** |
| C8 | CLAUDE | **Modified click passes through** | ⌘-click / middle-click a document result | The dialog stays OPEN and the browser follows the standalone href — no `preventDefault` | **PASS** |
| C9 | CLAUDE | Cross-type ranking | A query matching several types | Ties place documents **after memories, before tags** (source fan-out order under the stable sort); `countsByType` covers the whole pre-pagination set | **PASS** |
| C10 | CLAUDE | The duplicate-General question | Count enabled stores named `Quilltap General` | **Exactly 1 on real data** — the P4.D122 e2e-fixture collision does not reproduce here; record it as evidence for candidate 2 | **PASS** |

## Part D — the carried `8f910137` 💸 queue

| # | Owner | What | Gesture | Expect / verify | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | 💸 **The in-chat scenario picker on real data** | A real chat: seed → pick a preset → the Host revision bubble → re-pick the same (no-op) → clear | The revision announcement is byte-exact; a no-op writes nothing; `chats.scenarioText` / the transcript carry follows | **PASS** |
| D2 | CLAUDE | 💸 **Gallery download + detail-modal controls** | Friday's 198-photo gallery → the gallery-tab download button and the detail modal | The download control exists on the gallery tab (v4 bug 99's port) and the modal is reparented to the body — clickable, not trapped under the workspace stacking context | **PASS** |
| D3 | CLAUDE | 💸 The restyled **qt-*** surfaces | A pass across a few screens | The 364 formerly-inert `qt-*` sites now style (bugs 100/102) — no unstyled text where a token was inert | **PASS** |
| D4 | CLAUDE | 💸 A real CLI **completion** | Register a temporary `Dogfood` instance at the copy; `quilltap docs --instance Dogfood <TAB>` behavior via the completion script; remove the instance | Completions list real values, bash and zsh arms alike (bug 101's byte-copied templates) | **PASS** |

## Part E — the standing 💸 remainder

| # | Owner | What | Why here | Status |
|---|---|---|---|---|
| E1 | CLAUDE | 💸 **Pascal side effects — the chat tier** | The 2026-08-25 recipe: a custom tool with `effects: [{target: "state.dogfoodCounter", …}]` run in a **project-less** chat lands at the chat tier (the key exists nowhere → default). Verify from the effect record `{"target","previous","next","tier"}` plus the tier store | **PASS** |
| E2 | CLAUDE | 💸 Pascal side effects — the **project tier** | Pre-seed the same key at the project tier, run the tool in a chat inside that project → the project tier wins | **PASS** |
| E3 | HUMAN | 💸 The raised **Brahma budget** on a deep query | Genuinely exhausting 50 agent turns is expensive and non-deterministic; the wire half is already proven | DEFERRED-TO-HUMAN |
| E4 | HUMAN | 💸 **Memory dedup + conversation-summaries** first run | Real batch spend across the whole Friday corpus | DEFERRED-TO-HUMAN |
| E5 | HUMAN | 💸 The **NanoGPT caching** smoke + the #101 cache-read cost question | A cost judgment, not a correctness one | DEFERRED-TO-HUMAN |

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

### Part C — the Documents chip · **PASS**, after one real bug

**C1** — chips read `Chats · Characters · Messages · Documents · Tags · Memories`
in the new order; placeholder
`Search chats, characters, messages, documents, tags, memories...`; the
empty-state second line names documents too.

**C2 / C3 / C4** — over the real corpus (4,924 file links, 7,402 chunks, 18
document stores + 45 vaults): a name query returns hits across both store kinds
with v4's exact URL shape
(`/workspace?open=document-standalone&scope=document_store&mountPoint=<ref>&filePath=<rel>`);
a body-only query (`sextant`) returns **23 content matches** whose snippets carry
the chunk heading and the matched text; and a vault hit renders **both** badges,
`Document` + `Vault`, under the subtitle `Charlie Character Vault · Wardrobe/Covenant Traveler.md`.

**C5 — the fail-closed archived-vault exclusion · PASS.** Ten archived
characters, all ten with vault ids. A query hitting every vault's
`personality.md` returns **35 distinct vaults of the 45** and **zero rows from
any of the ten archived vault ids** (checked by mount id, not by name — two
similarly-named vaults make the name check lie).

**C6 / C7** — both open arms, and the discriminator is the database:
- Home focused → a `document-standalone` tab, **no `chat_documents` row**.
- A Salon focused → the document opens **in the chat**: a `chat_documents` row
  (`Wardrobe/Covenant Traveler.md`, `mountPoint: Charlie Character Vault`) and a
  Librarian message (`systemSender=librarian`, `systemKind=opened-by-user`).

  **C7 was FINDING #105 and is fixed** — see below.

**C8 — modified-click passthrough** — a ⌘-click was **not** `preventDefault`ed:
the browser followed the standalone href. (The pane navigates the same tab under
an automated ⌘-click, so the "and the dialog stays open" half is not observable
here; the unit spec pins `isModifiedClick` and the href assertion is in the beat.)

**C9 — ranking** — sort is `matchPriority` ASC then `updatedAt` DESC, strictly
monotone within a priority band, with documents interleaving among memories by
date. `countsByType` reports the whole pre-pagination set (23 documents) even
when the 50-slot page holds fewer. The tie-order rule (documents after memories,
before tags) is untestable here because no two rows tie on **both** keys.

⚠ **Read this before calling a missing DOCUMENTS group a bug:** with all six
chips on, a common word puts 100 messages ahead of everything, and documents
fall off the page entirely — v4's deliberate "no per-type page quota". It looks
exactly like a broken chip. It isn't.

**C10 — the duplicate "Quilltap General" does NOT reproduce on real data.** The
instance has **exactly one** store by that name, and in fact **no duplicate store
names at all** — so `docStoreAuthority`'s UUID arm never fires here and every ref
is a name. The collision is a property of the committed e2e fixture
(phase-4.md candidate 2), not of real instances.

### 🔴 FINDING #105 — a Documents result clicked from inside a chat did nothing (FIXED, `599f6be9`)

Clicking a Documents result with a Salon focused threw **NG0201** out of
`OpenDocumentFromSearch.open` → `injector.get(DocumentApi)` and the dialog just
sat there. `OpenDocumentFromSearch` is `providedIn: 'root'`, so its injector is
the **root** injector, which never sees `salon-conversation.ts`'s component
`providers: [… DocumentApi]`. The lane had already met NG0201 at *render* time
and moved the lookup to a lazy `injector.get`; that relocated the crash to click
time without fixing it.

- **Why nobody saw it:** both existing e2e beats run with Home focused (the
  standalone arm), and the unit harness constructs the service by hand with
  `injector: { get: () => ({ openDocument }) }` — a stub that always answers.
- **Fix:** build the stateless client in the root injection context,
  `runInInjectionContext(this.injector, () => new DocumentApi())`, memoized —
  deliberately **not** registered globally, because `document-picker.ts:335`
  relies on `inject(DocumentApi, {optional: true})` being **absent** outside a
  chat to fall back to `StandaloneDocumentApi`.
- **Guards:** three TestBed specs resolving the service the way the app does
  (mutation-proven — restoring `injector.get` reddens exactly the two that
  resolve, and leaves the "not registered globally" one green), plus a **third
  e2e beat** that clicks the card with a Salon focused. That beat **failed red
  against the pre-fix bundle** and passes against the fixed one.
- **Gate:** ng test 351 files / 5,295; ng build clean; Playwright **250 passed /
  1 skipped** (the standing component-transfer park; the suite grew 249 → 250).

### A6 — 💸 the cascade on a real "Let character choose" turn · **PASS**

A new chat with **Abigail** (whose character card already carries *"Let this
character choose their opening outfit"*), created through the New Chat form. The
`outfit-selection` cheap-LLM call logs as `SUMMARIZATION` (v4's
`map_task_type_to_log_type`), and its request carries both halves:

**The 4th system-prompt bullet, last in the opening list, immediately after
`- The character's personality`:**

> `- The character's own dressing instructions, when provided — these describe
> what the character prefers to wear and under what circumstances; weigh them
> heavily, above general appropriateness guesses`

**The note, last in the chain — after the Scenario note, immediately before the
blank line and `Available Wardrobe Items:`:**

> `Dressing Instructions (addressed to Abigail in the second person — "you" is Abigail):`
> followed by **v4's own stored bytes verbatim**.

### A7 — the cascade falls through to the project tier · **PASS**

Wrote project-tier instructions on **Wardrobe Design** through
`projectWardrobeInstructionsSet` (with deliberate surrounding whitespace — the
POST echoed the **trimmed** string, v4's contract), then made a chat in that
project with **Ariel**, who has no vault instructions
(`{"instructions":null}`), outfit mode *Let character choose*. Her prompt carries

> `Dressing Instructions (addressed to Ariel in the second person — "you" is Ariel):`
> `You dress for the atelier: muslin toiles, pinned hems, nothing precious.`

— the character tier missed, the project tier won, and the name interpolates
twice against the *character*, not the tier that supplied the text.

### B5 — 💸 an archived garment never reaches the Green Room pool · **PASS**

The A6 pool holds **35 items** and **none of the twelve archived titles** —
Abigail's eleven plus General's `Hotel Staff Uniform`.

⚠ **Trap banked:** a substring test says `Naked` IS in the pool. It is not — the
live composite **`Naked Marguerite`** contains it. Match the quoted title
(`| "…"`), not the substring.

### B8 — 💸 Archive / Restore on real vault files, and the `description` round trip · **PASS** (both scopes)

**Character-vault scope** (Trina's `Scenarios/Default.md`, through the character
edit form → Archive → **Save Character**, the local-form-data path):

```
before   # Default\n\nTrina arrives at the Lodge…      (no frontmatter at all)
archived ---\narchived: true\n---\n\n# Default\n\nTrina arrives at the Lodge…
restored # Default\n\nTrina arrives at the Lodge…      (byte-identical to before)
```

Restore **drops the key and the now-empty frontmatter block** — never
`archived: false`. `characterScenarioList` reads 0 by default and 1 with
`includeArchived`.

**File-backed scope** — the P4.D120 `build_scenario_file` fix, which is where
v5's description-drop bug lived, proven on a real General scenario that actually
carries one:

```
before   ---\nname: The Attic\ndescription: The Lantern's Projection Space\n---
archived ---\nname: The Attic\ndescription: The Lantern's Projection Space\narchived: true\n---
restored ---\nname: The Attic\ndescription: The Lantern's Projection Space\n---
```

The description survives the write in **both** directions, and the file returns
to its original bytes.

The section's own new help paragraph is in place too: *"Archiving a scenario
keeps it here but hides it from the chat pickers unless 'Show archived' is
ticked. Chats already using it are unaffected."*

### B9 — New Chat: the group optgroup v4 only just wired · **PASS**

Real Friday has **no group scenarios at all**, so the wiring is invisible until
one exists. Created one on **Sebold Family** through `groupScenarioCreate`
(⚠ the bag's body key is **`body`**, not `content` — a `content` bag refuses
`Invalid request body: Required`), then selected **Abigail** (a member) in New
Chat. The Starting Scenario dropdown's optgroups became:

```
General Scenarios: 20
Group Scenarios: Sebold Family: 1     ← "The Family Table — A dogfood scene"
```

With only **Charlie** selected the group optgroup is absent, so the union really
is keyed on the selected participants. `Show archived` is present; the restored
`The Attic` shows with **no** ` (archived)` suffix; and the per-character
Starting Outfit block offers *Let character choose* for Abigail and **not** for
Charlie (the user seat).

### B10 — the project wardrobe card's inline controls · **PASS**

Prospero → **Wardrobe Design**. The Wardrobe (31) card carries `Show archived`
in the `justify-between` row with `+ New wardrobe item`, and each row shows
**inline** `Edit · Archive · Delete` — not a kebab, exactly as v4 renders this
card. Archiving `Bare Feet` took the header to **Wardrobe (30)** and wrote
`archived: true` + `archivedAt` into the garment file.

---

## Part D — the carried `8f910137` 💸 queue

### D2 — the gallery download + the detail modal · **PASS**

Friday's Photo Gallery (60 tiles of her 212 photos). Opening one gives the
detail modal with its four controls, and both halves of v4 bug 99's port hold:

- `QT-IMAGE-DETAIL-MODAL` is a **direct child of `<body>`** — the reparent.
- `document.elementFromPoint` at each control returns the control itself:
  `Download`, `Copy to clipboard`, `Save to my gallery`, `Close (Escape)`.
  Before the reparent this is where the workspace toolbar's queue badges won the
  hit test.
- Clicking Download and Copy raises **zero** errors on a freshly-armed in-page
  handler.

⚠ **Trap banked:** `read_console_messages` returns the tab's console **history
across reloads**, so two stale pre-fix `NG0201`s showed up long after the fix and
a reload. Arm a fresh in-page `console.error` hook per gesture before trusting an
error as *this* click's.

### D3 — the restyled `qt-*` surfaces · **PASS**

The SPA gate's own guard ran green (934 classes, every reference resolves), and a
live sweep of the running DOM finds **199 `qt-*` classes in use against 817
defined**, with no unstyled text across ~12 screens visited today. The handful my
DOM sweep flagged are the known false-positive families: Tailwind
opacity-modifier forms (`qt-bg-card/50` — the escaped `\/50` selector my regex
can't see) and structural hook names with no styles by design
(`qt-salon-portal-holder`, `qt-chat-turn-controls`).

### D4 — a real CLI completion · **PASS**

Registered a temporary `Dogfood` instance **against the copy** (never the live
`Friday` registry entry), sourced `quilltap completion bash`, and drove it:

```
docs --instance <TAB>   → Lebanon  Friday  Ignite  V4test  Dogfood
docs --instance Dogfood --mount <TAB>
                        → Abigail\ Character\ Vault
                          $'Ab\305\253 al-Q\304\201sim … Character Vault'
                          Amy\ Character\ Vault  …  Church  …
```

Real store names read off the encrypted copy, correctly quoted: spaces
backslash-escaped, and the non-ASCII transliterated name emitted as an ANSI-C
`$'…'` string. That is bug 101's byte-copied bash template against genuinely
hostile input. The zsh script generates (`#compdef quilltap`). The temporary
instance was **removed** and the registry verified back to its four entries.

## Part E — the standing 💸 remainder

### E1 / E2 — 💸 Pascal side effects: the chat tier AND the project tier · **PASS**

Deferred four times; **two of the three remaining write paths now proven live.**
Authored a probe definition into `Quilltap General/Tools/dogfood_tally.tool.json`
declaring one effect on `state.dogfoodCounter`, and ran it through
`chatCustomToolRun` (v4's own manual-run route applies effects —
`app/api/v1/chats/[id]/custom-tools/route.ts:514`).

**Chat tier** — a project-less chat, key present nowhere:

```json
{"target": "state.dogfoodCounter", "next": 42, "tier": "chat"}
```
`chats.state` → `{"dogfoodCounter":42}`. **No `previous` key** — the store held
nothing, and v4's `undefined` does not serialize.

**Project tier** — seeded `projectStateSet {"dogfoodCounter": 7}` on Wardrobe
Design, then ran in a chat **inside** that project:

```json
{"target": "state.dogfoodCounter", "previous": 7, "next": 42, "tier": "project"}
```
The project's state became 42 and **that chat's own `state` stayed `{}`** — the
write went where the key already lives, which is the whole rule.

**Three traps this cost, all worth the next walk's time:**
1. The effect `value` grammar has **no identifiers**. `$state.x` and a bare
   `state.x` are both load-time rejections (with excellent messages —
   *"`state` is a bare word — there are no identifiers here. Quote literal text
   ('state') or use a `{{ref}}`"*). The reference spelling is `{{state.x}}`.
2. **A reference that resolves to nothing SKIPS that effect**, silently as far as
   `pascalMeta` is concerned (the applied list is empty, so v4 omits the key
   entirely). `{{state.dogfoodCounter}} + 1` on a fresh key writes **nothing** —
   which reads exactly like a broken feature. Seed the key, or use an expression
   that always evaluates.
3. A definition written through `documentWrite` needs a moment before the library
   lists it.

**Still owed: the group tier**, and it is now precisely characterized — it needs
a chat whose participants resolve to exactly one group (`groupTier.status ==
"single"`). Real Friday's two groups overlap on Charlie, so a purpose-built chat
of single-group members is the gesture.

### A verification that came free — the Workbench lists BROKEN definitions

Wrote a deliberately invalid definition (a bare-word effect expression). It does
**not** appear under `tools` — and for a while that looked like a finding, since
v4's `buildCustomToolLibrary` maps `errors` to `valid: false` entries
(`lib/pascal/workbench.ts:201`). v5 does exactly the same; my probe was reading
the wrong array. Under `errors`:

```json
{"valid": false, "definitionPath": "Tools/dogfood_broken.tool.json",
 "mountName": "Quilltap General",
 "reason": "effects.0.value: value is not a valid expression: \"state\" is a bare word — there are no identifiers here. …",
 "attachments": [{"kind": "general", "label": "General"}]}
```

Both probe files were deleted afterwards; the library is back to 9 tools, 0 errors.

---

## An observation worth an order, not a finding

**`systemHome` — the landing dashboard's one dispatch — takes a steady 7.5 s on
this instance** (7.50 s and 7.70 s on back-to-back warm calls; 859 chats, 32 live
characters, 8 projects). The first page load felt like ~30 s because it also
raced the boot embedding backfill. Nothing is wrong with the result and no v4
comparison was run, so this is not filed as a divergence — but the app's front
door costing seven and a half seconds of server time on real data is worth its
own look. Recorded here for the next `/setupphase`.

## Traps banked by this walk

1. **A `textContent` word-boundary regex cannot see a badge.** Adjacent badges
   render as `archivedtopbottom`, so `/\barchived\b/` matches nothing and reads
   as "the badge is missing". Count badge **elements**.
2. **Substring matching lies about wardrobe pools.** `Naked` "appears" in a pool
   that holds only the live composite `Naked Marguerite`. Match the quoted title.
3. **`read_console_messages` returns console history across reloads.** Two stale
   pre-fix `NG0201`s surfaced long after the fix and a reload. Arm a fresh
   in-page `console.error` hook per gesture.
4. **Read the whole response envelope before calling something missing.** The
   Workbench's broken definitions live under `errors`, not `tools`; looking only
   at `tools` manufactured a finding that did not exist. (Same family as 1 and 2:
   three false negatives in one pass, all from a too-narrow probe.)
5. **A missing DOCUMENTS group in search is usually v4's no-per-type-quota rule**,
   not a broken chip: 100 message hits saturate the 50-slot page.
6. **The Browser pane's `computer scroll` action times out here** ("the pane is
   hidden"), and `Page_Down` goes to whatever input has focus. Scroll the
   workspace's own `.qt-tab-pane` — and note there is one such element **per
   mounted tab**; pick the visible one or you will scroll a hidden pane and
   conclude nothing moved.
7. **A Pascal effect referencing a key that does not exist yet is SKIPPED**, and
   the skip is invisible in `pascalMeta` (v4 omits the key when the applied list
   is empty). It reads exactly like a dead feature.

## Result

| part | rows | outcome |
|---|---|---|
| A — dressing instructions | 10 | 10 PASS |
| B — archive instead of delete | 12 | 12 PASS |
| C — the Documents chip | 10 | 9 PASS + 1 FAIL→FIXED (#105) |
| D — the `8f910137` 💸 queue | 4 | 4 PASS |
| E — the standing 💸 remainder | 5 | 2 PASS, 3 DEFERRED-TO-HUMAN |

**41 rows: 37 PASS, 1 found-and-fixed, 3 deferred to the human.**

One v5 defect found and fixed (**#105**, commit `599f6be9`). No v4 bugs to file.
Zero panics in ~2 hours against the real 816 MB instance
(`RUST_BACKTRACE=1`, `combined.log` clean of `ERROR`).

**Discharged from the 💸 queue:** the per-tier dressing cascade on a real
"Let character choose" turn (both the character and project tiers), archived
garments absent from the Green Room pool, the archive walk end-to-end on real
data (five surfaces, both scopes), the Documents chip over the real stores, the
in-chat scenario picker, the gallery download + detail-modal controls, the
restyled `qt-*` surfaces, a real `docs --instance <TAB>` completion, and **two of
Pascal's three remaining write paths**.

**Still owed:** Pascal's **group** tier (needs a single-group chat), the Brahma
budget on a deep query, memory dedup + conversation-summaries, and the NanoGPT
caching smoke / #101 cost question — the last three all human calls on cost.
