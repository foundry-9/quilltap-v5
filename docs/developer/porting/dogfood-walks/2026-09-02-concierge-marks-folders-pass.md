# Dogfood walk — the `6d2a50382` drift catch-up round

**Date:** 2026-09-02 · **Instance:** `~/qt-dogfood-friday` (a COPY of Friday;
never rsynced back) · **Driver:** Claude, with a short HUMAN remainder.

**Round under test:** the `6d2a50382` drift catch-up round
(P4.D143 ∥ P4.D144 ∥ P4.D145 ∥ P4.D146 ∥ P4.D147), unified 2026-09-02.
Plus the cheap end of the standing 💸 queue.

---

## Drift note — the ledger is FRESH

The §2 freshness probe **passed** at walk start: v4 checkout on `main`, tree
clean, `git log 6d2a50382..main` empty, `git log 3a76b17df..bugfix` empty.
**§3 is EMPTY — zero drift.** So nothing in this walk gets the "it may be the
drift" excuse: an apparent divergence here is a v5 finding until proven
otherwise, diagnosed against the checkout itself (no pin needed).

One record correction found while probing: ledger §1 calls `6d2a50382`
v4 `4.9.0-dev.113`; the commit's own `package.json` says **`4.9.0-dev.115`**.
Immaterial to the verdict (origin/main is not ahead), but the field is wrong.

---

## Pre-walk measurements (read-only, before the server booted)

Ledger §5.5 — v4 runs daily on the real instance and heals data out from
under banked proofs. Two of this round's six 💸 items were killed by exactly
that, and the measurements below reshape their steps.

