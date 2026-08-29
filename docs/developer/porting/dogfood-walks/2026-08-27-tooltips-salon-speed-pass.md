# Dogfood walk — the P4.D131 round + the accumulated 💸 backlog (2026-08-27)

**Instance:** `~/qt-dogfood-friday` — a COPY of real Friday data, rsynced
2026-08-27 23:12 (main 819 MB / mount-index 737 MB / llm-logs 317 MB).
Disposable: never rsynced back. Server `target/release/quilltap-web` +
`apps/web/dist/quilltap/browser`, `127.0.0.1:3000`, log at
`scratchpad/server.log`.

**Rounds under test.** The 2026-08-26 pass closed the `b220999d` round; four
rounds have landed since without a walk, so this pass covers the newest round
in full and then works the backlog in priority order:

- **P4.D131 ∥ P4.D132 ∥ P4.D133 ∥ P4.65** (2026-08-27, baseline →
  `b121ac77f`) — the Tooltip primitive + nine-button adoption, the net-NEW
  ConfirmationBadge, `instances restore-key`, the Salon `ChatListPreloaded`
  batching.
- **P4.D130 ∥ P4.62 ∥ P4.63 ∥ P4.64** (2026-08-27) — the outfit pull-down +
  garments-only slot pickers; the `systemHome` N+1 fix.
- **The 4.9.0-push round** (P4.D126–D129) — bug-103 legacy seeding, the
  glm-5.3 vision rows, the 75 s compression budget + warn line, the About
  strings, three-shell completion, the two hover fills.
- **The `f3892158d` realtime round** (P4.D123–D125) — the queue chips + the
  `startedByKind` pulse, pushed invalidation with polling parked, the
  terminal same-origin refusal, the "Fallback polling (5s)" relabel.

## ⚠ Drift note (read before calling anything a defect)

The drift ledger was STALE at walk start; `/driftcheck` ran first and
recorded **2 commits past the baseline** (ledger `11edb1c6`):

- **`1560bd43b` — PORT, UNPROCESSED.** v4 retires Lima/WSL2. It **deletes
  the `isVM` key** from `/api/v1/system/data-dir`, drops `'lima'` from the
  Almanack `runtimeType` union, and drops the `VM (Lima/WSL2)` /
  `Electron + VM` labels from `self_inventory`. **v5 still emits the v4
  shape at the baseline, which is correct.** If the Profile screen, the
  Almanack's Runtime Type line, or a `self_inventory` runtime label looks
  "wrong" against v4's working tree, that is the pending drift row — NOT a
  v5 defect. Diagnose against `git show b121ac77f:<path>`.
- `7819afb1d` — NO-PORT? (CI/test plumbing only).

**Regen rule in force: PIN REQUIRED** — any oracle regen this walk needs runs
from a worktree pinned at `b121ac77f`.

## What NOT to expect to work (refusal-armed / known parks)

- **The component-transfer store probe** — the one standing Playwright skip;
  the committed e2e fixture lacks a General store. Not a product defect.
- **Web search via `SERPER_API_KEY`** — finding #98 is CLOSED; the key now
  comes from the configured `api_keys` row. No env var should be needed.
- **A subsystem background** (the `'theme'` fallback) — a standing recorded
  divergence, not a bug.
- **`isVM` removal / `lima` runtime labels** — see the drift note above.

---

