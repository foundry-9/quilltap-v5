# Dogfood walk — the `p4.9k` + `2f4254b42` rounds (character generators, character subprompts)

**Date:** 2026-09-07 · **Instance:** `~/qt-dogfood-friday` (a COPY of Friday,
rsynced 2026-09-07 15:25–15:29 local) · **Server:** `target/release/quilltap-web`
(built 15:51) serving `apps/web/dist/quilltap/browser` · **Driver:** Claude
through the Browser pane; the human unlocks and takes the deferred rows.

Rounds under test:

- **the `p4.9k` character-generators round** (P4.9K0 → K1 ∥ K2 ∥ K3 ∥ K4 ∥
  P4.80 ∥ P4.81, unified 2026-09-07) — the generator substrate + the whole
  edit-side/detail-side SPA (beats were gated then), `chatDelete` (dogfood
  #117), the two host wires, the `text_block_turn` marker, the three
  chain-stop log lines, the composer's wide `hasActiveCharacters` twin.
- **the `2f4254b42` character-subprompts round** (P4.D163 → D164 ∥ D165 ∥
  K1-resumed ∥ K2-resumed, unified 2026-09-07) — the whole character-
  subprompts feature (vault storage, the participant carry, the five verbs,
  the `## Additional Instructions` block, the greeting, the green room, the
  whole SPA half) and **every generator runner LIVE** (rename, refresh-
  archive, external prompt, optimizer, wizard, AI import).

## Result

**24 rows: 22 PASS, 1 PARTIAL (C4, with its scope measured), 1 N/A, 3
DEFERRED-TO-HUMAN. ZERO v5 defects.** Five apparent divergences were run to
ground and every one proved v4-faithful or an instrument slip (§3).

The round's whole 💸 queue is discharged — the subprompts feature on real
data and **every** generator runner with real spend — and, because v4 ran
character subprompts on this instance hours before the copy was taken,
three of the headline proofs are **cross-implementation against v4's own
bytes**: v5 reads v4's subprompt file, keeps the `selectedSubpromptIds` v5
used to drop, and **rebuilds the whole compiled identity stack
byte-identical to v4's** (md5 `b9beceb0…`, 10,133 chars). The green-room
note and the optimizer's suggestions-file format are byte-checked against
v4's own artefacts too.

## §0 Pre-walk state

**Drift-ledger §2 freshness probe: PASS** (2026-09-07, before any step) —
v4 checkout on `main`, tree clean, `2f4254b42..main` EMPTY,
`1a2b2164c..bugfix` EMPTY. Baseline `2f4254b42`; **§3 is empty — zero drift**,
so no step on this walk may blame drift for a divergence.

### §0.1 The pre-walk measurement (ledger §5.5) — and what it bought

v4 shipped character subprompts at `2f4254b42` **this morning** and then ran
the feature on the live instance before the copy was taken. The copy
therefore carries v4's own bytes for the round's headline feature:

| what v4 left on the instance | measured |
|---|---|
| `Subprompts/` vault folders | **1** (Abigail, mount `99586a4e…`) |
| subprompt files | **1** — `Subprompts/initiating.md`, 146 chars, written `2026-09-07T14:45:28.837Z` |
| chats whose participants carry `selectedSubpromptIds` | **3** (`c72ff089` "Emerald Hair and the Wife's Gift", `a3dba0f4`, `d2f44d34`) |
| seats with a NON-empty selection | **1** — Abigail in `c72ff089`, `["initiating"]` (the id is the SLUG) |
| chats whose `compiledIdentityStacks` already carries `## Additional Instructions` | **1** — `c72ff089`, envelope `version: 2` |
| `llm_logs` rows carrying the green-room fifth bullet ("Any additional instructions in play for this scene…") | **11** |

**Consequences for the plan.** Three proofs become cross-implementation
rather than self-referential, and they are the strongest rows on the walk:

1. v5 must READ v4's subprompt file back (A1) — v4 wrote it, v5 renders it.
2. v5 must not LOSE the carry (A2) — P4.D163 unit 1 was a measured
   data-safety fix: before it, every v5 participant rewrite dropped a
   v4-written `selectedSubpromptIds` on this very instance.
3. v5's `## Additional Instructions` block must equal **v4's baked bytes**
   (A3), and at `version: 2` v5's strict-equality read must ACCEPT v4's
   cached stack rather than rebaking it.

v4's block, captured before the walk (structure quoted; the body is the
user's own private content and is compared by hash/shape, not reproduced
here):