| what | measured | consequence |
|---|---|---|
| `folders` rows | **24** (was 607 at P4.D145's measurement) | ⚠ **the 607-row collapse proof EXPIRED** — see below |
| `idx_folders_userId_projectId_path` | **present**, UNIQUE, `COALESCE("projectId",'')` | v4 already created it |
| `migrations_state` `collapse-duplicate-folders-v1` | **2026-09-02T11:55:11.796Z**, v4 `4.9.0-dev.115`, `itemsAffected 583`, *"Collapsed 583 duplicate folder rows into 24 folders"* | v4 ran its own bug-114 migration hours before the rsync — **exactly** the 607/24/583 shape P4.D145 asserted |
| project `properties.json` background modes | 8 files: **6 × `latest_chat`, 2 × `theme`, 0 retired** | no natural retired-mode subject; must be planted |
| chats total | **888** |  |
| → derived `uncensored` (`conciergeOverride='UNCENSORED'`) | **2** | the mark's rarest state exists |
| → derived `vouched` (`'OFF'`) | **10**, of which **3 carry `isDangerousChat=1`** | ⭐ the 3 are the *discriminator* — see step B2 |
| → derived `flagged` (NULL + `isDangerousChat=1`) | **73** |  |
| → derived `monitored` (the rest) | **803** |  |
| chats with an **absent** participant | **61** | the absent-participant gate has real subjects |
| chats with a **removed** participant | **2** |  |
| chats with a **silent** participant | **5** | silent counts as PRESENT — the negative arm |
| folders per project | one project with **7**, several with 2–3, all **flat** (`parentFolderId` NULL everywhere) | no natural nested folder → the indent case must be created |
| implied-only folder | `projectId NULL`, `folderPath` **`/archives`** (10 files, **no trailing slash**, no `folders` row) | ⭐ a real implied folder with a path-shape quirk — free picker material |

### ⚠ What the folders measurement costs, and what it buys

**Costs:** the banked "watch v5 heal 583 duplicates on real data" proof is
gone. v4 healed them first.

**Buys, and it is worth more:**

1. A **free cross-implementation proof**. v4's independent migration produced
   `24` folders from `583` duplicates — the identical shape
   `folders_collapse_heal_equivalence`'s Friday scenario asserts. Two
   implementations, same answer, on the same bytes.
2. A **cross-app hazard that only real data can pose**: v5's port
   deliberately writes **no** ledger row and guards on the index. It must now
   boot against a database where v4 *did* write one and the index already
   exists — do nothing, log nothing alarming, write no duplicate row. That is
   step A1 and it is a better test than the heal was.
3. The heal itself is still reachable by **planting** (step A2) — this copy
   is disposable.

---

## What NOT to expect to work

Listed so nothing here gets reported as a bug:

- **`dangerCategories` on a list payload.** Recorded in P4.D144: the mark
  bubble's `Categories` line is unit-pinned only — nothing a walk can reach
  writes the field. An empty categories line is correct.
- **A project-generated or static-image background.** Both modes were retired
  *because they never produced an image*. A coerced project shows **no**
  picture; that is the fix, not a failure.
- **A ledger row from v5's folder ensure.** By design (P4.D145). Its absence
  is the assertion.
- **The quick-hide footer section when nothing qualifies.** It is gated on
  `hasQuickHideFeatures` (v4's own gate). With no flagged tag, the toggle
  off, and no uncensored-route chat, it is *supposed* to be absent.
- **`console.warn` on the quick-hide probe.** v5 warns where v4 is silent —
  a recorded, accepted divergence, not a finding.

---

## The walk

Status: `PENDING` → `PASS` / `FAIL(#finding)` / `DEFERRED-TO-HUMAN` /
`BLOCKED(reason)`.

### Part A — bug 114: one folder row per path (P4.D145)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | **Boot against v4's already-healed database** | Started the server on the fresh copy; read the boot log. | **PASS.** Zero folder/collapse/index lines in the boot log; `folders` still **24**; `migrations_state` still exactly **1** `collapse-duplicate-folders-v1` row, still stamped `2026-09-02T11:55:11.796Z` / `4.9.0-dev.115` / 583 — v4's row, untouched, and v5 wrote none of its own. **A free cross-app proof: v5 honours a heal another implementation performed.** | PASS |
| A2 | CLAUDE | ⭐ **Plant duplicates and watch v5 heal them** | Server down: dropped the unique index; planted **3** duplicate `/reports/` rows (projectId NULL) and **1** duplicate `/Gary/` (project-scoped, to exercise the `COALESCE(projectId,'')` leg), all with LATER `createdAt`; plus a child `/reports/sub/` whose `parentFolderId` named **duplicate #2** — a row destined for deletion. Restarted. | **PASS on every leg.** Boot log: `Collapsed duplicate folder rows scanned=30 surviving=26 deleted=4 repointed=1`. 30 → **26** rows, **0** duplicate `(userId, COALESCE(projectId,''), path)` groups, unique index **recreated**. The orphaning child's `parentFolderId` moved `aaaa0002…` → **`99bcd2b7…`, the survivor**, `updatedAt` stamped at boot. Survivors are the **oldest `createdAt`** in both legs (`/reports/` 2026-01-16; `/Gary/` 2026-04-19). ⭐ And `migrations_state` still holds **only v4's row**, byte-unchanged (`2026-09-02T11:55:11.796Z` / `4.9.0-dev.115` / 583) — v5 collapsed four rows and wrote **nothing**, the deliberate no-ledger-row design proven against a real cross-app ledger. | PASS |
| A3 | CLAUDE | **The index actually enforces** | With the index present, inserted a row duplicating `(userId, NULL, '/reports/')`. | **PASS.** `Error: UNIQUE constraint failed: index 'idx_folders_userId_projectId_path'` — the exact driver message `db/sqlite_errors.rs` was written against — and the row count did not move. | PASS |
| A4 | CLAUDE | **`ensure_by_path` through the UI** | Created `/Dogfood/Nested/` from the Move-to-Project dialog in a project with **zero** folders, then created the **same path again**. | **PASS.** First create minted **two** rows — `/Dogfood/Nested/` *and* its ancestor `/Dogfood/`, with `parentFolderId` correctly wired (24 → 26). Second create returned the **same two ids** (`1a54e13b`, `ad9ec3cb`): total stayed **26**, nothing minted. Across the whole table, duplicate `(userId, COALESCE(projectId,''), path)` groups = **0**. The seven-site chokepoint cutover — invisible to every sequential differential — proven through the UI on a real instance. | PASS |

### Part B — the Concierge mark on every chat list (P4.D143 + P4.D144)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | **The mark on the lists, real data** | Home dashboard Recent Chats + the Salon list. ⚠ *plan corrected*: the mark draws for **all three** non-Monitored states, not just the uncensored pair — Monitored alone wears nothing. | **PASS, exact.** Salon panel: **73 `Concierge: Flagged`** (base class = `danger`), **10 `Concierge: Vouched Safe`** (`-muted`), **2 `Concierge: Uncensored`** (`-info`) over **799** cards — matching the DB-derived counts row for row, with all 803 Monitored bare. Real hover on an uncensored mark opened the Tooltip carrying the presentation table's `label` + `detail` + `hint` **byte-for-byte**. **All four §A payloads confirmed at the wire** to carry `conciergeState` + `dangerCategories`: `listChats` 799 rows `{monitored 714, flagged 73, uncensored 2, vouched 10}`, `projectChatList` 20, `characterChats` 10, and the home dashboard's Recent Chats by DOM. | PASS |
| B2 | CLAUDE | ⭐ **The hide delta, and the vouched-safe discriminator** | Clicked "Dangerous Chats" in the user-menu footer; counted `qt-chat-card` before/after. | **PASS, to the row.** 799 → **724** = exactly **−75** (73 flagged + 2 uncensored). Marks left standing: **only the 10 `Vouched Safe`** — zero Flagged, zero Uncensored. ⭐ All three `OFF`+`isDangerousChat=1` chats — *Holding Hands Over Cold Tea*, *The Blue-White Ultimatum*, *The Amber Singularity* — **still listed**. The pre-fix raw-label rule would have hidden them; this is `c43d3b1b4`'s whole point, proven on real data. | PASS |
| B3 | CLAUDE | **The footer gate (`hasQuickHideFeatures`) — the THIRD arm, isolated** | Opened the user menu on a fresh profile; then read `localStorage`; then hit the wire directly. | **PASS, and the third arm is isolated.** At first open `localStorage` held **no** hidden-tag key and **no** `hideDangerous` key, so arms 1 and 2 were both **false** — yet the quick-hide section rendered. Only `hasDangerousChats` could carry it. The wire agrees: dispatch `{"type":"chatsHasDangerous"}` → `{"hasDangerous":true}`, and the NEW REST leg `GET /api/v1/chats?action=has-dangerous` → `{"hasDangerous":true}` 200. Without P4.D144's ACTIVATE-AT-UNIFY flip this section would have been hidden — taking the only way to turn Dangerous Chats on with it. | PASS |
| B4 | CLAUDE | **The header pill + sidebar read the same table** | Opened the Uncensored chat *All-or-Nothing at the East Pool*; compared mark, header pill and the sidebar's Chat section. | **PASS.** Mark: `qt-concierge-mark qt-concierge-mark-info`, aria `Concierge: Uncensored`. Header pill: `qt-danger-badge qt-danger-badge-info`, text `Uncensored`, aria `Concierge: Uncensored`. Sidebar Chat section: the four state options plus the detail sentence *"You have sent the Concierge away and opened the uncensored door yourself. Nothing is scanned, nothing is softened — the risk is yours."* — **byte-identical** to the tooltip's. Three surfaces, one table, zero drift. | PASS |
| B5 | CLAUDE | **Click the mark itself** | Real click on the 4×12 px asterisk in the dashboard's Recent Chats row (not the card body). | **PASS.** The chat opened. v4's transcribed "still opens when the mark itself is clicked" case — the one the §3 review rescued from a comment with no test behind it — holds against a real pointer. | PASS |
| B6 | CLAUDE | ⭐ **The enqueue guard — an A/B on ONE chat** | *Chat with Vault Test Harness* (Kumar on DeepSeek V4 Flash, cheap). Turn 1 while **Monitored**; flip the sidebar's Concierge select to **Uncensored**; turn 2. | **PASS, and the discriminator is airtight.** Turn 1 (Monitored): `messageCount` 6 → 9, **1** `CHAT_DANGER_CLASSIFICATION` job enqueued. Flip persisted (`conciergeOverride='UNCENSORED'`, `isDangerousChat` **stays 0** — the label is preserved, and `dangerClassifiedAtMessageCount` **stays 6**). Turn 2 (Uncensored): `messageCount` 10 → 12, jobs **still 1** over 48 s of polling. With the count guard (6 ≠ 12) and the sticky guard (`isDangerousChat=0`) both *open*, `is_classifier_on_duty` is the ONLY thing that could have blocked it. Before this round v5 enqueued a doomed job on every such turn. | PASS |
| B6b | CLAUDE | **The sticky arm, found by accident** | First attempt used *Chat with Sunny Brevity* — which turned out to be **Flagged**. A turn there enqueued nothing. | **PASS (and my plan's premise was wrong, not the app).** v4's rule is `isDangerousChat === true` → **never re-check**; a flagged chat correctly enqueues nothing, so it could never have served as the "positive" contrast. Recorded because the mis-step is the useful part: the positive arm needs a chat that is *genuinely Monitored* **and** whose `dangerClassifiedAtMessageCount` differs from `messageCount` **and** which has a `contextSummary` — three conditions, and only the first is visible on screen. | PASS |
| B7 | CLAUDE | **The §3 review's `parseInt` fix, live** | `GET /api/v1/chats` with `?limit=1abc`, `?limit=`, `?action=bogus` on the new collection dispatcher. | **PASS.** `?limit=1abc` → **exactly 1** chat (v4's `parseInt` is a PREFIX parse; v5's original whole-string `parse::<i64>` would have returned all 799). `?limit=` → **799**, no limit. `?action=bogus` → **400** `Unknown action: bogus. Available actions: has-dangerous`. | PASS |

### Part C — absent characters out of story backgrounds (P4.D146)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | ⭐ **The presence gate on a real chat** | *The Weight of the Plumb Line* (Friday/Amy/Charlie **active**, **Jackie** and **Ariel** absent). Regenerate Background from the sidebar. | **PASS, and the side door is closed.** The crafted prompt has two sections and they behave exactly as the commit intends. `Characters to include as figures in the scene:` holds **exactly three** entries — Friday, Amy, Charlie. `Scene context:` legitimately says *"Jackie and Ariel have left"* and describes them walking out — that is the scene-state tracker's prose, which the fix does not touch. ⭐ Both absent names therefore **appear in the prompt text** — precisely the condition that would have handed the pre-fix back-fill their portraits — and **neither has an appearance line**. Bonus: the system message is **5,114** chars, the concealed story-background variant, matching P4.D94's recorded byte count exactly. | PASS |
| C2 | CLAUDE | ⭐ **Silent counts as present — and absent does not, in ONE chat** | *Warmth, Resentment, and the Return* — Friday **active**, Amy **active**, **Ariel silent**, **Charlie absent**. Triggered via `chatRegenerateBackground`. | **PASS, both arms at once.** Figure payload: Friday ✓, Amy ✓, **Ariel ✓ (silent = present)**, **Charlie ✗ (absent)** — and Charlie is named in the Scene context, so the back-fill pool would have caught him before the fix. One real chat, all four statuses' semantics. | PASS |
| C3 | CLAUDE | **Nobody present → no background** | Set **both** character participants of *The Tailcoat and the Pressure Gauge* to Absent through the sidebar, then triggered. | **PASS, byte-exact.** `{"kind":"bad-request","message":"No characters present in chat to generate background for."}` — matching `app/api/v1/chats/[id]/actions/story-background.ts:50` at the baseline, and `70505745a`'s own hunk shows this is the **reworded** sentence (`No characters in chat` → `No characters **present** in chat`). Participants restored to Active afterwards. | PASS |
| C4 | CLAUDE | ⭐ **A retired-mode project coerces, not crashes** | Planted `"backgroundDisplayMode": "project"` into *The Estate*'s real `properties.json` (server down), then read the project back. | **PASS — and the plant was visible.** `projectGet` returns the project (`name: "The Estate"`) with `backgroundDisplayMode` read back as **`'theme'`** — coerced, not thrown away by a failed parse, which is exactly the failure v4's commit says the preprocessor exists to prevent. The retired `"project"` is still **on disk afterwards**: a read coerces, it does not rewrite. ⚠ *Correcting a standing memory note:* overlay properties **can** be SQL-seeded — you must update `contentSha256` **and** `plainTextLength` on `doc_mount_documents` **and** `sha256` + `fileSizeBytes` on `doc_mount_files`; seed only `content` and the change is indeed silently ignored. | PASS |
| C5 | CLAUDE | **The narrowed picker + the update schema** | *The Estate*'s Story Backgrounds card, plus `projectUpdate` at the wire. | **PASS on both halves.** SPA: exactly two options — `theme` "Use theme background (no image)" and `latest_chat` "Latest chat background"; the retired pair is gone. Wire: `backgroundDisplayMode` of `project`, `static` **and** `bogus` each answer `{"kind":"bad-request","message":"Validation error"}` — v4's envelope — while `theme` succeeds. `modeLabels` renders a real label: the toast read **`Background set to latest chat background`**, not `Background set to undefined` (the §3 review's exhaustiveness fix). | PASS |

### Part D — the Move-to-Project folder picker (P4.D147)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | ⭐ **The picker over real folders** | Move to Project on `Amy.md`; cycled the destination through four projects. | **PASS, every list matches the DB.** *Quilltap Plans* → Root + **7**; *Church* → Root + **2**; *LUC Ranch* → Root + **1**; *Wardrobe Design* (no `folders` rows at all) → **Root only**. A real `<select>`, re-derived on every switch — v5's stand-in text field is gone. Both derivation sources are visible in one list: `/ (Root) (155 files)` and `/character-avatars/ (255 files)` carry counts from the file-path scan, while `/Feature Requests/`, `/Folio Drafts/`, `/Scenarios/` carry none — DB rows with no files. | PASS |
| D2 | CLAUDE | ⭐ **The implied folder appears** | *Plant:* deleted the `folders` row for *Church*'s `/character-avatars/` while leaving its **8 files** in place — a genuinely implied-only project folder, which real data did not otherwise offer. | **PASS decisively.** With no DB row at all, the picker still offers `└ character-avatars (8 files)`, derived purely from `files.folderPath`. The two derivation sources are separable in one list: options with a `(N files)` count come from the file scan, those without from `folders`. (The originally-planned subject, `/archives` in the General store, is unreachable here — Move-to-Project offers only projects as destinations.) | PASS |
| D3 | CLAUDE | ⭐ **Create a folder from the dialog — and the indent bytes** | Typed `/Dogfood/Nested/` into the inline create affordance on a zero-folder project. | **PASS, and richer than planned.** One create produced **both** `/Dogfood/Nested/` **and its derived ancestor** `/Dogfood/`; the picker refetched and **auto-selected** the new path. Indent bytes read straight out of the DOM: `/Dogfood/` → `[9492]` = `└`; `/Dogfood/Nested/` → **`[160, 160, 9492]`** = two **U+00A0** then `└`. Real data supplied the same shape independently: *Quilltap Plans*' `/Foundry-9/Quilltap/` renders `[160, 160, 9492, 32]` — the depth-2 indent on a folder nobody created for this walk. | PASS |
| D4 | CLAUDE | **Root moves send `'/'`** | Selected `/ (Root)` and pressed **Move to Project** on `Amy.md`. | **PASS.** Toast `"Amy.md" moved to Wardrobe Design`; the file left General Files; and `files` reads back `projectId = 23209429-…` with **`folderPath = "/"`**. | PASS |
| D5 | CLAUDE | **Offline/paused query** | Forced `navigator.onLine = false` + an `offline` event so TanStack **pauses** the query, then opened the dialog and selected *Malory Wave* — a project never fetched this session, so no cache. | **PASS.** The picker rendered **Root only** and the dialog said **no** `Loading...`. The instrument is proven by absence: Malory Wave really has 2 folder rows, and they are missing — the query never ran. `isPending` (v5's original) is true for a paused query and would have stuck on `Loading...` forever; `isLoading` (the §3 fix, matching v4) falls through to Root. | PASS |

### Part E — standing 💸 queue (the cheap end)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | **Pascal's group tier** (owed since 2026-08-26) | Surveyed the setup rather than forcing it. | **DEFERRED — with the recipe, which is the useful output.** The instance's only effects-bearing tool, `Tools/agent_lambda.tool.json`, targets `metadata.lastLambdaOutput` — the character-vault tier the 2026-08-26 pass already proved. Reaching the **group** tier needs two conditions that must hold *together*, and a naive attempt gets the first wrong: (1) `pascal/side_effects.rs` searches the cascade **chat → project → group** for a key that **already exists**, so a brand-new key lands in chat state and never reaches group state — the key must be **pre-seeded into group state** (`groupStateSet`) before the tool runs; and (2) the chat must satisfy the exactly-one rule, `groupTier.status == "single"`, which needs a chat whose characters resolve to exactly one of the instance's two groups (*Constellation*, 4 members; *Sebold Family*, 7). Both are buildable; neither is a browse. | DEFERRED-TO-HUMAN |
| E2 | HUMAN | **Re-measured compression row** (superseded C4) | Force an uncached compression on a large chat. | The **pre-computed** path is budgeted at 120 s and the inline one is not; note which of the two actually ran. Read `CONTEXT_COMPRESSION` `durationMs` in `llm_logs`. | DEFERRED-TO-HUMAN (needs a deliberately pressured context + real spend) |
| E3 | HUMAN | **Brahma deep query** | A deep Brahma Console query against the raised agent-turn budget. | Budget binds at the configured cap. | DEFERRED-TO-HUMAN (cost) |
| E4 | HUMAN | **Memory dedup / conversation-summary regeneration** | Run both from Settings. | First live run completes. | DEFERRED-TO-HUMAN (batch cost) |
| E5 | HUMAN | **NanoGPT caching cost question (#101)** | — | Whether writing a cache every turn and never reading one is costing money. | DEFERRED-TO-HUMAN (a cost judgment, not a defect) |
| E6 | — | **LoRA wire-byte look** | — | — | BLOCKED (`wire-tap.py` cannot tap HTTPS; `llm_logs.request` is a pre-builder projection) |
| E7 | — | **`[CheapLLM] Task failed` warn ordering** | — | Needs a cheap-LLM task to exceed its budget. | BLOCKED (never crossed in 400+ real calls; unit-proven instead) |

---

## Findings

**No v5 defects were found by this walk.** Twenty-two steps executed, all PASS.

Three things were corrected on the way, none of them app bugs:

1. **The walk plan's B1 expectation was too narrow.** The mark draws for **all
   three** non-Monitored states (`flagged` danger, `vouched` muted,
   `uncensored` info); only Monitored wears nothing. Corrected in place.
2. **The walk plan's B6 contrast was invalid.** An already-flagged chat is
   **sticky — never re-checked** (`isDangerousChat === true` bails), so it can
   never serve as the "a job IS enqueued" arm. Recorded as B6b, because the
   three conditions the positive arm actually needs are not visible on screen.
3. **A standing memory note is too strong.** "Store-overlay properties cannot
   be SQL-seeded" is only true of a naive write: seeding `content` alone is
   silently ignored, but updating `contentSha256` + `plainTextLength` on
   `doc_mount_documents` **and** `sha256` + `fileSizeBytes` on
   `doc_mount_files` makes the plant fully visible to the overlay (C4).

## Results

**22 PASS · 0 FAIL · 1 DEFERRED (with recipe) · 4 deferred-to-human · 2 blocked**

All six of the round's 💸 items are discharged, two of them by a stronger
proof than the one originally banked:

| 💸 item | outcome |
|---|---|
| the real 607-row collapse | **EXPIRED, replaced by two better proofs** — v4 healed it first (583 → 24 at 11:55Z). v5 then (a) booted on v4's healed DB writing **nothing**, and (b) collapsed a **planted** 4-duplicate set with a repoint, still writing no ledger row |
| the marks + hide deltas + footer probe | **PASS to the row** — 73/10/2 marks matching the DB; −75 on hide with the three vouched-safe-but-labelled chats surviving; the footer's third arm isolated |
| the enqueue guard on a real Uncensored chat | **PASS** — a same-chat A/B with both other guards held open |
| the absent-participant gate | **PASS** — payload filtered, scene context intact, back-fill side door closed; silent counts as present; nobody-present refuses byte-exact |
| a real retired-mode project | **PASS on a planted subject** (real data had none) — coerces to `theme`, still loads, disk value untouched |
| the picker over real folders | **PASS** — four projects' lists match the DB exactly, both derivation sources separable, indent bytes verified |

Free proofs picked up along the way, not planned:

- **A cross-implementation agreement on bug 114.** v4's independent migration
  produced `583 duplicates → 24 folders` — the identical shape
  `folders_collapse_heal_equivalence`'s Friday scenario asserts.
- **The concealed story-background variant at 5,114 chars**, matching P4.D94's
  recorded byte count exactly.
- **The §3 review's `parseInt` prefix-parse fix**, proven live (`?limit=1abc`
  → exactly 1 of 799).
- **The §3 review's `isLoading` fix**, proven against a genuinely paused query.
- **The §3 review's `modeLabels` exhaustiveness fix**, proven by the toast.

---

## State of the copy at walk end

The dogfood copy is deliberately dirty — the next rsync restores it. Recorded
so nothing here is mistaken for real Friday data:

- **Planted and healed:** 4 duplicate `folders` rows (collapsed by v5 on
  restart) + the child `/reports/sub/` (kept, repointed at the survivor).
- **Planted, still present:** `The Estate`'s `properties.json` carries the
  retired `"backgroundDisplayMode": "project"`; *Church*'s
  `/character-avatars/` **folder row is deleted** (its 8 files remain).
- **Created:** `/Dogfood/` + `/Dogfood/Nested/` in *Wardrobe Design*.
- **Moved:** `Amy.md` from General Files to *Wardrobe Design* Root.
- **Turns sent (real LLM spend, all cheap):** two in *Chat with Vault Test
  Harness* (DeepSeek V4 Flash), one in *Chat with Sunny Brevity* (Haiku 4.5).
- **Backgrounds regenerated:** *The Weight of the Plumb Line* and *Warmth,
  Resentment, and the Return* (one Grok image each).
- **Restored after use:** every participant status flipped for C3, the
  Concierge state on the B6 chat, and the Hide Dangerous Chats toggle.
