# Dogfood walk — the eight-round backlog: compressed text, the Scenario
# Builder, the Concierge overhaul, the Post Office's letters (2026-09-29)

**Instance:** `~/qt-dogfood-friday` — a COPY of real Friday data, rsynced
2026-09-29 ~11:24 local (the `.db` mtimes). Disposable; never rsynced back.
**Server:** `RUST_BACKTRACE=1 ./target/release/quilltap-web --data-dir
~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser` (RUST_LOG in §2).
**Log of record:** the scratchpad server log + `~/qt-dogfood-friday/logs/combined.log`.
**Builds:** `quilltap-web` release 11:30 (core 0.0.1104); the `quilltap` CLI
was NOT in the human's release build (only `quilltap-web` was present) — built
by Claude with `cargo build --release -p quilltap-cli` before the measurements.

**Rounds covered** (every round unified since the 2026-09-18 pass):
`f45a517a9` (thirteen-commit) · `a2db63da7` (bugs 161/162) · `00c290c9a`
(bugs 163/164) · `d1c06cd9d` (the Scenario Builder) · the `d1c06cd9d` smalls ·
`b0b6656b5` (ten-commit) · `acadcc7cd` (the Concierge overhaul) · `97b25fc53`
(seven-commit + the text-side refusal seam).

## §0 Drift state

The ledger's §2 freshness probe **PASSED** at walk start (2026-09-29): v4
`main` **AT** the baseline `97b25fc53`, `bugfix` unmoved at `1a2b2164c`,
checkout on `main`, tree clean, **§3 EMPTY**. No step in this walk may blame
drift.

## §0.5 Pre-walk measurement (ledger §5.5 — before the first boot)

Read-only, on the copy, BEFORE v5 ever booted it.

| # | population | measured | consequence for the walk |
|---|---|---|---|
| M1 | copy vintage — `chats.renderedMarkdown` / `chats.conciergeOverride` present | **0 / 0** — both DROPPED; `migrations_state` 199 rows, newest `drop-chat-rendered-markdown-v1` 2026-09-26 | The copy is PAST `-dev.96`: the P4.D235 tolerant read runs its post-drop arm on real data. v5 must add NO `migrations_state` row (re-count after every boot). |
| M2 | first-boot re-embed cost (P4.D235 recipe) | `conversation_chunks` **17,350, 22 unembedded**; the arm-(B) set: **5 chunks / 3 chats / 70,150 chars** under `OpenAI Text 3 Large` (default) | ⭐ **v4 has ALREADY warmed live Friday** — the first boot enqueues ≤ 5 `EMBEDDING_GENERATE` jobs (≈ 17.5 K tokens, well under a cent). No zero-spend precaution needed. |
| M3 | Concierge states on real chats | `moderated` **887** (5 with refusals) · `unmoderated`/`concierge` **85** (3) · `unmoderated`/`operator` **13** · `locked`/`operator` **28** | ⭐ v4 has written all three states and both `setBy` values. The Concierge chips, the Locked guard and the refusal counters all have real subjects. Newest refusal: `11132b24…` "Specification for a Wearable Luz" (3, 2026-09-28). |
| M4 | `chat_informs` | **33** rows | Inform renders on real data. |
| M5 | FTS5 | `chat_messages_fts` **82,369** rows (148,472 messages); all five objects + three triggers present | Message search has a real index v4 built. |
| M6 | help docs by section | `help_docs` NULL embedding **0**; `help_doc_chunks` **738, 0 NULL** | ⭐ Dogfood #120 is closed ON LIVE DATA — v4's own section-embed reconcile has run. v5's boot reconcile should find nothing to do. |
| M7 | postboxes (`Mail/` in character vaults) | **39 letters in 9 vaults** — Charlie 11, **Friday 7**, Abigail 6, Ariel 5, Amy 5, the Librarian 2, Prospero/Leilani/Helene 1 | `read_mail` / `discard_mail` / letters-by-file-name have REAL letters — no plant needed. |
| M8 | built-in prompt templates | **27 rows, all `isBuiltIn`, newest `updatedAt` 2026-08-19** | v4 re-vendored 21 prompts at `c3eefa752` (2026-09) but its refresh is LAZY (runs on template reads), and nobody has opened the modal since — so v5's lazy refresh should fire `Built-in prompt template refreshed from shipped text` on the FIRST template read. |
| M9 | api keys | 11 providers: ANTHROPIC, DEEPSEEK, GOOGLE, GROK, MISTRAL, NANOGPT, OPENAI, OPENROUTER, SERPER, WAVESPEED, Z_AI | Live spend available on several wires. |
| M10 | stale lock | `quilltap.lock` from **2026-09-18** (v5's own last walk, PID 76331) + three 0-byte `-journal` files survived the refresh | The boot must reclaim the stale lock (a free re-proof); 0-byte journals are not hot. |

## §1 What is NOT live — do not report these as bugs

Collected from the eight rounds' records (full per-round research: status-log
round records `SL:137584`–`SL:155300`).

- **A bare `?action=` answers 400 `Unknown action: `** on every REST edge — the
  designed `dispatchAction` rule (P4.D220), not a regression.
