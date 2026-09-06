# Dogfood walk — the `p4.9i2` help/HelpChat round (P4.9I2A ∥ P4.9I2B ∥ P4.76 ∥ P4.77)

**Date:** 2026-09-05/06. **Driver:** Claude (agent-driven), with a short human
remainder. **Instance:** a COPY of real Friday at `~/qt-dogfood-friday` (the
human rsynced at 21:30–21:42 and built both halves at 21:37; the merge on
main is `6cdcdf6c` at 20:46 — everything post-dates it).

**Server:** `RUST_LOG=info,quilltap::pascal=debug RUST_BACKTRACE=1
./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir
apps/web/dist/quilltap/browser --port 3400` (the human is using 3000), log in
the scratchpad. The instance **auto-unlocks** (saved pepper). The Pascal debug
filter is on so P4.77's `render_template` debug lines (`tracing::debug!`) can
be seen at all.

**Queries:** `./target/release/quilltap db --data-dir ~/qt-dogfood-friday
--json "…"` (main), `--llm-logs` for `llm_logs`.

## Rounds under test

The `p4.9i2` help/HelpChat round, UNIFIED 2026-09-05: the help server whole
(the vendored + embedded `help/` tree, the boot-time ensure + the new
`help_docs` table ensure, the help-docs read verbs, the nine help-chats verbs,
the context resolver + help system prompt, the help-chat orchestrator on a
LIVE send seam) ∥ the HelpChat SPA (the Help dialog's Guide + Ask tabs, the
streaming fold, the entity picker, the rail entry) ∥ P4.76 (`POST
/api/v1/images?action=generate`, the FILES leg, the five recorded items) ∥
P4.77 (the `zod` guard, P4.D159, the capture-rig consolidation,
`render_template`'s debug lines).

## §0 Drift state at walk start

The ledger's §2 probe PASSED (v4 on `bugfix`, tree clean, `git log
c2232cd9a..main` and `git log 2b49f51aa..bugfix` both empty). Baseline
`c2232cd9a` = v4 HEAD; NO DRIFT; no surface carries a pending §3 row (the one
row, bug 119, is the unported `p4.9k`). Nothing in this walk can blame drift.

## §0.5 Pre-walk measurement (ledger §5.5)

- **The boot ensure ran SILENTLY on real data — the cross-app proof.** v4 had
  synced this instance's `help_docs` long ago (oldest `updatedAt`
  2026-06-09); v5's `ensure_help_docs_synced` found the path set current and
  wrote no doc row (no `Help documents synced` line in the log). 120 docs.
- **The P4.D77 upgrade backfill ran live at the same boot:** every doc had
  chunks 45 s later (`docs_without_chunks = 0`, 597 chunks), 120
  `EMBEDDING_GENERATE` jobs were minted at 02:45 and the pump was draining
  them during planning (63 → 167 `HELP_DOC` rows `EMBEDDED`, 1 `FAILED`; 456
  of 597 chunk embeddings present by 02:48). 💸 discharged before the walk
  began — real embedding spend, small.
- **A v4 wart, reproduced, recorded:** the stored help CONTENT on this
  instance predates the shipped tree (`help/agent-mode.md` stored 6,672 bytes
  vs shipped 6,713; `help/tabbed-workspace.md` 7,846 vs 8,704; hashes
  differ) because BOTH apps re-sync only when the PATH SET diverges (v4's
  `helpDocsDivergeFromDisk`, P4.d6) — 597 chunks here vs the e2e's 667 over
  the shipped tree is that difference, not a chunker defect. `help_search`
  and the Guide therefore serve stale prose on a long-lived instance in v4 as
  in v5. **Candidate upstream filing** (compare content hashes, not paths).
- Help characters present: **Riya** (default profile `DeepSeek V4 Pro
  Thinking`, NANOGPT, tool-capable, the instance default) and **Lorian**
  (`ChatGPT 5.5 Low Verb`, OPENAI). 12 v4-written help chats (latest
  2026-08-14). 812 salon chats (the welcome card's `< 3` gate is closed).
  Concierge `AUTO_ROUTE`; image profiles incl. the default `GPT Image 2`
  (OPENAI) and five WaveSpeed `isDangerousCompatible` rows. Keys for eleven
  providers incl. GOOGLE; **no GOOGLE connection profile exists** (A11 creates
  one over the existing key).

## §1 What NOT to expect to work

- The Guide auto-expanding a category: under the tabbed workspace v4's
  `usePathname()` is `/workspace` on every page (v4's own `/aurora` redirects
  into it), so nothing auto-expands in v4 either — recorded at the unification.
- The help chat's page context is likewise the `/workspace` doc
  (`tabbed-workspace.md`, `url: /workspace`) everywhere — v4-faithful.
- "Open this page in Quilltap" does NOT close the reader (v4's
  `handleNavigatePage` is `navigate(url)` alone).
- `update-context` PATCHes fire only when the pathname changes; inside the
  workspace it never does — so no `[System: User navigated to …]` rows.
- A help chat has no rename UI in v4's dialog (the verb exists; the dialog
  offers delete only).
- Rows the help chat's cheap-LLM tail cannot write on this vintage: none
  known — but the last pass's `llm_logs has no column named
  connectionProfileId` was the e2e fixture's vintage, not Friday's.

## Part A — the Help dialog (P4.9I2A ∥ P4.9I2B)

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | 💸 the boot sync + the P4.D77 backfill on a real instance | (measured before the walk, §0.5) | silent short-circuit; 120 jobs; chunks for every doc; embeddings arriving | **PASS** (§0.5) |
| A2 | CLAUDE | The rail entry + the Guide | Click the question-mark in the left rail (broad: the button body). | The dialog opens on the Guide tab; eleven categories with document-count badges; no welcome card (812 chats); the entry sits BEFORE the Brahma entry. Verify by `read_page` + the `[HelpDocs] Listed help documents document_count=120` log line. | **PASS** — dialog on Guide; eleven categories, badges 3/9/14/5/5/3/6/8/15/4/3 (Chats shows 14 of its 15 slugs — the file-less `shell-tools`, as v4); no welcome card (812 chats); rail order `qt-user-menu, qt-help-entry, qt-brahma-entry, …`; a centred `qt-dialog-overlay`; server `[HelpDocs] Listed help documents document_count=120` |
| A3 | CLAUDE | Guide text search + the snippet line | Type `describe` (a prose-only term) in the search box; then `uncensored`; then clear with the ✕. | Topics narrow on the first keystroke (title filter), then the server search lands (`[HelpDocs] Guide text search query=… match_count=N`) and matched topics show a muted snippet line under the title; clearing restores the full list. | **PASS** — `describe`: the instant title filter narrowed first, ONE settled request (`Guide text search query=describe match_count=24`; 17 of the 24 live in categories), 8 categories force-expanded, muted `…`-led snippet lines with the lopsided 30/160 window; `uncensored` → the two expected topics (Dangerous Content Handling, Scene State Tracker) with their snippets; the ✕ empties the box, collapses all eleven, and refocuses the input (v4's `inputRef.focus()`); a nonsense query renders zero topics. Instrument slip: the pane's `cmd+a` never reached the input — retyped after a clear instead |
| A4 | CLAUDE | The topic reader, math, links | Open `Math Notation`; scroll; click a related-topic link; use Back; open a topic whose page link is `/settings?tab=chat` and click "Open this page in Quilltap". | Markdown + KaTeX render; a related link opens IN the Guide; Back restores the list AND the scroll position; the page link opens/focuses a workspace tab in place with the reader still up (v4). | **PASS** — `Mathematical Notation` renders (six headings, one live KaTeX node `a²+b²=c²`, the eight `$`-bearing examples as code spans); Back reads `Chats (The Salon)`; the Related Pages links are in-Guide `qt-help-guide-doc-link` buttons (v4's `a` override) — following one opens `Message Actions` in the reader; Back returns to the PREVIOUS DOCUMENT at its saved scroll (600 px), a second Back to the list with every category collapsed (v4's sections unmount under the reader, so their ref-held open state resets — consistent, recorded); `Chat Settings` → `Open this page in Quilltap` opened the Settings tab (`The Foundry`) with Chat selected, in place, URL still `/workspace`, the reader still up (v4's `navigate(url)` alone). Instrument: the pane was hidden, so the reader was scrolled by script, not the wheel |
| A5 | CLAUDE | The Ask launcher over v4's rows | Switch to Ask. | The seat pills (Riya, Lorian) with avatars; "Recent" lists the 12 v4-written help chats with participants + message counts (v4's `countVisibleMessages` — SYSTEM/TOOL rows excluded); open one → its transcript renders v4's rows. | **PASS** — pills Riya + Lorian with avatars, Riya auto-selected (`quilltap:help-chat-selected-characters = ["f11db2bc…"]`, the first tool-capable seat); `Recent Help Chats` lists all 12 v4-written chats; the badge for `What can Riya do?` reads **7** = every event row (4 tool events + SYSTEM + USER + ASSISTANT — v4's list route counts `messages.length`, measured against the DB); opening it renders v4's USER + ASSISTANT rows with markdown (the tool events hidden, as v4's list does) and stores `quilltap:help-chat-last-id` as a PLAIN string; `quilltap:help-tab = ask` in sessionStorage |
| A6 | CLAUDE | 💸 A real Ask turn with a tool | New conversation, Riya only, ask: "Where do I change the app's theme? Take me there." | Streamed reply; `help_search`/`help_navigate` TOOL rows in the transcript; a navigation button under the reply whose click opens the settings tab; `llm_logs` rows for the chat (the streamed call + the tool loop, provider NANOGPT); the created chat has `chatType='help'`, `helpPageUrl='/workspace'`, a SYSTEM `Help chat initiated for page: /workspace` row; the async tail: a `MEMORY_EXTRACTION`-class job and, at a checkpoint, the title job. | **FAIL(#111, #112)** — chat `37c6289c…` created (`chatType='help'`, `helpPageUrl='/workspace'`, the SYSTEM `Help chat initiated for page: /workspace` row, the title job renamed it *Changing app theme location*, `MEMORY_EXTRACTION` ran on DEEPSEEK); `help_search` + `help_navigate` executed (10 TOOL rows) — but **eight empty ASSISTANT turns**, the duplicate-call guard on turn 9 (`duplicate_count=3`), a forced-final EMPTY, no assistant row, nothing on screen. Two findings: **#112** the loop's id-less tool rows never reach a NANOGPT model (v4-faithful, nine plugins drop them — HUMAN cross-check on live v4 owed); **#111** not one `CHAT_MESSAGE` row in `llm_logs` for the ten streamed calls — FIXED `4abbfe3b`, re-run as A6b. |
| A6b | CLAUDE | 💸 A6 re-run on the fixed build | New conversation, Riya only, a question needing NO tool ("In one sentence, what is the Salon?") — #112 makes a tool-needing turn end silent on this seat. | A streamed reply on screen; an ASSISTANT row; **one `CHAT_MESSAGE` row in `llm_logs`** with `messageId` NULL, `characterId` = Riya, `connectionProfileId` = her NANOGPT profile, `durationMs > 0`. | **PASS** — chat `23d41c8c…` (*What is the Salon in one sentence?* after the title job); the reply on screen and as the ASSISTANT row (147 chars, Riya's participant); **`llm_logs` `CHAT_MESSAGE` row `ff19a3a6…`**: `messageId` NULL, `characterId` `f11db2bc…` (Riya), `connectionProfileId` `dbc68593…` (her NANOGPT default), `durationMs` 2910, usage 161/88/249, `cacheUsage` present, request 25,742 bytes; the tail wrote TITLE_GENERATION + MEMORY_EXTRACTION rows as before. **Observation, v4-faithful:** the MEMORY_EXTRACTION *system-event* row in `chat_messages` stamps the seat's model (NANOGPT `deepseek-v4-pro:thinking`) while the call went to the cheap LLM (DEEPSEEK `deepseek-v4-flash`) — measured on 40 v4-written help-chat event rows: 40/40 differ from their `llm_logs` row the same way. Candidate upstream nicety, not a v5 defect. |
| A7 | CLAUDE | The page context in the prompt | From A6's `llm_logs.request`. | The system prompt carries `## Help Assistant Role`, `## Current Page Context` naming the `/workspace` doc (`Tabbed Workspace`), `### Additional Context:` for the three wildcard docs (`search`, `sidebar`, `width-toggle`) — incl. the duplicate-wildcard quirk ONLY if the primary were a wildcard (it is not here), and the agent-mode block with `10` turns. | **PASS** — from row `ff19a3a6…`'s `request` (`messageCount` 2, `toolCount` 23): the system prompt (24,975 chars) carries `## Character Identity`, `## Help Assistant Role`, the MANDATORY tool-execution block, `## Agent Mode Instructions` with *up to **10 tool iterations*** (the HELP_MAX_AGENT_TURNS constant on the wire), `## Current Page Context` → *The Tabbed Workspace*, `URL: /workspace`, `### Page Documentation` with the whole doc, then exactly three `### Additional Context:` sections — *Search Bar*, *Left Sidebar*, *Width Toggle Button* (the wildcard docs, in that order; no duplicate — the primary is not a wildcard), then `## User Character` + `## Identity Reminder`. |
| A8 | CLAUDE | 💸 Multi-character help turn | New conversation with BOTH seats, ask a one-line question. | Both answer in sequence; the SSE carries `turnStart`/`turnComplete` (no `skipped` key) for the second seat and one `chainComplete`; the transcript has two ASSISTANT rows with distinct `participantId`s. | **PASS** — chat `ff942ee4…`, both seats selected, *"Name one thing the Commonplace Book is for."* Captured on a second `EventSource` (proven open before the send, 0 frames pre-send): for Riya three `toolsDetected`/`status`/`toolResult` triples (`help_search`, the same query thrice — #112's shape), then `done` (`messageId`, `usage`, `cacheUsage`, `attachmentResults`, `toolsExecuted`), then **`turnStart` for Lorian only** (`characterName`, `chainDepth`), Lorian's `done`, **`turnComplete` with NO `skipped` key**, **one `chainComplete`** (`reason`, `nextSpeakerId`, `chainDepth`), then the realtime hint. Transcript: Riya's three empty ASSISTANT + TOOL pairs, her forced-final answer (319 chars — this time the model answered from its own knowledge), Lorian's 152-char reply (OPENAI `gpt-5.5`, one call, no tool); six `CHAT_MESSAGE` rows (5 Riya NANOGPT + 1 Lorian OPENAI, all `messageId` NULL, durations 2.9–4.6 s) + the title row. **#112 on the wire:** the logged requests carry `assistant` rows for each empty turn and **zero `tool` rows** on NANOGPT — the model never sees a tool result and repeats the search. Observation: Lorian's request carries Riya's three empty assistant rows in its history (the tier-3 `two_characters_with_tool_history` case pins that shape at the pin). |
| A9 | CLAUDE | Past-chat delete + reload | Delete the A8 chat from the launcher; reload the tab; reopen Help. | The row is gone; the last-chat id in `localStorage` (`quilltap:help-chat-last-id`, a PLAIN string) is cleared when it was current; the selected seats persist (`quilltap:help-chat-selected-characters`); the tab choice persists in `sessionStorage`. | **PASS** — the A8 chat deleted from the launcher's hover ✕ (`title="Delete"`, no `confirm()` — none was called): the row vanished from Recent (15 → 14), `chats`/`chat_messages` rows 0/0 while its 11 `llm_logs` rows stay (v4 keeps the ledger); after a full reload Help reopens on Ask with 14 Recent rows and no *Commonplace* row, both seats still selected from `quilltap:help-chat-selected-characters`, last-id null. **Measurement note:** the *cleared when it was current* branch (`help.service.ts:145`) cannot be discriminated through the UI — reaching the launcher's Recent list goes through *New help chat*, which clears the key first; recorded, not a defect. |
| A10 | CLAUDE | Seat snap-back (the §3 fix) | In the launcher, deselect BOTH seats. | The last deselection snaps the first tool-capable seat back (v4's effect deps) — the pill re-lights. | **PASS** — discriminated, not just observed: Riya alone → click Riya → Riya stays (`data-selected`); then +Lorian, −Riya → *Lorian alone* (no snap while one seat remains), then −Lorian (the last deselection) → **Riya** re-lights, not Lorian — the first tool-capable seat, v4's effect; `localStorage` `quilltap:help-chat-selected-characters` follows (`[Riya]`). Also seen: *New help chat* clears `quilltap:help-chat-last-id`. |
| A11 | CLAUDE | 💸 A GOOGLE-seated help chat (the §3 finding's live leg) | Create a GOOGLE connection profile over the existing key (a Gemini model from Fetch Models), set it as Lorian's default, ask Lorian a tool-needing question, then a follow-up. | The second turn's `llm_logs.request` shows the tool row carried (a `tool` role message with the result JSON) where a NANOGPT seat's would drop it; the model's follow-up answer uses the tool result. Restore Lorian's default after. | PENDING |
| A12 | CLAUDE | The entry's disabled rule | (measurement only) | With two eligible seats the entry is enabled; the disabled arm needs no eligible character — not exercised on this data, recorded. | **PASS** (measured) — the rail's Help button `disabled=false`, no `aria-disabled`, two eligible seats in the launcher; the disabled arm needs an instance with no eligible character, which this data cannot supply — recorded. |

## Part B — `POST /api/v1/images?action=generate` + the FILES leg (P4.76)

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | The Zod gate, no spend | `curl -X POST '…/api/v1/images?action=generate'` with `count: 11`, then `size: 'huge'`, then an empty prompt, then `prompt: 42`. | Each a 400 `Validation error`; nothing written (`files` count unchanged); the `api.v1.images.generate` activity anchor logs. | **PASS** (the first pass was VACUOUS — every shape lacked the required `profileId`, so all four 400s were that; re-run with a present-but-nonexistent `profileId` so a *valid* parse answers `Connection profile not found` instead, which makes each arm discriminating) — v4's real `generateImageSchema` arms: `options.n` 11 / 0 / 1.5 → **400 `Validation error`**, `n: 10` → parse OK (`Connection profile not found`); `quality: 'huge'`, `style: 'vivid2'`, `options: null` (optional-not-nullable), `tags[0].tagType: 'BOGUS'`, empty `prompt`, `prompt: 42`, `profileId: 'not-a-uuid'` → all **400 `Validation error`**; `options.size: 'huge'` and `tagId: 'not-a-uuid'` parse OK (`z.string()` both — the walk's original `size` gesture was wrong about v4). `files` 2,903 before and after.
| B2 | CLAUDE | The route-only profile miss | `imageProfileId` of a random uuid. | **400** `Connection profile not found` (this route; the image-profiles route's 404 is NOT copied). | **PASS** — a valid body with `profileId` `7d0d9a5e-…` (well-formed, absent): **400 `{"error":"Connection profile not found"}`**, nothing written. |
| B3 | HUMAN | 💸 A real generation through the route | One image on the default `GPT Image 2` with a plain prompt. | 201 with `{ images: [{id, filename `generated_<ts>_0_<sha8>.webp`, …}] }`; a `files` row `source=GENERATED` under the Lantern mount's `tool/` subfolder; the `[Images v1]` lines. | PENDING |
| B4 | HUMAN | 💸 The AUTO_ROUTE reroute | A prompt the classifier flags (the operator's judgement) with the OPENAI default. | The classification call in `llm_logs`; the generation goes to the FIRST `isDangerousCompatible` profile (`WaveSpeed Flux`, the first in id order — measure), not the desk; the receipt names it. | PENDING |
| B5 | CLAUDE | The FILES leg's raw `tagId` | `POST /api/v1/files` multipart with `tags[0][tagId]=not-a-uuid`. | v4's **500** `Failed to upload file`; the bytes are written then the row refused — measure `files` before/after. | **PASS** — multipart to the *Quilltap General* store with `tags=[{"tagId":"not-a-uuid"}]` → **500 `Failed to upload file`**; `tags={"a":1}` (truthy non-array) → 500 the same; `tags=[not json` → **400 `Invalid tags JSON`**. `files` 2,903 before and after (the main row refused). **And the bytes ARE written first, as v4's order has it:** the first refused upload left blob `1c17c611…` (32 bytes, sha `da89e3e7…`) + mount file row `5a93b08f…` in the *Quilltap Uploads* store at 03:31:12Z with **no main `files` row** — an orphan, v4-faithful; a later good upload (201, deleted after) deduped onto that same blob by sha, so blob/file counts never moved. |
| B6 | CLAUDE | Import-from-URL with an opaque scheme | `POST /api/v1/images` `{url: 'data:image/png;base64,<1×1 png>'}`. | 201; the stored basename `png;base64,….webp` (Zod's `z.url()` accepts it; the WHATWG opaque path) — the §3 review's measured shape. Delete the row after. | **FAIL(#113) → FIXED, re-run below as B6b** — `{url: 'data:image/png;base64,<1×1 png>'}` answered **500 `Internal server error`**: the host's reqwest seam refuses the `data:` scheme (a thrown fetch → the middleware 500), where Node's `fetch` runs the Fetch Standard's *data: URL processor* and v4 imports it (the review's measured `png;base64,….webp`). Fixed in `quilltap-host`'s `image_import_fetch.rs`: the processor transcribed from the standard, pinned by six vectors measured on Node 24 (`200 OK`, the mediatype as `content-type` with parameters kept, `text/plain;charset=US-ASCII` when absent, percent-decoding, `DATA:` case-insensitive, malformed base64 → thrown), mutation-proven. |
| B6b | CLAUDE | B6 re-run on the fixed build | The same `data:` body. | 201; the stored basename `png;base64,….webp`; a `files` row `source=IMPORTED`; delete after. | PENDING |
| B7 | CLAUDE | `?action=bogus` on the images POST | `POST /api/v1/images?action=bogus` with a tiny multipart image. | Falls through to UPLOAD (v4's route shape); 201. Delete after. | **PASS** — `POST /api/v1/images?action=bogus` with a 1×1 PNG multipart: **201** (v4's route shape — an unknown action falls through to upload), the row `ee91efed…` `source: upload`, **`b7.webp` / `image/webp` / 98 bytes / 1×1** (the host pixel codec transcoding on this path too); `files` 2,903 → 2,904 → 2,903 after `DELETE /api/v1/images/{id}` (200 `{success:true}`). |

## Part C — P4.77

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | `render_template`'s debug lines | In the Workbench, dry-run a custom tool whose message references `{{metadata.nope}}` and `{{state.nope.deep}}` and `{{bogus}}`. | Three `DEBUG quilltap::pascal` lines in `combined.log`: `…references metadata the character cannot render` (`reason=no such metadata key`), `…references state it cannot render` (`reason=no such state path`), `…carries an unknown placeholder`; the rendered message unchanged (placeholders left verbatim). | PENDING |
| C2 | CLAUDE | The `zod` guard + P4.D159 | (not a browser surface) | Proven at the gate; the mirror refreshed. | **PASS** (gate) |

## Part D — the standing 💸 queue

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| D1 | HUMAN | The Brahma deep query on a raised budget | Ask the Console a question needing several `run_sql` turns. | Answers within the budget; salvage on exhaustion. | PENDING |
| D2 | HUMAN | Memory dedup / conversation-summary regeneration | Settings → the two cards. | The jobs run to completion; row deltas. | PENDING |
| D3 | HUMAN | #101 NanoGPT caching cost question | (judgement) | — | PENDING |
| D4 | — | The LoRA wire-byte look | BLOCKED (HTTPS; pre-builder projection) | — | BLOCKED |
| D5 | — | The Opus 5 byte strip | BLOCKED (HTTPS) | — | BLOCKED |

## Part E — regression smoke around the seams the round touched

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | The Salon still streams | Open a salon chat, send one line. | Stream + `done`; the `turnComplete` frame still carries `skipped` (the `Option` widening — Salon writes `Some`). Verify on `/api/events`. | PENDING |
| E2 | CLAUDE | Brahma Console's composer (shared `qt-help-composer`) | Open the Console, type, Enter. | Unchanged. | PENDING |
| E3 | CLAUDE | The rail order + the dialog chrome | Look. | Help before Brahma; the dialog a centred overlay (the recorded divergence). | **PASS** — Help before Brahma in the rail (measured on the footer's children); the dialog is the recorded centred overlay |

## Findings

- **#111 — FIXED (`4abbfe3b`, core 0.0.806, harness 0.0.695).** Help (and
  Brahma) streamed turns wrote no `llm_logs` rows; v4 logs every
  `streamMessage` call at `chunk.done`. The help loop now logs on a clean
  stream end with `messageId` NULL; pinned in the tier-3 family (26 rows,
  mutation-proven). Brahma's identical gap is a recorded follow-up.
- **#112 — RECORDED, v4-faithful.** A tool-needing help turn ends in silence
  on nine of ten providers because the loop's tool rows carry no
  `toolCallId` and the plugins drop id-less tool rows; only GOOGLE keeps them.
  Candidate upstream filing after the HUMAN cross-check on live v4.
- **#113 — FIXED.** `POST /api/v1/images {url: 'data:…'}` was a flat 500:
  reqwest has no `data:` URL processor; Node's fetch does, so v4 imports the
  payload. The host seam now runs the Fetch Standard's processor locally
  (six Node-measured vectors, mutation-proven).
- **Stored help-doc content is stale against the shipped tree** (§0.5): the
  sync's divergence trigger is path-only, so a doc whose bytes changed under
  the same path keeps its old `content`/chunks — v4-faithful (the same
  trigger), candidate upstream filing; the boot sync being silent on this
  instance is the cross-app proof that v4's rows satisfy v5's check.

## Instrument notes

(filled as the walk runs)