```
## Additional Instructions
The following also apply to you in this conversation.
### Initiating
<146 chars of instruction text>
```

placed after the personality paragraph and before `## Character Manifesto`.

### §0.2 Scale of the copy

918 chats · 135,381 chat messages · 45 characters (10 archived) · 49
connection profiles. Reinforced about-self memories (the optimizer's
precondition, ≥ 2): the top character has **181**. Largest chat: "The Bridge
of Ordinary Breathing", 1,934 messages.

### §0.3 What is NOT expected to work (from the order headers — do not file these)

- **`aiImportStream`'s JSON-Schema validation step**: the port records a
  `VALIDATION_UNAVAILABLE` refusal — no JSON-Schema crate is linked
  (P4.9K2's flagged dependency question). A skipped/refused validation step
  in an AI import is the RECORDED divergence, not a defect.
- **SSE Tier-2 remainders (K1/K2 item 9/10, OPEN):** the two generator SSE
  edges assert shape only; neither `Cache-Control` nor `Connection` is
  compared, and `llm_logs` row counts per case are recorded, not compared.
- **`CHARACTER_HEADSHOULDERS_BACKFILL`** is still the named-refusal row in
  the job runner (K2 Tier-3 recording OPEN).
- **P4.81 item 7** stays MEASURED-not-fixed (the shared default-profile flip).
- Two recorded v4-truthiness arms in the wizard: a truthy NON-ARRAY
  `sourceFileIds` answers 400 where v4 streams to a `done{_fatal}`; a truthy
  non-array `regenerateSteps` is dropped where v4 keeps it. Both are
  recorded-not-fixed, with a ruling owed.

## §1 The walk

Status key: `PENDING` → `PASS` / `FAIL(#finding)` / `DEFERRED-TO-HUMAN` /
`BLOCKED(reason)`.

### Part A — character subprompts (P4.D163 ∥ P4.D164 ∥ P4.D165)

| # | Owner | Step | Gesture (broad, not the beat's) | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | v5 reads v4's subprompt | Character edit → Abigail → **System Prompts** tab → the Subprompts section | The one row `Initiating` renders with v4's 146-char body; bytes equal the vault file (`quilltap db --mount-points` + the blob) | **PASS** |
| A2 | CLAUDE | **the carry survives a foreign participant write** | In chat `c72ff089`, change something on the participant card that is NOT subprompts (impersonate toggle / a seat setting), then re-read `chats.participants` | Abigail's `selectedSubpromptIds` still `["initiating"]` — the unit-1 data-safety fix, measured against v4's own row | **PASS** |
| A3 | CLAUDE | **the `## Additional Instructions` block vs v4's baked bytes** | (a) boot/read the chat and confirm `compiledIdentityStacks` is UNCHANGED (v5 accepts v4's version-2 bake); (b) force a rebake (toggle the selection off/on) and diff v5's produced block against §0.1's capture | (a) md5 of the column unchanged; (b) v5's block byte-identical to v4's | **PASS** |
| A4 | CLAUDE | subprompt CRUD + the vault write | Create a subprompt through the editor modal; edit it; delete it | `Subprompts/<slug>.md` appears in the mount index with v4's frontmatter shape; the delete fans out (the id leaves every carrying selection) | **PASS** |
| A5 | CLAUDE | the picker on the Salon participant card + New Chat | Select on the card in a chat that has none; and select at chat creation. **Correction to the plan:** there is exactly ONE New-Chat picker mount (the multi-character picker panel) — the single-character one is a recorded NO-COUNTERPART (`new-chat-form.ts:68-80`), not a missing port | Persisted to `chats.participants`; realtime hint published | **PASS** |
| A6 | CLAUDE | the green room's fifth bullet + the debug line | Trigger a real "let the character choose" dressing consult on a chat with a selection | `combined.log` carries `Subprompts in play for the green room` with `chat_id`, `character_id`, `count` (the §3 review fix); the outfit prompt carries the bullet + the note | **PASS** |
| A7 | CLAUDE | the greeting's subprompt context | Create a new chat with Abigail and the subprompt selected at creation | The greeting call's context carries the subprompts (RAW six-key); one LLM call | **PASS** |
| A8 | CLAUDE | the five verbs' guard ladders | curl the REST edges: missing character, bad body, unknown action | v4's guard order (404 before 400) and v4's exact bodies | **PASS** |