- **`?action=has-dangerous` / `ChatsHasDangerous` are RETIRED** (v4 removed them).
- **The message danger-flag UI does not exist in v5** (badges, blur, "Not
  Dangerous", `override-danger-flag`) — the Display card is write-only. Old
  transcripts render the retired `manual-*` / `auto-flagged-refusals` kinds as
  ordinary `danger` rows; v5 never writes them.
- **The picture-arm 502 rides `kind: internal`** (ruled); the streamed retry is
  one broadcast stream per message id (recorded).
- **The refusal-ledger hook in `write_apply` is dormant**; a FLAT
  `{"code":"content_filter"}` image refusal is missed by the image failover
  (P4.118 item 12); an OpenAI stream that opens 200 then sends an `error` frame
  takes the EMPTY path (item 7); the OpenRouter-SDK 403 / Google 4xx triggers
  await a ruling.
- **Try uncensored answers 409 `no-understudy`** when the desk profile lacks a
  key or shares the original's model — v4-faithful.
- **The wardrobe TOOLS never fire the avatar trigger** (named follow-up).
- **The mail handlers PROPAGATE a read `Err`** where v4 answers "cannot find your
  postbox" (smalls order); `discard_mail` skips the protected-document check and
  the Librarian announcement (v4 property).
- **The transcript render's `Current time:` header is UTC** (standing convention).
- **`chatGet` projects `contextSummary` only** — the fold cursor is not on it.
- **Animated GIF/WebP inputs are stored ORIGINAL** (the ruled decline); a corrupt
  second frame keeps the first frame as a still WebP (ruled divergence).
- **`update_message` NULL-content repair ERRORs** on v5 (pinned divergence).
- **`quilltap sync` of a real photo reports `create` every run** (v4-faithful
  convergence defect, upstream-filing candidate); sync holds the writer for the
  whole run (ruling pending).
- **`?action=reattribute` answers 501** (named deferral).
- **Scenario Builder:** `curlConfigured` is always false; a pre-stream driver
  failure is a JSON 500 (both recorded).
- **The three text-compression migrations, image re-encode, and the stored
  `renderedMarkdown` reclamation stay DEFERRED** — no compaction on v5 boot.
- **`combined.log` bag keys are snake_case** (tree-wide, named once).

## §2 Server launch

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core::tools=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```
`quilltap::=debug` catches the custom targets (`quilltap::boot`,
`quilltap::help`, `quilltap::concierge_refusal`, the post office) without the
per-query noise of the whole `quilltap_core` tree.

## §3 The walk

Status: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### A — Boot (zero spend beyond ≤ 5 embeds)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| A1 | CLAUDE | boot on the copy | stale-lock reclaim in the lock's history; no panic | **PASS** — the lock's history reads `stale-detected … PID 76331 is no longer running` → `stale-claimed … Claimed by PID 86554`; no panic. The copy has NO passphrase, so the engine unlocked at boot (no human step). |
| A2 | CLAUDE | read boot log | `Conversation render reconciliation: found incomplete conversations count=` ≤ 3 then `…complete`; ≤ 5 `EMBEDDING_GENERATE` jobs | **FAIL(#121)** — `count=3` then `complete incomplete_chats=3 enqueued=3`, as predicted; but the three renders then enqueued **39** embeds (17 + 15 + 7 — EVERY interchange of the three chats), not ≤ 5. Cause: v5 renders the transcript's `Past conversation message timestamp:` lines in **UTC** (`September 29, 2026 at 1:35 AM` for `2026-09-29T01:34:14Z`) where v4 renders in the host zone (8:34 PM CDT), so every re-rendered chunk's text differs from v4's, and the v4-faithful upsert (content changed → `embedding = null`) re-embeds all of them. Spend trivial (~$0.01); the class is not — see #121. The P4.D235 recipe's "step 2 is the upper bound" is also wrong: the bound is every chunk of the incomplete chats. |
| A3 | CLAUDE | read boot log | FTS5 reconciler: DEBUG `Message search index counts`; NO rebuild WARN expected (v4 built it) — if it rebuilds, record the duration | **PASS** — `Message search index counts context="startup.chat-message-fts-reconcile" eligible=82369 indexed=82369`; no rebuild. |
| A4 | CLAUDE | read boot log | help reconcile: `Help docs reconciled` with nothing to embed (M6) | **PASS** — `[HelpDocSync] Sync completed totalOnDisk=129 … unchanged=129`; `Help docs reconciled … unchanged=129 sectionsBackfilled=0 incomplete=0`. |
| A5 | CLAUDE | `migrations_state` count after boot | still **199** | **PASS** — 199 after boot. |

### B — Reads over real data (zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| B1 | CLAUDE | open the Salon list; hover a chat card's Scriptorium badge | badge in one of three states; the reworded `qt-tooltip` string (aria-label) | **PASS** — 911 badges: **910** `Scriptorium: transcribed and indexed, every word findable — click to re-render` + **1** `…transcribed, the indexing still under way — click to re-render` (the boot's re-embeds still draining). Keyboard focus opens the `qt-tooltip` bubble with the same bytes (screenshot). ⚠ Instrument: the pane's synthetic hover sends no `pointerenter`, so a hover never opens a `qt-tooltip` — focus or a dispatched `pointerenter` does. |
| B2 | CLAUDE | open the LARGEST chat; wheel-scroll up | compressed rows decode; no `qt_text` error in the log | **FAIL(#123) → FIXED** — "The Bridge of Ordinary Breathing" (`e59a1b8c…`, 4,460 rows, 1,727 compressed; 2,028 projected messages, 9.0 MB): the Salon sat on *Loading chat…* and `chatGet` took **13.1 s** (curl, twice). Decode was NOT the cost (`qt_text` over the whole chat: ~40 ms). `sample` put 4,737 of 4,745 request-thread samples under ONE `sqlite3_step` in `project_chat_transcript` → `resolve_message_attachments`, i.e. the per-message `files` × `json_each(linkedTo)` probe (v4's `findByLinkedTo` shape, faithful), with the CPU in `chacha20_block`/`poly1305` — page re-DECRYPTION: v5 opened every connection with SQLite's ~2 MB default cache where v4 sets `cache_size = -64000` (64 MB) + `temp_store = MEMORY` on every connection. Fixed: after the fix `chatGet` = **1.10–1.15 s**, byte-identical length (9,013,666 B). No `qt_text` error anywhere. Wheel-scroll re-check (pane shown): the chat opened in 3.8 s including the full page load and tab restore; 60 real wheel ticks carried the transcript from 9:36 AM back to 7:34 AM with rows loading as it went. |
| B3 | CLAUDE | message search for a real phrase | FTS5 results returned over the real index | **PASS** — `uiSearch {q:"Zbinden", types:"messages"}` → 200, `totalCount: 22`, `countsByType {messages: 22}`, first hit a Librarian précis in "The Trustee's Head Count"; **84 ms**. |
| B4 | CLAUDE | open a Locked chat, an Unmoderated/concierge chat, and "Specification for a Wearable Luz" | sidebar state select + header pill with refusal count 3; chat GET carries `conciergeState`/`SetBy`/`Reason`/`RefusalCount` | **PASS** — chat GET: "Wearable Luz" `unmoderated`/`concierge`/`refusals`/**3** (a REAL v4 auto-switch); "Brass Ring" `locked`/`operator`/`migration`/0; neither carries `conciergeOverride` or `renderedMarkdown`. Header: an `eye-off` "Unmoderated" pill whose bubble reads *"The Concierge moved this chat to the uncensored desk after three refusals. Set it back to Moderated if you disagree."*; "Brass Ring"'s header shows a "Locked" pill. Sidebar Chat section: a flat `Moderated|Unmoderated|Locked` select, value `unmoderated`, enabled. v4-written `The Concierge · provider refusal` and `· danger` chips render in the transcript. |
| B5 | CLAUDE | open a chat with a `chat_informs` row | the inform chip renders; `chatInformsList` 200 | **PASS** — "The Brass Ring at the Bottom of the Pool" (8 informs, all consumed): `chatInformsList` → 200 `{batches: []}` — v4's list is pending-only too (`chat-informs.repository.ts:70` `.filter(r => !r.consumedAt)`). The record rows are Host whispers to one seat, so they are HIDDEN until **All Whispers** is on — v4's `whisper-visibility.ts` is the same rule (only `announcement` is exempt as operator-authored). With All Whispers on, `1ac4846b…` renders as *The Host · out of character · whispered to Amy · 4:28 PM*. **Recorded as a v4-faithful UX gap (candidate upstream note, #122):** the operator cannot see their own consumed informs without the global toggle, where their own announcements are exempted for exactly that reason. |
| B6 | CLAUDE | Settings → The Concierge tab | third tab, five cards, values from the real `conciergeSettings` | **PASS** — `/settings?tab=concierge` opens the Foundry on the Concierge tab with the five cards *On Duty / The Uncensored Desk / When a Provider Refuses / Display / Pre-Screening (Advanced)*; every control equals the stored row: on duty ✓, text desk `32053cdb…` (NanoGPT DeepSeek V4 Flash Latest), image desk `425d7fe4…` (WaveSpeed Qwen), vision `c19c142c…` (Z.AI GLM 4.6V), image-prompt `32053cdb…`, auto-switch **3**, new chats `moderated`, display `SHOW` + badges ✓. |
| B7 | CLAUDE | open Import-from-Template modal (Prompts) | log `Built-in prompt template refreshed from shipped text` × the stale rows; `updatedAt` moves | **PASS** — the first `promptTemplateList` (13 ms) logged **21** `Built-in prompt template refreshed from shipped text … source=plugin` (the whole shipped catalogue — the copy's rows predated `c3eefa752`); `updatedAt` moved on exactly those 21 of the 27 built-ins; a second read refreshed **nothing** (count stays 21). Driven by the dispatch read the modal issues. |
| B8 | CLAUDE | a project's chat list | the "Remove from project" corner control sits in the card, no blank line box | **PASS (layout, measured)** — "The Estate" project (`/prospero/024526a8…`): 20 cards carry the control; each sits 9 px from the card's top and right, the title row starts at 25 px, card height 138 px; the chain is `BUTTON` (static) inside `QT-TOOLTIP` **`position: absolute`, `display: flex`** inside the relative `A` — out of flow, so no line box above the content row (the unification's 82-vs-58 px regression is gone). Measured from the DOM with the pane hidden; no screenshot. |
| B9 | CLAUDE | Salon sidebar → Images switch on an image-bearing chat | the section always offered; hides/reveals images with the stand-ins | **PASS** (re-run once the pane was shown) — the switch lives in the user menu's quick-hide *Content Filters* as **Salon Images**. On "The Middle Is Known" (`7feb10bf…`): OFF → the Salon's **11** `<img>` drop to **0** (avatars become initial stand-ins `C` / `L` / `A`, the story background clears) while the **16** images outside the Salon stay (screenshot); ON → 11 again. |

### C — Refusal-armed edges (zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| C1 | CLAUDE | `DELETE /api/v1/chats/<throwaway>?action=` | 400 `Unknown action: ` + `availableActions`; chat NOT deleted | **PASS** — on "Hawking radiation from black holes" (`9c4315eb…`, 3 messages): `400 {"error":"Unknown action: ","availableActions":["reset-state","stop-impersonate"]}`; `chatGet` afterwards 200 — the chat survives. `GET /api/v1/chats?action=has-dangerous` → `400 Unknown action: has-dangerous` (retired). |
| C2 | CLAUDE | `chatUpdate` with a retired `conciergeState` (`uncensored`) | 400 | **PASS** — `400 bad-request "Validation error"`. |
| C3 | CLAUDE | settings PUT with a retired concierge key | 400 `Invalid settings: … replaced by conciergeSettings` | **PASS** (over dispatch) — `chatSettingsUpdate {dangerousContentSettings}` → *"Invalid settings: dangerousContentSettings was replaced by conciergeSettings"*; with `cheapLLMSettings.imagePromptProfileId` too → *"… were replaced …"*. v5 has **no REST edge** for `/api/v1/settings/chat` (405) — the SPA uses the dispatch verb; the same REST-gap class as the image-profiles edge, recorded not filed. |
| C4 | CLAUDE | `chatInform` on a chat with no LLM seat / an unknown target | `No LLM-controlled seat to inform.` / `Unknown inform target(s)…` | **PASS** — unknown id → `400 Unknown inform target(s) for this chat: 0000…`; Charlie's user seat → `400 Not an LLM-controlled seat in this chat: 32aac8b3…`; `chatInformCancel` of an unknown batch → `404 Inform batch not found`. (The no-LLM-seat arm not driven — no such real chat handy.) |
| C5 | CLAUDE | `messageRetryUncensored` on a USER message; `chatRetryImageUncensored` on a Locked chat | 400 `Only assistant messages can be retried`; 409 `locked` | **PASS** — USER message → `400 Only assistant messages can be retried`; in the Locked "Brass Ring" chat, both `chatRetryImageUncensored {kind:'background'}` and `messageRetryUncensored` on an assistant reply → `409 conflict "locked"`. |
| C6 | CLAUDE | `quilltap db` against a bogus `--data-dir` | v4's per-target `Cannot open database: …` | **PASS** — an empty instance and a nonexistent dir both answer v4's `Database not found: <dir>/data/quilltap.db`, exit 1 (Tier R pins the per-target strings byte-for-byte; `Cannot open database:` is the present-but-unopenable arm). |

### D — The Post Office on real letters (zero spend — tool runs, no LLM)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| D1 | CLAUDE | `chatRunTool list_mail` as Friday | her 7 letters by FILE NAME | **PASS** — in "The Trustee's Head Count" (`c22d1a65…`), `private: true`: *"Your postbox holds 7 letters, newest first. (Each letter is named by its file name — hand that to read_mail, discard_mail, or send_mail's in_reply_to.)"*, each with its `read_mail` / `send_mail(in_reply_to)` / `discard_mail` handles. ⚠ The dates render in UTC (`August 30, 2026 at 07:40 PM` for 14:40 CDT) — #121's class. |
| D2 | CLAUDE | `read_mail` by bare file name, then by `qtap://self/Mail/…` | letter body; `read_mail: letter read` with `markedAlerted`; the frontmatter rewritten | **PASS** — bare `1781515924666-from-charlie.md` and `qtap://self/Mail/1781517188982-from-abigail` (no `.md`) both read, DEBUG `read_mail: letter read … path="Mail/…"`. Every real letter was already announced, so Friday **`send_mail`ed Amy a test letter** (`in_reply_to` an Abigail letter): delivered as `Mail/1790708463074-from-friday.md` with `alerted: false` + `inReplyTo` + the nested reply preface; Amy's `read_mail` logged `markedAlerted=true` and the stored frontmatter now reads `alerted: true` (the rewrite mints a new `fileId`, as every content write does). ⚠ The v5-written reply preface **persists** the UTC time (`In reply to your letter of August 30, 2026 at 07:40 PM`) — #121. |
| D3 | CLAUDE | `read_mail` on `../x` and on a missing name | refused / `read_mail: no such letter` | **PASS** — `../quilltap.dbkey` and `Mail/sub/x.md` → *"Name the letter by its file name alone — the Post Office will not rummage outside your postbox."*; `1700000000000-from-nobody.md` → *"No letter named … rests in your postbox. list_mail will show you what does."* + DEBUG `read_mail: no such letter`. |
| D4 | CLAUDE | `discard_mail` one letter | `discard_mail: letter discarded`; the file link gone from `doc_mount_file_links` | **PASS** — Amy discarded the test letter: *"…has been consigned to the wastepaper basket."*, INFO `discard_mail: letter discarded`, link count 0. |
| D5 | CLAUDE | `list_email` (old name) | unknown tool — no alias | **PASS** — `Unknown tool: list_email`. |

### E — State changes + write paths (zero or near-zero spend)

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| E1 | CLAUDE | flip a throwaway chat Moderated → Unmoderated → Locked in the sidebar | `chats.conciergeMode`/`SetBy=operator`/`Reason=manual`; Host `danger` rows `set-unmoderated`, `set-locked` | **PASS** (dispatch `chatUpdate {conciergeState}` — the verb the sidebar select sends; the pane was hidden) on `9c4315eb…`: → `unmoderated/operator/manual`, → `locked/operator/manual`, → `moderated/null/null` (v4's 887 moderated rows carry the same NULLs). Each flip wrote one `concierge`/`danger` announcement (*"By the operator's own hand, the Concierge has been sent away…"*, *"The operator has locked the present company…"*, *"…the conversation is Moderated once more."*) with an `opaqueContent` operator advisory — the same column shape as v4's own auto-switch row on "Wearable Luz". (`set-*` are writer kinds, not a stored column.) |
| E2 | CLAUDE | New Chat with a non-default Concierge state | created in that state | **PASS** — `chatCreate` Friday (llm, her default Z.AI GLM 5.3 Flash) + Charlie (user), `conciergeState: 'unmoderated'` → `265bdad7…` at `unmoderated/operator/manual`, the Concierge announcement first, then the Prospero / Host / Aurora preamble and a greeting. The greeting's `llm_logs` row is **NANOGPT `deepseek/deepseek-v4.1-flash`** — the uncensored DESK, not Friday's profile — which is v4's `routeDirect: true` for `unmoderated` (`resolver.service.ts:241-255`): v4-faithful. |
| E3 | CLAUDE | edit a message | single UPDATE; FTS still finds the new text, not the old | **PASS** — `messageEdit` on the greeting (`fe412ffc…`) appending `Quuxwaddle` → `uiSearch` 1 hit; a second edit to `Frobnostic` → old 0 / new 1; `ftsId` **83790** and `rowid` **176976** unchanged across both edits (a true UPDATE under `_au`, not DELETE+INSERT). |
| E4 | CLAUDE | rename a chat by hand, then regenerate-title | `Title regenerated` … and a hand-rename honoured by the fold path | **PASS (regenerate arm)** — `chatUpdate {title:'Hand-Named by the Operator'}` then `chatRegenerateTitle` → 200 `{success, title:"The Concierge Departs, the Door Stands Open"}`; INFO `[Auto Title] Chat retitled … source="regenerate" from="Hand-Named by the Operator"` + `[Chats v1] Title regenerated … outcome="applied"` (an explicit regenerate SHOULD override a hand name). The retitle also **queued a Lantern story background** (`[Auto Title] Queued story background generation`) → routed direct to the uncensored image desk (`Unmoderated chat: routed direct to the uncensored desk`) → **failed: `Unknown image provider: WAVESPEED`** — see #124. The fold-time hand-rename guard needs real folds; left to F4. |
| E5 | CLAUDE | add a participant to a chat; reactivate one | `[Chats v1] Participant added` + the avatar-refresh DEBUG (`participant-join`); avatar job enqueue only if the trigger is on | **PASS (INFO arms)** — `chatAddParticipant` Amy → INFO `[Chats v1] Participant added … participant_id="97e0b0bb…" character_name=Amy controlled_by=llm`; `chatRemoveParticipant` then add again → INFO `[Chats v1] Participant reactivated` under the SAME participant id, status `active`. No `[CharacterAvatar]` enqueue (the chat's `avatarGenerationEnabled` is NULL → off). The two `refresh_avatar_for_arriving_character` DEBUG arms target `quilltap_core::api::chat_cast`, outside the walk's `quilltap::=debug` filter — not observed, not claimed. (⚠ Instrument: the chat GET's participant projection nests `character` — no top-level `characterId` — which first read as "Amy missing".) |
| E6 | CLAUDE | upload a still image to a Scriptorium store; then an animated GIF | still → WebP with the OUTPUT size on `files`; GIF kept original | **PASS** — `POST /api/v1/mount-points/<Friday vault>/blobs` (ffmpeg `testsrc` images): `still.png` (7,011 B) → 201 `storedMimeType: image/webp`, `relativePath: Dogfood/still.webp`, `originalMimeType: image/png`, `doc_mount_files.fileSizeBytes` **11,354** (the OUTPUT); 3-frame `anim.gif` (18,430 B) → 201 `image/gif`, `Dogfood/anim.gif`, 18,430 — stored ORIGINAL (the ruled animated decline, P4.108). |

### F — Live LLM, a handful of calls each

| # | owner | gesture | expected + evidence | status |
|---|---|---|---|---|
| F1 | CLAUDE | send one message in a real chat | turn completes; content stored compressed if ≥ 512 B | **PASS** — `chatSend` as Charlie in the walk chat (`265bdad7…`, Unmoderated) → 200 in 40 s; Friday's reply `fd47d411…` routed direct to the desk (`[DangerousContent] Rerouted to uncensored provider (Unmoderated chat) original_profile=Z.AI GLM 5.3 Flash uncensored_profile=DeepSeek V4 Flash Latest`) and stored **compressed** — `hex(substr(content,1,3)) = 510101`, 1,018 text → 560 stored. The chain then ran Amy's turn — see #125. |
| F2 | CLAUDE | Inform one seat, then send a turn | chip; next turn consumes (`consumedAt` set); the injected block in the `llm_logs` request | **PASS** — `chatInform` to Friday's seat ("mention the word periwinkle") → `batchId 1acf3002…` + the Host `inform` record; `chatInformsList` 1 pending. The turn consumed it (`consumedAt 19:10:51.408Z`, `consumedByMessageId = fd47d411…`); Friday's logged request (decoded with `qt_text`) carries it as ONE `system` message at index **2**, right after the two system blocks; the reply says *periwinkle*. |
| F3 | CLAUDE | streamed swipe (re-roll) on an assistant message | the `Regenerating…` plate beats; new variant selected; the swipe row carries tokens | **PASS (wire)** — `messageSwipe {stream:true}` on `fd47d411…` → 200 `{message}` in 12 s; an `/api/events` listener caught **295** `swipeProgress` frames keyed `progressId = fd47d411…`, the status beats in v4's order *Regenerating — gathering Friday's memories and context... → Regenerating — sending to Friday... → Regenerating Friday's reply... → Regenerating — filing the new line...*, reasoning + content frames, then `{done, message}`. The variant `dcccc235…` (`swipeIndex 1`) persisted `tokenCount 8716 / promptTokens 8423 / completionTokens 293`, `rawResponse`, `reasoningContent`, and **re-applied the consumed inform** (says *periwinkle* again). No `llm_logs` row for the re-roll and ~⅓ the prompt tokens (no tools) — both v4's shape (`regenerate-swipe.service.ts` calls `provider.streamMessage` directly, *Single generation, no tools*, no `logLLMCall`). The SPA plate itself not seen — pane hidden. |
| F4 | CLAUDE | Organize → Rebuild Summary… | confirm text + toast; `Context summary cleared for rebuild`; a fold job; DEBUG `Resolved speaker names for fold` | **PASS (server)** — `chatRebuildSummary` on "Ten Courses at Masque" (`2639be66…`, 74 messages, a 1,807-char v4 summary) → 200 `{success, jobId}`; INFO `[Chats v1] Context summary cleared for rebuild`; the priority `CONTEXT_SUMMARY` job refilled it in 13 s (1,667 chars, *"# Active threads - Charlie and Amy's first dinner at Masque…"*), `lastFullRebuildTurn` left at 0; both `SUMMARIZATION` requests label speakers by NAME (`[Charlie]…`, no `Participant …` fallback — bug 161); the fold's title pass `[Context Summary] Generated title … outcome="unchanged"`. On the new walk chat (too short to fold) the same verb cleared and completed in 16 ms. The speaker-names DEBUG target is outside the walk's filter; the confirm/toast strings need the pane. |
| F5 | CLAUDE | Scenario Builder from New Chat, one run, then save | `scenarioBuilderProgress` frames; `SCENARIO_BUILDER` llm_logs; saved scenario id resolves | **PASS (wire; save not driven)** — `scenarioBuilderBuild` (in-world, the Estate kitchen, Friday + Charlie, Z.AI GLM 5.3 Flash): **324** `scenarioBuilderProgress` frames keyed `progressId = runId`, the loop `reasoning → toolsDetected [search, doc_grep] → status → toolResult ×2 → reasoning → toolsDetected [search] → toolResult → reasoning → done`; `done` carries a 1,561-char scene, `toolsExecuted: 3`, `usage 29,663 / 1,118`, `webAvailable: false` (v4's `isScenarioWebAvailable` is `mode === 'real' && …` — correct for in-world); three `SCENARIO_BUILDER` `llm_logs` rows (7.5 s / 10.8 s / 25.9 s), one per round. The save dialog (`addScenario`, bug 165) needs the pane. |
| F6 | CLAUDE | `@` mention typeahead in the composer over a real cast; `:` after Shift+Enter | menu lists the cast; soft-break typeahead works | **PASS** (pane shown) — typing `@` in "The Middle Is Known" opens the menu with the five seated characters first, each marked *in this chat* (Abigail, Amy, Charlie, Friday, Laura), then the rest of the roster with titles (Anjali · The Mender, Ariadne · The Visiting Scholar, Ariel · Messenger-in-Residence…); `Lau` + Enter replaced `@Lau` with **Laura**; Shift+Enter then `:smi` → the editor holds `Laura<br>:smi` and the emoji typeahead is open (`:smiley:`, `:smile:`, …) — the P4.D224 soft-break fix holds on a real composer. (The rail's "4 characters" counts LLM seats only — v4's formula; five seated is right.) Composer cleared afterwards. |

### H — Human remainder (spend or judgment)

| # | owner | gesture | why human | status |
|---|---|---|---|---|
| H1 | CLAUDE (via a posed provider) | a real refusal → reroute to the uncensored desk + auto-switch after N (the instance's setting is **3**) | needs a prompt a real provider refuses | **PASS — against a POSED primary; the desk real.** No prompt short of genuinely dreadful makes a live model refuse, and the classifier never reads prose (only a provider error code, a moderation stop reason, or policy wording in the ERROR text — `refusal.rs`, v4's order), so the walk stood up `harness/tools/refusal-server.py` (Azure's `400 {"error":{"code":"content_filter",…}}`) behind an `OPENAI_COMPATIBLE` profile "REFUSAL probe (dogfood)" seated as Friday in a new **Moderated** chat `504096a6…`. Three turns: each `[Failover] Primary call failed` → WARN `[DangerousContent] Attempting uncensored retry` → INFO `Uncensored retry succeeded` (NANOGPT `deepseek/deepseek-v4.1-flash`) → INFO `Moderation refusal recorded … kind="text" purpose="chat"` (count 1, 2, 3) → DEBUG `Auto-switch check: below the threshold … threshold=3` at 1 and 2; at 3 the state write `unmoderated/concierge/refusals` + the Concierge's *"Three times now the house's regular staff have declined this conversation…"* (v4's bytes, as on "Wearable Luz"). Each reply's trail: `{REFUSAL probe, via: primary, outcome: refused, trigger: moderation-refusal, evidence: provider-code, detail: "code content_filter: 400 The response was filtered due to the prompt triggering Azure OpenAI's content management policy…"}` then `{DeepSeek V4 Flash Latest, via: concierge, outcome: answered}` — **P4.118's owed coded-4xx reroute, proven end to end** (posed body, real pipeline, real desk). No per-turn Concierge bubble on a TEXT reroute — v4 posts `refusal-rerouted` only from `image-failover.ts`. The greeting on the refusing primary fell back to the static line. Flipping the chat back to Moderated logged `Moderation refusal ledger reset`. |
| H2 | CLAUDE | Try uncensored (picture arm + message arm) | image spend | **PASS, both arms.** **Message:** `messageRetryUncensored` on a Moderated reply in "The Trustee's Head Count" (answered by DEEPSEEK `deepseek-v4-flash`) → INFO `[DangerousContent] Retrying a turn on the uncensored desk` → a new variant `swipeIndex 1` in the original's group, trail `{DeepSeek V4 Flash Latest, via: concierge, outcome: answered}`; on screen the row shows the desk's model badge and its hover text reads *"DeepSeek V4 Flash Latest · NANOGPT: deepseek/deepseek-v4.1-flash — sent by the Concierge; answered"*. **Picture (background):** `chatRetryImageUncensored {kind:'background'}` → 200 *Story background regeneration queued*; `Resolved the uncensored image retry understudy … excludedJson=["6de59b2d…"]` (the original GPT Image profile excluded) → `"Try uncensored": painting on the uncensored understudy` → NanoGPT `flux-2-dev` answered first time (19 s) → job completed. Only possible because the desk now points at the #124 NanoGPT profile. |
| H3 | CLAUDE | conceal-mode story background | image spend | **PARTIAL — the gate proven, the marker unreachable here.** Sanitizing runs only with the pre-screen ON (Moderated) or an Unmoderated scene that cannot reach the desk (`appearance_resolution.rs`, v4 `3b463d6b1`); this instance runs pre-screen OFF, so a moderated background logs `Appearance sanitization not applicable under this Concierge policy` (seen). With Pre-Screening ticked in the Concierge tab: "Trustee" (Friday clothed in that scene) classified `sexual 0.02`; the refusal-probe chat set back to Moderated (Friday `naked, barefoot`) → `APPEARANCE_RESOLUTION` → `DANGER_CLASSIFICATION` `sexual 0.0375`, below the 0.8 threshold, so no `Sanitizing appearances` / `flagged for cinematic concealment` — and the Concierge tab's threshold slider bottoms out at **0.1**, so the marker path cannot be forced through the UI with this appearance. The crafter's own concealment guidance (P4.D239) DID compose a draped picture: *"barefoot at the tall window… her bare shoulders and back to the room… a linen curtain and the counter's shadow concealing her"* → GPT Image 2.5 Flare. Pre-Screening turned back OFF afterwards (confirmed in the row). The marker arm stays owed: it needs an appearance the moderation endpoint scores ≥ 0.1. |
| H4 | HUMAN | the standing queue: Lantern budget, token-limit turn, the four planted proofs, dedup/summaries, Brahma deep query, #101, compression re-measure | spend / long runs | **DEFERRED-TO-HUMAN** — unchanged from the 2026-09-18 pass; (B9, F6 and B2's wheel re-check were run by Claude once the pane was shown.) |


## §4 Findings from this walk

| # | finding | class | disposition |
|---|---|---|---|
| **121** | v5 renders transcript/letter times in **UTC**; v4 renders in the **host zone**. On real data it moves money and meaning: every chat v4 rendered that v5 re-renders gets different chunk text, so every embedding is nulled and redone (39 embeds on first boot where 5 were owed; on an instance BOTH apps open, each re-render undoes the other's vectors), and recalled transcripts, `list_mail` / `read_mail` dates, and v5-WRITTEN reply prefaces carry a time five hours off (`August 30, 2026 at 07:40 PM` for 2:40 PM CDT) | A **standing, deliberate convention** (`conversation_markdown.rs` / `format_time.rs`: the differential pins both sides to `TZ=UTC`) whose production cost was never measured before | **ESCALATED to the human — a ruling, not a walk fix.** Options: (a) format in the host's local zone in production and keep `TZ=UTC` only as the harness's pin (v4's actual behaviour); (b) keep UTC and accept the re-embed churn + wrong-zone text. The Host's own time line (`The Host marks the time as … 2:10 PM`) already renders local — so v5 is internally inconsistent today. |
| **122** | A consumed Inform is invisible to the operator unless **All Whispers** is on | **v4-faithful** (`whisper-visibility.ts` exempts only `announcement` as operator-authored) | RECORDED — candidate v4 note: the operator's own informs deserve the same exemption as their own announcements. No v5 change. |
| **123** | The largest chat's GET took 13 s | **Port divergence** — v4's per-connection `cache_size = -64000` + `temp_store = MEMORY` never ported | **FIXED** `0246202cd` — 13.1 s → 1.1 s. |
| **124** | The instance's uncensored IMAGE desk (WaveSpeed Qwen) fails: `Unknown image provider: WAVESPEED` — every Unmoderated-chat Lantern/story background, every image reroute, and Try uncensored's picture arm | **Unported surface** — WaveSpeed is an npm-installed third-party plugin (`plugins/npm/@quilltap--qtap-plugin-wavespeed` on the instance), not in v4's tree; v5 has no npm-plugin loader (the P4.42 registry deferral) | RECORDED — a candidate order: either a native WaveSpeed image provider or the npm-plugin loader. Failure is loud and clean (ERROR + job failed; no spend). |
| **125** | In an Unmoderated chat, **chained follow-up turns go to the character's own (moderated) profile**, not the uncensored desk — only the directly-addressed reply is rerouted | **v4-faithful** — v4 gates `routeDirect && !isContinueMode && content` (`danger-orchestrator.service.ts:83`); v5 identical (`orchestrator.rs:1421`) | RECORDED — candidate v4 note: a turn chain in an Unmoderated chat may hit a moderated provider (a refusal then takes the ordinary failover). No v5 change. |

| **126** | The operator's own private tool runs read **"Prospero · tool run · to unknown"** in the chip strip (`announcement-group.ts` `whisperNames`) where the expanded row resolves them to "you" | **v4-faithful** — v4's `AnnouncementChip.tsx:32` also writes `participantNames?.[id] \|\| 'unknown'`; its bug-30 fix (`resolveWhisperTargetLabel`) reached `MessageRow` only | RECORDED — candidate v4 note (the chip should share `resolveWhisperTargetLabel`). No v5 change. |

Non-findings recorded so nobody re-files them: v4-written chat rows with `createdAt: ""` are skipped with `Skipping corrupted chat message` on every list read (P4.109, v4's own skip); 15 legacy whole-conversation chunks up to 2.7 M chars are refused by the embed cap (`Oversize: … exceeds 131072-char cap`, v4's rule); a swipe writes no `llm_logs` row and sends no tools (v4's shape); v5 has no REST edge for `/api/v1/settings/chat` (dispatch only).

### #124 — the NanoGPT Flux trial (the human's suggestion, 2026-09-29)

On the copy: an image profile **NanoGPT Flux 2 Dev (dogfood)** (`b9c8d354…`,
`NANOGPT` / `flux-2-dev`, the instance's NanoGPT key) created by
`imageProfileCreate`, flagged `isDangerousCompatible` (the Concierge's desk
select lists only such profiles — v4's rule; the profile first did NOT appear),
then chosen in **Settings → The Concierge → The Uncensored Desk → Image
profile** (saves on change; `conciergeSettings.uncensoredImageProfileId`
confirmed). A retitle of the Unmoderated walk chat queued the Lantern:
`[DangerousContent] Rerouting to uncensored image profile … NanoGPT Flux 2 Dev`
→ `Unmoderated chat: routed direct to the uncensored desk` → `Image call
answered first time … primary_provider=NANOGPT primary_model=flux-2-dev` →
job completed. The result is a 1248×832 WebP (185,612 B, `source: GENERATED`)
set as the chat's `storyBackgroundImageId` — three recognisable characters
in a steampunk kitchen, periwinkle envelope and all (looked at, not
described). **The image call took 101 s.** So #124 has a zero-code
workaround: point the desk at a native NanoGPT image model.