## Part A — the P4.D131 round (this round's surfaces)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | **Tooltip dwell on the action bar** | Hover the Copy button on a real assistant message; wait past the 200 ms dwell. Then move the pointer away and confirm the 120 ms grace closes it. | A `qt-tooltip` bubble appears with v4's copy; **no `title=` attribute anywhere in the row** (the adoption deleted all nine). Verified by `read_page` + a DOM query for `[title]` inside the action bar. | **PASS** |
| A2 | CLAUDE | **All nine buttons carry a bubble** | Enumerate the action-bar buttons on a real message in both roles (user + assistant) and check each has a tooltip anchor and an explicit `aria-label`. | Nine anchors, zero `title`s, aria-labels present. The re-attribute button reads v4's NEW copy ("…to a different participant" — v5 used to carry the old wording). DOM query. | **PASS** |
| A3 | CLAUDE | **Tooltip flip + clamp at a viewport edge** | Hover a button on the FIRST message (top of the scroll) and on a message at the very bottom; also narrow the window. | The bubble flips above/below and clamps horizontally rather than overflowing. Screenshot + bubble bounding box vs viewport. | **PASS (partial — flip unreachable, stated)** |
| A4 | CLAUDE | **The reparented-bubble leak** (the lane's measured trap) | Hover several different buttons in sequence, then count `body > .qt-tooltip*` nodes. | Bubbles do not accumulate on `document.body` — the component removes the moved node by hand. `document.querySelectorAll` count returns to 0 after the last close. | **PASS** |
| A5 | CLAUDE | **The ConfirmationBadge (net-NEW to v5)** | Find a real message with an answer-confirmation record on the Friday copy (query first). Click the badge to pin it. | The badge renders as a real `type="button"` with the right state; clicking pins (`data-pinned`), the bubble survives the pointer leaving, Escape dismisses. If no real confirmation rows exist, MEASURE that and say so. | **PASS** |
| A6 | CLAUDE | **Salon chat-list speed at real scale** (P4.65's whole point) | Time the Salon list's dispatch on the real instance — `enrich_chats_for_list` used to cost 8.6–12.2 s. | The batched path lands ~5.7× faster. Measured from the server log's per-request timing and a wall-clock `curl` of the dispatch verb; compare with the round record's 2,227/1,451 ms. | **PASS** |
| A7 | CLAUDE | **The `1b0ce9eba` deletions** | Grep the served CSS bundle for the three deleted `display:none !important` rules. | Absent; `.qt-chat-message-action-bar { display:flex }` still present. `curl` the chunk + grep. | **PASS** |
| A8 | CLAUDE | **The `try_decrypt` IV-length panic guard** | Build a scratch instance, hand-craft a `.dbkey` whose IV is valid hex of the WRONG length, run `quilltap instances restore-key` against it with a throwaway pepper. | No panic; the CLI reports failure gracefully (v4's Node GCM accepts any IV length and fails the auth check). Needs no real pepper. Server log / CLI exit code. | **PASS** |
| A9 | **HUMAN** | **`instances restore-key` with the REAL pepper** | Delete/rename the `.dbkey` on the copy, then rebuild it from `ENCRYPTION_MASTER_PEPPER`. | The rebuilt `.dbkey` opens the copy's databases. **Human-run** (Claude never handles the pepper). | **PASS** |

## Part B — the P4.D130 round

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | **The outfit pull-down** | Open a real character's wardrobe, use the outfit quick-pick pull-down; dismiss with Escape (capture-phase). | The pull-down opens, lists composed outfits from the real pool, and Escape closes it without closing the parent. Screenshot + DOM. | **PASS** |
| B2 | CLAUDE | **Garments-only slot pickers** | Open the per-slot pickers on a real character. | Slot pickers offer garments only (accessories/hair excluded per slot semantics); the chip row still gets `allItems` whole. Screenshot. | **PASS** |
| B3 | CLAUDE | **The `systemHome` fix at real scale** (P4.64) | Load the home dashboard cold and time it. | ~0.39 s, not the old 8.8 s. Server-log timing. | **PASS** |

## Part C — the 4.9.0-push round 💸

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | **The two hover fills** | Hover the two surfaces the census found bare. | A visible hover state (the utilities are solid, not the inert unwritten form). Screenshot before/after. | **PASS** |
| C2 | CLAUDE | **The About strings** | Open About. | The provider sentence + the Live-interface bullet read v4's post-`dcab791c2` copy. ⚠ **Drift:** v4 has since rewritten the VM/Lima prose (`1560bd43b`) — v5 correctly still shows the baseline text. | **PASS** |
| C3 | CLAUDE | **Live three-shell completion** | Run `quilltap completion bash|zsh|fish` from the built CLI and exercise one real `<TAB>`. | All three templates emit; a real completion fires. Shell output. | **PASS** |
| C4 | CLAUDE | **The 75 s compression budget + `[CheapLLM] Task failed` warn** | Find or force a compression run. | The local-first budget applies; the warn line appears in `combined.log` when it fails. Log grep. | **PASS (partial — stated)** |
| C5 | CLAUDE | **The glm-5.3 vision rows** (bug 104) | Fetch Z.AI image/vision models on a real key. | `glm-5.3-flash` rows present, vision models not dropped. Network response. | **PASS (human-run)** |
| C6 | **HUMAN** | **bug-103 seeding on a real pre-4.9 archive** | Import a genuine pre-4.9 backup archive. | Legacy columns seeded. **Deferred:** needs a real archive file the human has; also a heavy write. | DEFERRED-TO-HUMAN |

## Part D — the `f3892158d` realtime round 💸

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | **The queue chips over a real generation** | Watch the toolbar chips (`Main`/`Emb`/`Sum`/`Dgr`/`Img`) while a job runs. | Counts move with the real queue; the `startedByKind` pulse fires. Screenshot sequence + the jobs verb. | **PASS** |
| D2 | CLAUDE | **Pushed invalidation with polling parked** | Watch `/api/events` while mutating something, and confirm the fallback poll is NOT running. | The hint arrives on the SSE channel; no 1.5 s/8 s poll while the channel is healthy. Network tab. | **PASS** |
| D3 | CLAUDE | **The terminal same-origin refusal** | Open the terminal WS with a foreign `Origin` header. | Post-upgrade 1008 close, v4's framing. `curl`/WS probe. | **PASS** |
| D4 | CLAUDE | **The "Fallback polling (5s)" relabel** | Find the toggle. | Reads v4's exact relabel. Screenshot. | **PASS** |

## Part E — the long-standing backlog

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | **Pascal's group-tier write path** (the last of the four) | Run a Pascal custom tool with an `effects` write scoped to a GROUP, from a single-group chat. | The write lands in the group's store with siblings intact. DB read-back. | **DEFERRED-TO-HUMAN** |
| E2 | CLAUDE | **The Brahma deep-query budget** | Ask Brahma something that runs past the old 25-turn cap. | The raised budget (default 50) binds; the salvage path is not hit. `llm_logs` + the console. Cost-bounded: one query. | **DEFERRED-TO-HUMAN** |
| E3 | **HUMAN** | **Memory dedup / conversation-summaries first run** | Run both maintenance actions. | **Deferred by cost** — batch LLM work over 800 MB of real data. | DEFERRED-TO-HUMAN |
| E4 | **HUMAN** | **NanoGPT caching cost question (#101)** | Judge whether the gateway ever reads a cache. | **Deferred:** a cost judgment, not a correctness one. | DEFERRED-TO-HUMAN |

---

## Results log

### A6 + B3 — the two speed proofs (PASS, 2026-08-27)

Both measured by wall-clock `curl` against the running server on the real
copy, three runs each, warm:

- **A6 — `listChats` at real scale: 1.34 / 1.34 / 1.35 s**, HTTP 200,
  **4,148,602 bytes**, **779 chats** enriched. P4.64 measured the
  pre-batching Salon list at **8.6–12.2 s**; the round record's post-fix
  enrich legs were 2,227 / 1,451 ms. The batched `ChatListPreloaded` path
  is live and holding at roughly **7×** on data one rsync fresher than the
  round's own measurement (which was md5-identical at 4,104,806 bytes —
  the extra 43,796 bytes are chats written since).
- **B3 — `systemHome`: 0.32 / 0.30 / 0.31 s**, 40,211 bytes. P4.64
  measured the dropped-preload defect at **8.8 s** and the fix at 0.39 s;
  the front door is now sub-third-of-a-second on the real instance.

Both are live proofs of P4.64's diagnosis (the N+1 was a dropped-preload
PORT defect, not the hypothesised cause) and of P4.65's fix.


### A1–A4 — the Tooltip primitive on real messages (PASS, 2026-08-27)

Chat: "The Inches We Keep Making" (175 messages, real Friday data).

- **A1 — the 200 ms dwell.** Hovering Copy: **0** portalled bubbles at
  100 ms, **"Copy message"** rendered at 700 ms. The bubble is a direct
  child of `document.body` (v4's `createPortal`) carrying `role="tooltip"`,
  `aria-hidden="true"`, `data-placement`, and an inline
  `top`/`left`/`visibility` style.
- **A2 — the nine buttons.** Across five action bars: assistant rows carry
  5 buttons / 5 tooltip anchors, user rows 4 / 4 — the nine distinct kinds.
  **Every button has a tooltip anchor and an explicit `aria-label`**, and
  the re-attribute button reads v4's NEW copy, `Re-attribute to a different
  participant` (v5 used to carry the old wording without the "a").
  Twenty-four anchors on the page, **all of them message action bars**.
  - ⚠ **Eight `title=` attributes survive inside the action bar**
    (`Prompt tokens` ×4, `Completion tokens` ×4). **Measured against v4 at
    the baseline: these are v4's own.** `0bd841394` converted
    `MessageActionBar`, not `TokenBadge` — `git show
    b121ac77f:components/chat/TokenBadge.tsx` still has
    `<span title="Prompt tokens">`, `title="Completion tokens"`, and
    `title="Estimated cost"`. v5's `token-badge.ts` carries all three
    verbatim; the cost span never renders because v4's `MessageActionBar`
    passes `showCost` without `estimatedCostUSD` (documented in the v5
    component). **Faithful, not a leftover.**
- **A3 — geometry (PARTIAL, honestly).** Clamp holds: a measured bubble came
  back `withinViewport: true` at 1280×720, placement `top`. **The flip
  branch is not reachable by gesture in this layout** — the only tooltip
  anchors are message action bars, and the message list's top edge sits
  ~100 px below the viewport top, so the "no room above" case cannot be
  produced. Recorded rather than faked; the lane pins it by unit spec (the
  flip-inversion mutation reddens). Narrowing to 380 px breaks the chat
  layout itself — a non-target breakpoint, not a tooltip defect.
- **A4 — the reparent leak (the lane's measured trap).** Hovering five
  different buttons in sequence left **exactly one** node on
  `document.body`, never five. After two full open→close cycles the count
  is **0** and `body.children` went 14 → 13. No accumulation.
- **Riding along:** the Delete button renders with the round's review-fix
  danger chrome — the trash glyph is visibly **red**
  (`qt-chat-message-action-icon-danger`, the rule that used to sit inert).

### A5 — the ConfirmationBadge, net-NEW to v5 (PASS, 2026-08-27)

**Population measured first** (ledger §5.5) over 128,401 real messages —
5,736 carry a confirmation record:

| state | rows | with detail |
|---|---|---|
| `vouched` | 5,544 | **0** |
| `amended` | 164 | **164** |
| `stood-by` | 28 | **28** |

That distribution is itself the pin-gate's justification: `hasDetail` is
`notes \|\| original`, and **no vouched row has either**, so pinning is
correctly unavailable on 5,544 of 5,736 badges and available on exactly the
192 that carry something to read.

- Badges render on real data as a real `<button type="button">` inside a
  `qt-tooltip` anchor, with `data-confirmation-state` and a full
  `aria-label` ("Answer confirmation: Vouched. Checked against what the
  character recalled…"). Seen live: `vouched`, `unvetted`, `amended`.
- On an **amended** row in "The Vials That Rewrote Memory"
  (`data-has-detail="true"`), hovering produced the structured bubble —
  title **Amended**, the summary sentence, then the **WHAT LOOKED OFF**
  section rendering the real `confirmationNotes` (a scrolling three-point
  critique of the reply). This is the field family P4.D132 found the mapper
  was dropping, read back off real rows.
- **Click pins:** the bubble takes `data-pinned="true"` +
  `data-interactive="true"` and **survives the pointer leaving** (measured
  1.2 s after moving away).
- **Escape dismisses** (0 bubbles on `body`) and returns the focus ring to
  the badge.
- ⚠ **Walk trap worth keeping:** `data-pinned` is on the **bubble**, not the
  badge — a first query against the badge returned `null` and looked exactly
  like a defect. (Also: the Browser pane's screenshot frame was 800×450 while
  the CSS viewport was 1280×720; a hover at raw screenshot coordinates lands
  at 1.6× the intended point and silently misses.)

### A7 — the `1b0ce9eba` deletions (PASS, 2026-08-27)

v4's commit deleted exactly three `display: none !important` rules from
`app/styles/qt-components/_chat.css`. All three are absent from v5's
**served** bundle (`styles-TJZXFAXA.css`) *and* from the SPA source:

| v4-deleted rule | in served bundle | in `apps/web/src` |
|---|---|---|
| `.qt-chat-desktop-hover-actions` | 0 | 0 |
| `.qt-chat-message-desktop-actions` | 0 | 0 |
| `.qt-chat-desktop-timestamp` | 0 | 0 |

The rule that had to survive did:
`.qt-chat-message-action-bar{…display:flex;…}` is present (twice, the base
rule + the border-colour override). `qt-chat-message-action-icon-danger` is
present — the review-fix rule that used to sit inert. The whole
`qt-tooltip-*` family shipped: `qt-tooltip`, `-anchor`, `-body`, `-hint`,
`-quote`, `-section`, `-section-label`, `-title`.

### A8 — the `try_decrypt` IV-length panic guard (PASS, 2026-08-27)

The §3 review's third finding, exercised **end to end through the real CLI**
rather than only at unit level — and with **no real pepper**, so it needed
nothing deferred.

Two scratch instances, each given a `.dbkey` built from the real file's
*shape* with every secret-bearing field scrubbed (only `version`,
`algorithm`, `kdf`, `kdfIterations`, `kdfDigest`, `minServerVersion` copied
verbatim — verified by diff; `salt`/`ciphertext`/`authTag`/`pepperHash` all
replaced with junk). A freshly generated random pepper drove both runs.

| instance | `iv` | bytes | result |
|---|---|---|---|
| `qt-ivtest` | `aabbcc` | **3 — wrong length** | no panic |
| `qt-ivtest2` (control) | `aa`×16 | 16 — correct length, junk | no panic |

**Both produce byte-identical behavior**: the same
`WARNING: the existing .dbkey holds a DIFFERENT pepper than the one given.`
and the same successful rewrite. That is the point of the guard — v4's Node
GCM accepts any IV length and simply fails the auth check, so the
wrong-length file must land on the ordinary "doesn't decrypt" path, not on a
`Nonce::from_slice` panic. `RUST_BACKTRACE=1` was set for both runs; no
panic frame appeared. `try_decrypt_pepper`'s "None instead of ANY error"
contract holds on hand-edited files, which is exactly what `restore-key` is
aimed at.

Also observed in passing (v4-faithful `restore-key` chrome): the three
per-database `absent` lines, the `--force` gate's
`No encrypted database exists here, so the pepper cannot be proved.`, the
timestamped `.bak-…Z` of the previous file, mode 0600 on the new one, and
the closing note that character ARCHIVE bundles are passphrase-encrypted and
this offline path cannot re-encrypt them (the P4.D63 unit-7 sweep's job).

## Part D — the `f3892158d` realtime round

### D1 — the queue chips + the `startedByKind` pulse (PASS, 2026-08-27)

**The §A wire contract holds exactly.** `GET /api/v1/system/jobs` on the real
instance answers four keys in v4's insertion order — `stats`, `activeByKind`,
`startedByKind`, `processor` — with both kind objects carrying **exactly the
five keys** (`memory`, `embedding`, `summary`, `danger`, `image`) at integer
values, and **`activeByType` absent** (correctly opt-in since `664cfca84`).

**The chips move on real work.** Four `chatRenderConversation` calls on real
chats, polled at 150 ms:

```
active={}                                        pending=0  processing=0
active={'summary': 3}                            pending=2  processing=1
active={'summary': 3, 'embedding': 1}            pending=3  processing=1
active={'summary': 3, 'embedding': 10}           pending=12 processing=1
active={'summary': 2, 'embedding': 21}           pending=22 processing=1
```

The kind map discriminates correctly on real data: `CONVERSATION_RENDER` →
**summary**, and the chunk work those renders cascaded into →
**embedding**, while `memory` / `danger` / `image` stayed 0 throughout
because no such work existed. Boot had shown the same chips at 6/8/8/6/0
draining to zero.

**The pulse fires — and the case where it doesn't is v4's own design.**
`startedByKind` stayed `{}` for the entire background run, including
EMBEDDING_GENERATE jobs measured at **398 ms, 374 ms and 503 ms** — all well
past the 250 ms `BLIP_THRESHOLD_MS`. That looked like a defect and was
chased down; **it is not.**

- A **request-scoped** embedding does bump it. A real `memorySearch` on a
  Friday character (473 ms) was caught mid-flight and after:
  `active {'embedding': 1} started {}` → `active {} started {'embedding': 1}`.
  The pulse works.
- A **background job** deliberately does not. The job pump wraps every
  handler in `run_attributed_to_job(activity_kind_for_job_type(&job.job_type), …)`
  (`job_runner.rs:428`), which adds the kind to the attribution mask
  **without** beginning a span — because the job's own PENDING/PROCESSING
  row is already the count. The handler's inner
  `track_activity(Embedding, …)` then hits
  `current_attribution() & bit(kind) != 0` and returns transparently, so no
  span opens and nothing bumps `started`.
- **v4 is identical**, measured not assumed:
  `lib/background-jobs/child/child-entry.ts:173` reads
  `await runAttributedToJob(activityKindForJobType(job.type), () => handler(job))`,
  and v4's own `generateEmbeddingForUser` carries the same "re-entrant by
  kind, so an embedding job is not counted twice" comment v5 transcribed.

So `active` is "queue rows + inline work" and `started` is the inline-work
pulse alone, on both sides. No finding.

### D3 — the terminal WebSocket same-origin refusal (PASS, 2026-08-27)

Driven against a **real PTY session** (`POST /api/v1/terminals` on a real
chat → a live `/bin/zsh`), with a hand-rolled socket probe that completes the
handshake and then reads post-upgrade frames.

⚠ **The first probe used a bogus session id and every arm came back
`1000 Session not found`** — which looked like the gate was missing. It is
not: the gate deliberately fires **after** the session-exists check, "exactly
where v4's does" (`terminal_routes.rs`). Against a real id, all eight arms
are exactly right:

| `Origin` sent | outcome |
|---|---|
| `http://evil.example.com` | **1008 `Unauthorized`** |
| `http://127.0.0.1:9999` (same host, other port) | **1008 `Unauthorized`** |
| `:::not a url:::` | **1008 `Unauthorized`** |
| `https://127.0.0.1:3000` (other scheme) | allowed — streams |
| `""` (empty) | allowed — streams |
| *(header absent)* | allowed — streams |
| `null` (opaque) | allowed — streams |
| `http://127.0.0.1:3000` | allowed — streams |

Every "allowed" row actually **streamed live PTY output** (real zsh prompt
escape bytes), so the allow arms are proven by consequence, not by absence of
a close frame. The refusals reproduce v4's observable shape precisely: the
HTTP upgrade **completes** (101) and the socket then closes 1008, rather than
the upgrade being refused — which is what a browser client sees from v4.

Note the two arms that discriminate the ported rule: a **different port is a
different origin** (refused — the check compares HOST, host+port), while a
**different scheme is not** (allowed — scheme is not part of the comparison).
Both match `check_origin`'s documented arms.

The warn line reaches both the console and P4.49's `combined.log`, in v4's
JSON shape with the full reason sentence:

```json
{"timestamp":"2026-08-28T04:40:36.760Z","level":"warn",
 "message":"Rejecting WebSocket upgrade",
 "context":{"module":"quilltap::terminal","session_id":"ce30a1fd-…",
            "reason":"unparseable Origin header (:::not a url:::)"}}
```

Session deleted afterwards (`DELETE … → 200`).

### D2 — pushed invalidation with the fallback poll parked (PASS, 2026-08-27)

Measured with a `fetch` counter installed in the page, **proven alive before
any negative was trusted** (the standing dogfood-instrument rule — the first
liveness check called the *original* `fetch` and so bypassed the wrapper,
reading a false `0`; re-done through `window.fetch` it read 1).

**Idle, channel healthy — 17.4 s, timer ticks 17:**

```
jobsFetches: 1   ← my own liveness call, nothing else
eventsFetches: 0    otherFetches: 0
```

Zero app-initiated `/api/v1/system/jobs` requests. The fallback poll is
parked, and the Tasks Queue switch reflects it (off).

**The discriminator — a mutation the browser could not have known about.**
Counters reset, then three `chatRenderConversation` calls fired **from curl**,
outside the browser entirely. 12.7 s later, without a single user gesture:

```
jobsFetchesSinceMark: 13
chips: Mem0 Emb0 Sum0 Dgr0 Img0  →  Mem0 Emb23 Sum2 Dgr0 Img0
```

The SPA cannot have learned this from a timer (it made **zero** requests over
the preceding 17 s of idle) and cannot have learned it from its own action
(it took none). It learned it from the push, then refetched — and the 13
requests across 12.7 s are the adaptive ~1.5 s in-flight cadence, which is
supposed to run *while work is active* and stand down when it is not. That
is the whole D-round design demonstrated end to end on real data.

### D4 — the "Fallback polling (5s)" relabel (PASS, 2026-08-27)

Settings → Data & System → Tasks Queue renders **`Fallback polling (5s)`**,
and `Auto-refresh` appears nowhere in the page text. Byte-identical to v4's
`components/tools/tasks-queue/TaskFilters.tsx:125` at the baseline.

## Part B — the P4.D130 round

### B1 — the outfit pull-down (PASS, 2026-08-27)

The Wardrobe screen for **Abigail** (a real Friday character), Outfit Builder
panel. `Wear an outfit…` opens a searchable pull-down ("Search outfits…")
listing her real composed outfits with the slots each fills:

```
Incarnate Everyday — Abby's Second Sente…   Top, Bottom, Footwear, Accessories · replaces
Naked Marguerite                            Top, Bottom, Footwear, Accessories · replaces
```

**Escape closes only the pull-down.** After the keypress the pull-down's
search box is gone while the Wardrobe screen (`Outfit Builder` still
rendered) and its workspace tab both survive — which is the point of the
capture-phase handler: it must not bubble up and tear down the surface
hosting it.

### B2 — garments-only slot pickers (PASS, 2026-08-27)

Five equipped-slot rows render — **Top, Bottom, Footwear, Accessories,
Hair** — Hair being P4.D87's fifth slot, here `Empty`. Each row's `+` opens a
per-slot picker. Measured over the real wardrobe, **zero offenders in every
one**:

| picker | items offered | every item carries the slot |
|---|---|---|
| Top | 13 | ✅ |
| Accessories | 9 | ✅ |
| Hair | 1 (`Long emerald braid`) | ✅ |

The discriminator: **`Wayfarer's Fieldwork Ensemble [top\|bottom\|footwear\|accessories]`
appears in BOTH the Top picker and the Accessories picker**, while
accessory-only items (`Apple Watch`, `Ansible Forge Mark Four`) appear in
Accessories and are absent from Top. So the filter is *membership in this
slot*, not a blanket "garments only, no accessories" exclusion — which is
exactly the `allItems`-passed-whole shape the lane pinned RED first.

## Part C — the 4.9.0-push round 💸

### C3 — live three-shell completion (PASS, 2026-08-27)

**All three templates are byte-identical to v4's REAL launcher** run from a
worktree pinned at the baseline (`/tmp/qt-v4-pin-dogfood-b121ac77f`, pin
verified by the presence of `packages/quilltap/lib/dbkey-restore.js`):

| shell | lines | `cmp` vs v4 |
|---|---|---|
| bash | 362 | **byte-identical** |
| zsh | 717 | **byte-identical** |
| fish | 319 | **byte-identical** |

And a **real TAB actually fires** — the bash template sourced into a live
shell, its registered `_quilltap_complete` driven with real `COMP_WORDS`:

```
$ quilltap instances <TAB>
  list ls show path where add create remove rm delete set-passphrase
  passphrase default rename restore-key rebuild-key

$ quilltap instances restore-key --<TAB>
  --names-only --json --clear --passphrase --no-passphrase --data-dir
  --force --yes --help

$ quilltap <TAB>
  db docs themes instances memories memory-diff recall-replay logs
  migrations maintenance file-verify completion
```

P4.D133's new verb and all four of its flags are offered. (fish carries them
in its own `-l 'no-passphrase'` / `-l 'force'` / `-s 'y' -l 'yes'` form,
which is why a naive `--force` grep misses it there.)

### C2 — the About strings (PASS, 2026-08-27)

Rendered live from the user menu → About:

- Provider sentence: `Anthropic, OpenAI, Google Gemini, Grok, DeepSeek,
  Z.AI, NanoGPT, Ollama, OpenRouter, and OpenAI-compatible APIs` — matches
  v4 `b121ac77f:app/about/AboutView.tsx:217`.
- The **Live interface** bullet renders in full, and **after** `LLM tools`
  as the spec pins the order — matching v4's `:225` word for word.

**Drift confirmation (not a defect).** The same page still reads "powered by
a lightweight Linux VM behind the scenes", "Native desktop app – macOS
(Lima/VZ) and Windows (WSL2)…", and "Desktop & Infrastructure: Electron,
Lima, Docker". That is v4's text **at the baseline**, rendered correctly.
v4's pending `1560bd43b` rewrites all three; until the catch-up round lands,
this is exactly what v5 should show — and it usefully confirms the drift
row's SPA scope from the live surface.

### C1 — the two solid hover fills (PASS, 2026-08-27)

Friday's Photo Gallery on the real instance — **60 photos**, each tile
carrying both hover buttons (60 `Set as avatar`, 60 `Download image`). Both
fills were inert on *both* sides until the 4.9.0-push round; measured here by
computed style on the element actually under the pointer
(`document.querySelectorAll(':hover')`, not `querySelector`, which returns
the first of sixty):

| button | resting | hovered | token |
|---|---|---|---|
| `Set as avatar` | `rgb(26,28,35)` | **`rgb(50,174,116)`** + white text | `--color-success` = `hsl(152 55% 44%)`, fg `hsl(0 0% 100%)` |
| `Download image` | `rgb(26,28,35)` | **`rgb(129,151,218)`** + `rgb(19,21,27)` text | `--color-primary` = `hsl(225 55% 68%)` |

Both confirmed visually in the screenshot as well (green and blue pills on
the first tile). Theme in use: `madmans-box`.

⚠ **Measurement trap worth keeping:** the first pass hovered 2 px off
(`x=78` vs `x=80`), landed on the button's padding edge, and read the
resting colour — reporting `filled: false` for `Set as avatar` while
`Download` passed. That looked exactly like the known
"unwritten `hover:` variant is inert" bug, and the bundle was searched for a
missing rule before the coordinate was rechecked. Both rules and both tokens
were present all along. **Re-hover before believing a negative from a
computed-style read.**

## Extra coverage — branches the plan did not list (all PASS, 2026-08-27)

Added mid-walk once Parts A/B/D came back clean, on the principle that the
value here is the gesture the e2e never makes.

### The keyboard path — `focusin` opens with NO dwell

v4 opens a tooltip immediately on focus (React's delegated `onFocus`), which
is a **different branch** from the 200 ms hover dwell and was untested.
Measured with a `MutationObserver` on `document.body` timestamping the
bubble's insertion:

| trigger | bubble appears |
|---|---|
| `focus()` → `focusin` | **13 ms** |
| real pointer hover (earlier, `computer`) | absent at 100 ms, present at 700 ms |

The two branches are cleanly distinguished: focus is immediate, hover waits.
(An earlier attempt timed this with `await sleep(60)` and got `elapsed: 1103`
— the tool bridge overshoots sleeps badly, so a wall-clock read across the
bridge cannot prove a sub-200 ms claim. The observer timestamp can.)

### `focusout` closes an unpinned bubble

Controlled sequence from a cleared board: `focus()` → **1** bubble,
`blur()` → **0**. Correct per `onAnchorFocusOut()` (`if (!pinned) closeNow()`).

⚠ **A false alarm worth recording.** Mid-walk a "Copy message" bubble
appeared stranded — visible 2 s after blur, with focus moved elsewhere. It
looked like a real leak. It was **my instrument**: the timing test had
dispatched a synthetic `pointerenter` that never got a matching
`pointerleave` at the same listener, so the component's hover state stayed
set and held the bubble open past the focus close. The controlled re-test
above (real focus, real blur, no synthetic pointer events) is clean. Synthetic
pointer events in this component are unreliable in both directions — they
failed to OPEN a tooltip in the hover measurement and failed to CLOSE one
here.

### Outside-pointerdown dismisses a pinned bubble

Distinct from the Escape path already covered in A5. Pin the Amended badge →
1 bubble with `data-pinned="true"`; click a neutral spot → **0 bubbles**.

### The badge's fourth state — `stood-by`

Three of the four states were seen rendered on real data (`vouched`,
`unvetted`, `amended`). **`stood-by` was proven on the wire but not on
screen:** `chatGet` for "The Chord That Found Its Basement" returns 641
messages carrying **6** rows with `confirmed: false`, `confirmationRevised:
false`, `confirmationNotes` present and `confirmationOriginalContent` absent
— which is exactly the `stood-by` + `hasDetail: true` mapping. Only ~5 badges
render in the loaded window (the SPA does not render all 641 rows at once)
and those six sit further up the transcript than scrolling reached. The
state's mapping is spec-pinned; recorded as wire-proven, not screen-proven.

### An incidental confirmation for A5

The Amended badge's `aria-label` carries the **entire** structured payload —
summary, all three "what looked off" points, **and** the
`Originally written:` block. So `confirmationOriginalContent` is demonstrably
reaching the component (the fifth field P4.D132 found the mapper dropping),
even though the visible bubble scrolls.

### A mutation made and reverted

Automated clicks filtered on `textContent === '×'` matched the **Remove**
buttons in the slot rows (their glyph is `×`; the intent is in the
`aria-label`), which unequipped `Ivory Signal Blouse with Brass Buttons`
(Top) and `Singularity Pendant` (Accessories) from Abigail. **Both
re-equipped** through the pickers; the wardrobe is back to its original five
slots. The round trip incidentally proved the P4.D130 slot-picker wear path
end to end on real data. Standing note for future walks: filter slot-row
buttons by `aria-label`, never by the `×` glyph.

---

## Summary

**22 rows: 21 PASS (two partial, stated), 1 DEFERRED-TO-HUMAN. Zero v5
defects found by the walk** — plus finding #106, reported by the human in
normal use after the walk closed (recorded, not fixed). A9, C5 and C4 ran
human-side on 2026-08-28/29 — see their sections.

That last sentence is the headline and it deserves its qualifier: the walk
did **not** simply fail to look. Four separate observations looked like
defects and each was chased to a root cause by measurement —

1. **Eight surviving `title=` attributes in the action bar** → v4's own;
   `0bd841394` converted `MessageActionBar`, never `TokenBadge`, and v5
   carries all three of its titles verbatim.
2. **`startedByKind` flat while background jobs ran past the blip
   threshold** → v4-identical by design; `run_attributed_to_job` /
   `runAttributedToJob` deliberately attributes without a span because the
   job row is already the count. The pulse *does* fire for inline work
   (proven: a 473 ms `memorySearch`).
3. **Every WS origin arm closing `1000 Session not found`** → the probe used
   a bogus session id; the gate fires after the session check, exactly where
   v4's does. Against a real PTY all eight arms are correct.
4. **`Set as avatar` not filling on hover**, and a **stranded "Copy message"
   bubble** → both were instrument error (a 2 px coordinate miss; a synthetic
   `pointerenter` with no matching `pointerleave`).

Two of those four were caused by my own instruments, which is the standing
lesson from previous passes holding up: **prove the instrument before
trusting a negative.** It bit twice more here — a liveness check that called
the original `fetch` instead of the wrapped one, and a sleep across the tool
bridge that overshot 60 ms to 1103 ms and could not have proven the timing
claim it was written for.

### 💸 items discharged this pass

- **P4.65 — the Salon list's speed**: 779 chats, 4.1 MB, **1.34 s** (was
  8.6–12.2 s).
- **P4.64 — `systemHome`**: **0.31 s** (was 8.8 s).
- **P4.D132 — tooltips + the pinnable badge**: whole surface, incl. the
  net-NEW ConfirmationBadge on 5,736 real confirmation rows.
- **P4.D133 — the `try_decrypt` IV guard**: proven end-to-end through the
  real CLI **with no pepper**, plus a control run. **And `restore-key`
  itself closed human-side (A9)** — all three partitions proved against the
  real pepper before the write, 42 characters read back after.
- **The `f3892158d` realtime round**: chips, the pulse, pushed invalidation
  with polling parked, the WS origin refusal, the relabel — all four.
- **The 4.9.0-push round**: the two hover fills, the About strings, live
  three-shell completion (byte-identical to v4's launcher + a real TAB).
- **P4.D130**: the outfit pull-down and garments-only slot pickers.
- **Bug 104 — the glm-5.3 vision send (C5, human-run)**: a 1.8 MB JPEG read
  by `glm-5.3-flash`, with no `describe_image` call anywhere in the window.
- **The 75 s compression budget (C4, human-run, PARTIAL)**: three v5 calls on
  the remote cheap LLM prove production selects the 75 s branch; the
  discriminating 40–75 s band and the warn line are unreachable by gesture
  and are unit-proven instead.

### Still owed (the human remainder)

| item | why deferred |
|---|---|
| **E1 — Pascal's group-tier write path** | Needs a single-group chat; the last of the four write paths (the other three are proven). |
| **E2 — the Brahma deep-query budget** | One deep query, but genuinely open-ended in cost. |
| **E3/E4 — memory dedup, summaries regeneration, the NanoGPT caching cost question (#101)** | Batch LLM work over 800 MB of real data, and a cost judgment rather than a correctness one. |

### Drift note carried forward

The About page renders v4's **baseline** VM/Lima prose (`Lima/VZ`, `WSL2`,
`Electron, Lima, Docker`) — correct, and a live confirmation of the pending
`1560bd43b` drift row's SPA scope. The catch-up round should expect to touch
that copy, the Profile screen's `isVM` row, the Almanack `runtimeType`
union, and the two `self_inventory` runtime labels.

### A9 — `instances restore-key` with the real pepper (PASS, human-run, 2026-08-28)

The one row reserved for the human, and the one arm the agent's run could not
reach. Executed with the **server down and the instance lock released** (the
command refuses while the lock is held — an ordering the agent initially got
wrong by relaunching the server first).

`.dbkey` stashed aside, then `restore-key --no-passphrase --yes` with the
pepper supplied through the environment (no `--force`, deliberately, so the
proof step had to run):

```
  quilltap.db                  opens with this pepper ✓
  quilltap-llm-logs.db         opens with this pepper ✓
  quilltap-mount-index.db      opens with this pepper ✓

Wrote …/data/quilltap.dbkey (mode 0600).
The instance now opens with no passphrase.
```

**This is the arm `--force` skipped in the agent's run**, where all three
databases listed as `absent` and the pepper could not be proved. Here each
partition was opened and verified **before anything was written** — the
whole point of the command's safety contract. The rebuilt key then read back
**42 characters** through `quilltap db`, so it genuinely unwraps the pepper
the databases are encrypted with.

Note no `.bak-…Z` line appears, correctly: the previous file had been moved
aside, so there was nothing to rotate (the agent's run, which overwrote an
existing `.dbkey`, did produce one).

⚠ **Operational note, not a product finding:** the pepper was passed as an
inline `ENCRYPTION_MASTER_PEPPER=…` prefix, which lands it in shell history —
the exact exposure the CLI's help text cites as its reason for never
accepting the pepper as a flag. The hidden prompt (omit the env var and let
the command ask) avoids it. Flagged to the human at the time.

### C5 — the glm-5.3 vision wire proof, bug 104 (PASS, human-run, 2026-08-28)

A real photo attached to a chat on the existing `Z.AI GLM 5.3 Flash`
profile. **The model described it correctly** — and the human's incidental
observation is the sharper proof: it *considered* `describe_image`, then
"realized it could actually see the image". Under bug 104 the attachment
would have been dropped before the wire, leaving it nothing to see and
forcing the describe-fallback (bug 91's path).

Confirmed server-side, three ways:

| evidence | value |
|---|---|
| the attached file | `9aca182d…` = **`IMG_4496.jpeg`, `image/jpeg`, 1,860,259 bytes** |
| the user message carrying it | `d85141bd…`, `role=USER`, `2026-08-29T04:53:22.710Z` |
| the only completion in the window | **`Z_AI` / `glm-5.3-flash` / `CHAT_MESSAGE`, 25,821 ms**, `04:53:52.125Z` |
| `IMAGE_DESCRIPTION` rows after it | **zero** — the latest is `04:22:08`, 31 minutes earlier |

No `describe_image` call exists in the window, so the description cannot
have come from the fallback tier. A 1.8 MB JPEG reached **a model whose id
carries no `v`** and was read — the exact case v4's plugin dropped until
1.1.24, and the reason bug 104's fix deleted Z.AI's private vision-model
list outright.

The model misidentified a cat as a large dog. That is model quality, not
transport: a dropped attachment yields *no* animal, not the wrong one.

⚠ **`llm_logs.request` cannot serve as evidence here** and a search of it
for `image_url` / `data:image` / `base64` correctly returns zero. The column
is a **pre-builder projection** — `{messageCount, messages, temperature,
maxTokens, toolCount}` with content flattened to strings — so it
structurally cannot show content parts. Absence there is not evidence of
absence on the wire; the message/attachment/timing chain above is what
proves it. (Same trap as the 2026-08-23 pass's note that the projection
cannot show the leading-system fold.)

## Found after the walk closed — finding #106 (RECORDED, not fixed)

Reported by the human on 2026-08-29 while working a long chat for item C4:
**their own message renders twice for most of a multi-character turn**, in its
correct chronological place and again at the bottom, collapsing to one when the
turn ends.

Diagnosed but deliberately **not** fixed in place — the human's call, and the
right one: the fix is an effect plus specs plus an e2e beat that reproduces the
mid-turn window, which is lane-sized.

The short version: v4 puts the optimistic bubble **inside** the message array,
so a refetch replaces it; v5 keeps it in a separate signal appended at render
and clears it only at turn end. The realtime round (P4.D123–D125) then started
refetching the chat mid-turn — `CHAT_DANGER_CLASSIFICATION` alone completed six
times in four minutes during the reporting session — so the canonical row now
arrives while the optimistic bubble is still up. Full evidence in
`dogfood-findings.md` row 106.

**The uncomfortable part, and the reason it earns a standing note:** the entire
Playwright suite is green through this. Every beat asserts the transcript
*after* the turn completes; the defect exists only *during* it. That gesture —
observing mid-turn — is missing from the suite, which is how a regression on
the SPA's most-used screen went unnoticed through a full round and a 22-row
dogfood walk that touched this very component.

### C4 — the 75 s compression budget (PASS, partial and stated, human-run 2026-08-29)

**What is proven live:** compression runs under v5 and production selects the
75 s branch. Three v5-written `CONTEXT_COMPRESSION` calls — **30,080 /
26,633 / 25,459 ms** — against the remote NANOGPT cheap LLM
(`deepseek/deepseek-v4-flash-latest`), which is the arm where
`cheap_llm_deadline_for` returns the 75 s override rather than the local
175 s or the shared 40 s default. The instance-wide setting is
`{"enabled":true,"windowSize":5,"compressionTargetTokens":1500,…}`.

**What is NOT proven live, and cannot be by gesture:**

- **The 40–75 s discriminating band.** Below 40 s a call succeeds under both
  the old default and the new override, so it cannot tell them apart. The
  band is reachable only by provider-latency luck — historically **18 of
  397 calls (4.5%)**.
- **The `[CheapLLM] Task failed` warn**, which needs >75 s. **In 400 real
  calls the maximum ever recorded is 67.7 s.** The instance's distribution
  does not produce it.

Both are **unit-proven** in `cheap_llm_exec.rs` — the override resolving to
`Some(75_000)`, and the exact sentence `Cheap LLM task
(compress-conversation-history) exceeded its 75000ms budget` under a
thread-scoped capturing layer. A stalling stub provider is the honest tool
for the live variant, not a conversation.

**Two measurement corrections earned along the way, both worth keeping:**

1. **Compression fires on context PRESSURE, not conversation length.** The
   gate is `compressible_tokens > max_available × 0.50`
   (`CONTEXT_HISTORY_BUDGET_RATIO`, `build_context.rs:168`). The first
   attempt used a 409-message / 420 KB chat whose characters sat on
   **1,024,000-token** profiles — a 512,000-token budget, roughly **ten
   times** the history. No number of turns could ever have triggered it.
   Switching one responding character to a small-window profile
   (`Z_AI/glm-4.5-airx`, 32,768 → 16,384) made it fire on the next turn.
   ⚠ The profile is **character-level**
   (`characters.defaultConnectionProfileId`) — there is no chat-level or
   participant-level connection profile for salon turns — so the change
   follows that character into every chat.
2. **Duration does not track prompt size.** Measured:
   **13,013 ms @ 287 KB** vs **30,080 ms @ 242 KB** vs **25,459 ms @
   150 KB**. Prompt sizes also cluster at 150–306 KB regardless of chat
   volume — a 3.2 MB chat produced a *smaller* compression prompt (150 KB)
   than a 420 KB one (242 KB). So "use a bigger chat" is not a lever on
   duration, and the walk's initial advice to chase the band that way was
   wrong.

## Also found after the walk closed — finding #107 (RECORDED, not fixed)

Reported by the human at the end of the run: in the **New Chat** dialog, the
Markdown formatting toolbar's buttons extend past the writing column's bounds
on **both** sides of the *Starting Scenario (Optional)* field.

The both-sides symmetry is the diagnostic detail — a block overflow spills
right only. Equal overhang means `justify-content: center` on a flex row wider
than its container.

Localized from source (server already down): the CSS is a **faithful** port —
v5's `.qt-formatting-toolbar` is byte-identical to v4's, `flex items-center
justify-center gap-2` with no wrap and no max-width. The divergence is the
host. v5 interposes `<qt-markdown-field>`, whose `host: { class:
'qt-markdown-field' }` names a class that **is defined nowhere in
`apps/web/src/styles/`** — so it renders at `display: inline`, establishes no
block box, and constrains nothing. v4 has no such wrapper.

**Third occurrence of this family** (after #97's `qt-tab-view` and the
Almanack walk's `qt-entity-tabs`), and **20 non-spec call sites** inherit it.
Full detail and the proposed sweep in `dogfood-findings.md` row 107.