### Part B — the generator runners, LIVE (P4.9K1 ∥ P4.9K2)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | **Rename & Replace preview does not hold the writer** | Run a dry-run rename over the real instance (918 chats / 135k messages) and issue a write DURING it | The preview completes and the concurrent write is NOT blocked — the §3 unify fix (dry run on the read pool) | **PASS** |
| B2 | CLAUDE | Rename execute | Execute a rename on a throwaway character | Occurrences rewritten; the report's counts match a SQL count | **PASS** |
| B3 | CLAUDE | refresh-archive | Run it on one of the 10 archived characters | v4's report shape; no vault damage | **PASS** |
| B4 | CLAUDE | external prompt generator (real spend) | The external-prompt dialog on a character | A generated prompt + an `llm_logs` row | **PASS** |
| B5 | CLAUDE | **the optimizer, "Refine from Memories"** (real spend) | Run on the character with 181 reinforced about-self memories; watch the SSE progress | Progress frames arrive (`generatorProgress`); bug-119 containment log lines in `combined.log`; suggestions render and apply | **PASS** |
| B6 | CLAUDE | the AI wizard (real spend) | Generate fields for a new character, streaming | Streamed frames; fields land; the ten `FIELD_PROMPTS` reachable | **PASS** |
| B7 | CLAUDE | AI import / Summon From Lore (real spend) | Import a character from a short lore text | Steps stream; the `VALIDATION_UNAVAILABLE` refusal is the RECORDED divergence | **PASS** |
| B8 | CLAUDE | the SSE edge bytes (Tier-2 item 9 is OPEN) | Capture the raw SSE stream of one generator run with curl | Records the real frame sequence — an honest complement to the open item | **PASS** |

### Part C — the `p4.9k` round's other surfaces

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | **`chatDelete` on real data** (dogfood #117) | Delete a chat from the card's trash (both card sites) | The chat and its cascade rows are gone; v4's dispatch shape | **PASS** |
| C2 | CLAUDE | the card-link button interception fix | Click a button INSIDE a chat card's link | The button acts; no navigation (the v5-only bug the round fixed) | **PASS** |
| C3 | CLAUDE | the `text_block_turn` marker's second stream | A turn that emits a text block then continues | The second stream fires | **N/A** |
| C4 | CLAUDE | the three chain-stop log lines | Stop a chain mid-turn | The three lines in `combined.log` | **PARTIAL** |
| C5 | CLAUDE | the composer's wide `hasActiveCharacters` twin | A chat whose only active character is USER-controlled | The composer is enabled and the placeholder is not `Add a character to start chatting…` | **PASS** |
| C6 | CLAUDE | P4.81 item 1 — the host's Zod `details` carry | curl a refused `chatCreate` | `details` reaches the wire through the host | **PASS** |

### Part E — added during the walk (broad gestures the specs skip)

| # | Owner | Step | Gesture | Expected | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | the wizard's cross-step gate | Step 2 `Skip physical description` → step 3 | The Physical Description row is disabled and labelled `(skipped in previous …)` | **PASS** |
| E2 | CLAUDE | `Select all` + `Background Context` | Both, then generate | 8 enabled fields queued; the context reaches the model | **PASS** |
| E3 | CLAUDE | subprompt collision + 100-char title | two identical titles; a 100-char title | `-2` suffix; a 60-char slug | **PASS** |

### Part D — the standing 💸 queue (deferred)

| # | Owner | Step | Why deferred | Status |
|---|---|---|---|---|
| D1 | HUMAN | the Brahma budget on a deep query | up to 50 agent turns of real spend | DEFERRED-TO-HUMAN |
| D2 | HUMAN | memory dedup + conversation-summary regeneration, first run | batch cost over 135k messages | DEFERRED-TO-HUMAN |
| D3 | HUMAN | NanoGPT prompt-caching cost question (#101) | a billing question, not a code question | DEFERRED-TO-HUMAN |

## §2 Results in detail

### A1–A3 — the cross-implementation trio (PASS)

**A1.** `characterSubpromptList` on Abigail returns exactly v4's row —
`{id: "initiating", path: "Subprompts/initiating.md", title: "Initiating",
content: <the 117-char body>, updatedAt: "2026-09-07T14:45:28.837Z"}` — and
the vault file itself reads
`---\ntitle: Initiating\n---\n\n<body>\n` (146 chars, matching
`plainTextLength`), so v5 parses v4's frontmatter and body byte-for-byte.
The character-edit **System Prompts** tab renders the row with its title,
its `Subprompts/initiating.md` path and its body; the Salon participant
card's picker reads it as **`Subprompts · 1 of 1 in play`**.

**A2 — the unit-1 data-safety fix, proven against v4's own row.** A FOREIGN
participant write (Abigail's *Participation status* `active → silent`,
through the sidebar select) and its restore both landed, and
`selectedSubpromptIds: ["initiating"]` survived both. Before P4.D163 unit 1
this write dropped the key.

**A3 — v5's bake is byte-identical to v4's.** v4 left
`chats.compiledIdentityStacks` on `c72ff089` at envelope **version 2**,
md5 `b9beceb0cba36f8d8474d0d9cf132766`, 10,133 chars, carrying a 216-char
`## Additional Instructions` block (md5 `21923b8ca30f5aced8e4bc1faaa18497`)
between the personality paragraph and `## Character Manifesto`.

- Two participant writes left it **untouched** — v5's strict-equality read
  accepts v4's cache rather than recompiling it.
- Unticking the subprompt **rebaked and dropped the block**: 10,133 → 9,910
  chars, `## Additional Instructions` absent (clear-on-drop).
- Re-ticking rebuilt the **whole stack byte-identical to v4's** — same md5,
  same length, same version, and the block itself byte-for-byte.

That single toggle proves the render, the compiler bake, the version
envelope and both cache transitions at once, against v4's own bytes.

### A4–A5 — CRUD, the slug rules, and the fan-out (PASS)

Walked on **Ariel** (no `Subprompts/` folder before the walk), so the
first-write folder creation was exercised: `doc_mount_folders` gained a
`Subprompts` row on Ariel's mount at create time, the same shape v4's row
on Abigail's mount has.

- **Slugging, against v4's real `slugifySubpromptTitle`:** the deliberately
  awkward title `Keep it Brief — no spoilers! (walk probe)` became
  `Subprompts/keep-it-brief-no-spoilers-walk-probe.md` — exactly v4's
  NFKD → lowercase → `[^a-z0-9]+`→`-` → trim → 60-slice rule. The
  frontmatter keeps the original title, em dash intact.
- **The collision suffix:** a SECOND subprompt with the identical title
  landed at `…-walk-probe-2.md` — v4's `${base}-${n}` loop.
- **Update keeps the path:** retitling `-2` to `Totally Different Name`
  left `Subprompts/keep-it-brief-no-spoilers-walk-probe-2.md` in place, with
  v4's note rendered (`Kept on file as … The file name stays put when the
  title changes…`) and the toast `Subprompt updated`.
- **The picker on the Salon card** ticked both onto Ariel's seat in
  `d2f44d34` (5 seats) and persisted `["…-2", "…"]`; the user seat has no
  picker and the four LLM seats do.
- **The delete fan-out**, the round's own log line, verbatim:
  `Subprompt change fanned out … remove_selection=true chats_touched=1
  seats_recompiled=1`, with the id gone from Ariel's selection and the
  vault file gone. The earlier UPDATE fanned out with `chats_touched=0`
  because nothing carried it yet — the two arms discriminate.

**Not a defect, measured:** a collapsed picker with no selection reads
`None on file` before it has looked — v4's `useCharacterSubprompts` defers
the query identically (`enabled: open || selectedIds.length > 0`) and
computes the same summary string, so v5 is faithful.

**Not a defect, measured:** two checkbox clicks inside ~50 ms lose the
first (the second emits from a `selectedIds` prop the server round-trip has
not yet updated). v4's picker is the same controlled shape with the same
async parent, so the window exists there too. Sequential ticks persist both.

### A8 — the guard ladders (PASS, read against v4's real route)

Eight probes against `/api/v1/characters/{id}/subprompts[/{sid}]`, each
compared to `app/api/v1/characters/[id]/subprompts/route.ts` at the
baseline:

| probe | v5 | v4's rule |
|---|---|---|
| GET, missing character | `404 {"error":"Character not found"}` | `notFound('Character')` |
| GET, non-uuid id | 404, same body | `findByIdRaw` finds nothing |
| POST `{}` | `400 Validation error` + both Zod-4 `invalid_type` issues | `createSubpromptSchema.parse` throws |
| POST `{}` on a MISSING character | **400, not 404** | **v4-faithful** — v4 parses the body BEFORE the existence check (`route.ts:50-56`) |
| POST on an ARCHIVED character | `409 {"error":"Character is archived; subprompts cannot be added"}` | v4's `CharacterArchivedError` arm, byte-exact |
| POST title 101 chars | `400` `too_big … <=100 characters` | `SUBPROMPT_TITLE_MAX_LENGTH = 100` |
| POST title exactly 100 | `201`, id slugged to exactly **60** chars | v4's `.slice(0, 60)` |
| POST `content: ""` | `400` `too_small … >=1 characters` | `z.string().min(1)` |
| GET / DELETE an unknown subprompt id | `404 {"error":"Subprompt not found"}` | `notFound('Subprompt')` |

### A6–A7 — the green room and the greeting (PASS, both against v4's own bytes)

**A7 (the greeting / chat-creation context).** A new chat created from the
New-Chat picker with `initiating` ticked persisted
`selectedSubpromptIds: ["initiating"]` on the seat at CREATE, baked the
block into the new chat's `compiledIdentityStacks`, and — the direct proof —
its persisted 11,713-char SYSTEM message contains v4's 216-byte block
**verbatim** at offset 4,595 (the greeting takes the RAW context, not the
compiled stack, so this is the independent leg).

**A6 (the green room).** Chat creation with *Let character choose* fired a
real outfit consult, and its `llm_logs` request carries both halves:

- the **fifth bullet** in the system message — `- Any additional
  instructions in play for this scene, when provided — smaller standing
  directions the character is following in this particular conversation;
  honour anything in them that bears on dress` — after the
  dressing-instructions bullet;
- the **`subpromptsNote`** in the user message, before `Available Wardrobe
  Items:`.

Both are cross-checked against v4's own rows on this instance: **all four
v4-written consults and v5's carry the bullet verbatim**, and the ONE
v4-written consult that had a subprompt in play carries a note that is
**byte-identical to v5's** (246 chars, `IDENTICAL: True`).

The §3 review's own fix is live too — with `RUST_LOG=…quilltap_core=debug`
the run logs
`[applyOutfitSelections] Subprompts in play for the green room
chat_id=d59b4fa5… character_id=af38f265… count=1`, carrying the `chat_id`
the pre-fix site (inside `choose_llm_outfit`) could not have.

Incidentally the note *worked*: with `initiating` in play the model dressed
Abigail in a single accessory and nothing else.

### B1–B4 — rename, refresh-archive, external prompt (PASS)

**B1 — the §3 unify fix proven at real scale.** A dry-run rename preview over
the whole instance (`e` → `3`, case-insensitive) scanned **6,858,481
occurrences** — 6,736,603 of them in chat messages — in **1.65 s**, and
**five writes landed DURING it**: subprompt creates at t+0.07 s … t+1.37 s,
every one `201` in 13–14 ms. Before the unify fix the dry run held the RW
writer for the whole scan, and those writes would have queued behind it.
A narrower preview (`Abigail` → `Abigaila`, case-sensitive) returned v4's
summary shape with `chatMessages: 42678, total: 43980` in 1.26 s.

**B2 — rename EXECUTE with the gestures no spec makes.** On a throwaway
character: the **Additional Replacements** row (`Add` → `walk-probe` →
`WALK-PROBE`) with its **per-pair `Aa` case-sensitive checkbox** ticked, the
preview's counts grid (`5 / 0 / 0 / 0 / 0 / 5`) and its
`Replacements (5)` table, then `Execute 5 Replacements` behind the native
confirm — whose text is v4's byte-exact
`Are you sure you want to rename this character and update 5 occurrences?
This action cannot be undone.` Result: the DB row renamed, and the vault's
`identity.md` / `description.md` / `personality.md` rewritten, with the
case-sensitive pair landing as `WALK-PROBE`. Both log lines fired with v4's
fields (`total_changes=5`, `character_fields_changed=["name","identity",
"description","personality"]`, `duration_ms=27`).

**B3 — refresh-archive (no UI exists; driven by REST and by the verb).**
Both arms match v4's bodies exactly: a character with no chats answers
`{"queued":0}` (no `total` key — v4's shape), a two-chat character
`{"queued":2,"total":2}` through both the REST action and the
`characterRefreshArchive` verb, with v4's log line and its four fields.
*Measured, v4-faithful:* three concurrent calls each report `queued: 2`
while only 4 render jobs exist — because v4's `enqueueConversationRender`
REUSES a pending job and returns `{isNew:false}` rather than throwing, so
v4's own `catch` comment ("Skip chats that already have a pending render
job") describes an arm v4 cannot reach either.

**B4 — the external-prompt generator, live, with every control moved.**
Profile switched off the default, the optional **Scenario** select set (the
spec never touches it) and **Maximum Output Size** dragged to 1,500 (readout
`1,500 tokens · ~6,000 characters`). A real Z_AI `glm-5.3-flash` call
returned in **8,846 ms**, 3,237 tokens, `output_length: 5449` — inside the
requested cap — with all three log lines carrying `scenario_id` and
`max_tokens`, and an `EXTERNAL_PROMPT` `llm_logs` row holding the real
duration and usage.

**Not a defect, measured (a false alarm run to ground):** v5's three
`CHAT_MESSAGE` rows this walk carry `connectionProfileId = NULL` while v4
attributes 1,057 of 1,078. All three of v5's are chat-creation greeting
calls — and **all 19 of v4's own null `CHAT_MESSAGE` rows since 2026-08-25
land 6–42 s after their chat's `createdAt`**, i.e. every one is a greeting
call too. The greeting path is unattributed in v4 as well.

### B5 — the optimizer's VAULT arm (PASS; no spec covers this arm at all)

Run on **Amy** (181 reinforced about-self memories) with every preflight
control moved — a non-default model (`glm-5.3-flash`), **Maximum Memories
to Analyse** dragged 30 → **10**, `Filter Memories` opened with **Search
Query `boundaries`** and the semantic checkbox left on, and **Save as
suggestions in the vault** ticked. None of that is touched by
`character-optimizer-flow.spec.ts`, which clicks `Commence Refinement`
with the defaults.

Everything reached the server: `output_mode: "suggestions-file"`,
`max_memories: 10`, `search_query: "boundaries"`,
`use_semantic_search: true`. The progress panel showed v4's three segments
and the SEMANTIC counter line — **`27 memoirs matched; top 10 selected for
analysis`** (the `<f> memoirs matched; top <n> selected` arm, not the plain
retrieval one). Seven `CHARACTER_OPTIMIZER` calls, 140.8 s total, **all
seven carrying `connectionProfileId`** — matching v4's own 10-of-10.

The terminal wrote `Suggestions/refinement-2026-09-07-212627.md` (6
patterns → 6 suggestions), and **v4 left two of its own suggestion files on
this instance from April**, so the format is cross-checked: identical
frontmatter keys in identical order (`type: character-suggestions`,
`generatedAt`, `characterId`, `characterName` quoted, `model` quoted,
`memoryCount`, `suggestionCount`) and identical prose —
`# Refinement Suggestions — <date>` / `The automata have consulted <n>
memoirs from <Name>'s Commonplace Book and offer the following proposals…`

*Not observed:* the modal's `Suggestions Inscribed` terminal card — the
workspace tab it lived in was navigated away mid-run. The run finished and
persisted regardless, which is the property that matters.

### C1–C2 — `chatDelete` and the card-link interception (PASS)

The Salon card's trash button **is inside the card's `<a>`** (verified in
the DOM: `del.closest('a')` is the card link) and clicking it **does not
navigate** — the v5-only capture-phase interception bug this round fixed.
The gate is `window.confirm` with v4's byte-exact sentence
`Are you sure you want to delete this chat?`; accepting it removed the chat
(921 → 920), cascaded its 8 messages, and logged both lines — `Chat deleted`
(`conversation_summary_vault_bridge`) and `[Chats v1] Chat deleted`.

*Not a defect, measured:* one `conversation_chunks` row survived the delete
— and the instance already carried **four v4-written orphan chunks** (from
2026-08-13 and 2026-08-25, for chats v4 itself deleted). v4 leaks them too.

### B6–B8 — the wizard, the AI import, and the raw SSE bytes (PASS)

**B6 — the AI Wizard with three uncovered gestures.** Step 2 set to **Skip
physical description** (the spec leaves the default), which then rendered
the step-3 Physical Description row **disabled and labelled `(skipped in
previous …)`** — a cross-step effect nothing else exercises; **`Select all`**
(8 enabled fields; the four `(has content)` rows correctly disabled) and a
**Background Context** the model demonstrably read ("brass airship",
"archivist"). All 8 streamed with per-field ✓ rows and snippets, then
`Generation Complete` → `Apply to Character` → `Save Character`. The
artifacts are real: `manifesto.md` 600 chars, `example-dialogues.md` 2,441,
`properties.json` carrying the generated title, **3 `Scenarios/*.md`** and
**10 `Wardrobe/*.md`** written into the vault during generation (the
host-applied composites). `wardrobe_items` stays 0 — correct, the whole
instance keeps wardrobe in the vault (585 files, 0 rows).

**B7 — Summon From Lore, driven from the `/characters` toolbar mount** (the
spec walks only the Salon cast mount). Freeform lore only, `Generate
Example Chat` ticked (the spec leaves it off), a chosen profile. All eleven
glyph rows progressed ✓/●/○ and the run produced 5 physical variants, 2
system prompts, 11 wardrobe items + 2 outfits, 12 memories, a 9-message
example chat — then `Import Character` → **`Import Successful! … (13
entities imported)`**, with the character, its 12 memories and its vault on
disk.

⭐ **The recorded `VALIDATION_UNAVAILABLE` divergence is user-visible and
honest**, exactly as P4.9K2's header says it should be — the Review step
carries `Some steps had issues (non-critical): validation: Schema
validation is not available in this build: no JSON-Schema engine is
linked, so the assembled export is returned unvalidated (and unrepaired)`
and the import proceeds. Nothing is silent.

**B8 — the raw SSE bytes** (an honest complement to K1/K2's OPEN Tier-2
item 9). `POST /api/v1/characters?action=ai-wizard-stream` returned:

```
HTTP/1.1 200 OK
content-type: text/event-stream
cache-control: no-cache
connection: keep-alive

data: {"type":"start"}

data: {"type":"field_start","field":"title"}

data: {"type":"field_complete","field":"title","snippet":"…"}

data: {"type":"done","fullContent":{"title":"…"}}
```

v4's handler (`app/api/v1/characters/handlers/post.ts:548-576`) sets
**exactly those three headers** and frames as
`data: ${JSON.stringify(event)}\n\n`. **The live bytes match v4's; it is
only the lane's ASSERTIONS that stop short** — the item-9 remainder is a
test-coverage gap, not a wire gap. (The wizard's own Zod arm was measured
on the way: a body missing `sourceType`/`characterName`/`background`/
`fieldsToGenerate` answers v4's 400 `Validation error` with all four issues.)

**C6 — the P4.81 host wire.** A refused `chatCreate` now answers with the
Zod `details` array carried through the host (`{"kind":"bad-request",
"message":"Validation error","details":[…]}`), including dogfood finding
#115's own shape (`controlledBy: "LLM"`), which refuses at the validation
stage with the two expected issues.

### C5 — the composer's wide `hasActiveCharacters` (PASS)

The discriminating shape had to be built, because **no chat on the real
instance has only user-controlled character seats**. Flipping Abigail's
seat to `User (you type)` in chat `d59b4fa5` left BOTH character seats
`controlledBy: 'user'` — the exact case where the pre-fix narrow predicate
(`controlledBy === 'llm'`) reports zero active characters. The composer
stayed **enabled** (`contenteditable="true"`) and the page never showed
`Add a character to start chatting…`, which is v4's wide
`type === 'CHARACTER' && isActive`. The seat was restored to `llm`
afterwards (verified in `chats.participants`).

### C3 — reclassified N/A

P4.81 item 3 (`text_block_turn`'s marker) is a change to a committed
harness FIXTURE, not a live surface — there is no browser gesture that
exercises it. Its proof is the family's own mutation, already recorded in
the round.

### C4 — the three chain-stop log lines (PARTIAL, with the scope measured)

**`[TurnOrchestrator] Chain error, stopping` proven LIVE**, at ERROR level,
with v4's context bag intact. Built by pointing a chained seat at a dead
OpenAI-compatible endpoint (`http://127.0.0.1:9/v1`):

```
level=error  [TurnOrchestrator] Chain error, stopping
  chat_id=d59b4fa5… chain_depth=1 user_id=…
  error: primary stream failed: error sending request for url (http://127.0.0.1:9/v1/chat/completions)
```

and the site's **safety pause landed** (`chats.isPaused = 1`) — bug 123's
announced pause, from the branch that sets it. The same window gave a free
live proof of P4.D135's failover chain: `Fallback chain built` →
`[Failover] Primary call failed; walking the fallback chain` (warn) →
`[Failover] Fallback chain exhausted` (error).

**The other two are not browser-reachable, and the walk measured why:**

- `singleTurn: skipping chain loop` — the ONLY production site that sets
  `single_turn: true` is the autonomous-room enclave
  (`enclave/step.rs:687`); every interactive path passes `false`
  (`host/spine.rs:1517`). A `Nudge` on a multi-character chat therefore
  takes the chain loop, which it did — 8 × `[TurnOrchestrator] Chain
  decision: continue`. Closing this row needs an autonomous-room run.
- `Chain stopped: empty response` needs a chained turn that returns no
  content AND no skip — a provider stub, not a gesture.

Both are already unit-pinned with mutation proofs over the guard table
(`orchestrator.rs:4598-4765`).

## §3 Findings

_No v5 defect was found by this walk._ Five observations were run to ground
and every one turned out to be v4-faithful or an artefact of the
instrument; they are recorded inline above and summarised here:

1. A collapsed, unselected subprompt picker reads `None on file` before it
   has looked — v4 defers the same query and computes the same string.
2. Two checkbox clicks inside ~50 ms lose the first — v4's picker is the
   same controlled shape over the same async parent.
3. `refresh-archive` reports `queued: n` even for chats whose render job is
   already pending — v4's enqueuer reuses rather than throwing, so v4's own
   "skip" catch is unreachable there too.
4. v5's `CHAT_MESSAGE` `llm_logs` rows carried no `connectionProfileId` —
   but **all 19** of v4's own null rows since 2026-08-25 are greeting calls
   too (6–42 s after their chat's `createdAt`), which is exactly what v5's
   three were.
5. One orphaned `conversation_chunks` row survived a chat delete — the
   instance already carried four v4-written orphans of the same shape.

_(appended as they are found)_

## §4 Instrument notes (the standing rule: prove the instrument first)

1. **A `ref` click can land on stale coordinates while the Browser pane is
   hidden.** `computer{left_click, ref}` on the subprompt picker reported a
   coordinate and did nothing (`aria-expanded` stayed `false`); the same
   element's `.click()` opened it. Two earlier ref clicks in the same
   session DID work, so this is intermittent — **read back the state the
   click was supposed to change before trusting it**. It very likely also
   swallowed the "Rebuild system prompt" click that made A3(a) look
   inconclusive.
2. **`window.confirm` returns FALSE by default in the pane.** The first
   chat-delete click looked like a no-op; the gate is `window.confirm` and
   nothing had accepted it. Stub it (and capture its text — the sentence is
   itself a comparand).
3. **Walking up from a link to "the card" can climb to the whole page.** A
   `parentElement` loop that stops at "an ancestor containing buttons"
   selected a container with 829 Delete buttons, so the click hit the wrong
   chat's control. Stop at a NAMED host (`qt-chat-card`) and assert the
   count is 1.
4. **A background waiter's own pattern can match unrelated text.** An
   `until grep -qi '…\|Suggestions'` loop exited immediately because
   `output_mode: "suggestions-file"` was already in the log — the
   self-matching class the standing rule warns about, in a new disguise.
   Wait on the completion sentence, not on a word from the topic.
5. **A DB read taken seconds after a UI action can read the pre-write
   state.** `equippedOutfit` read `null` right after a green-room chat
   creation and was populated moments later; `llm_logs` was empty for a
   chat that had in fact logged. Re-read before concluding an absence.
6. **`llm_logs.request` is a pre-builder projection but it DOES carry the
   system message's text** — enough to compare a rendered prompt section
   byte-for-byte, which is how A6 and A7 were proven.

## §5 What the walk left on the copy

The copy is disposable and is replaced by the next rsync, so the walk's
evidence was left in place: two throwaway characters
(`Quillberta Walkington` from the rename/wizard rows, `Ferrous Kettleby`
from the AI import), two throwaway chats, and Amy's new
`Suggestions/refinement-2026-09-07-212627.md`. Everything that touches
**v4's own data was restored and verified**: Abigail's seat in `c72ff089`
carries `["initiating"]` exactly as v4 left it, the vault holds only v4's
`Subprompts/initiating.md`, **no chat carries a dangling subprompt id**,
the walk's dead-endpoint connection profile is deleted (49 profiles, as
before), and Abigail's seat in `d59b4fa5` is back on an LLM profile.
